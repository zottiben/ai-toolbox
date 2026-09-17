//! Repo fixtures, built the way the installer builds them.
//!
//! Every fixture is assembled from the *real* catalogue rather than from invented files.
//! That is deliberate: a test that installs a made-up hook proves the reader can read a
//! made-up hook, and would keep passing after a shipped hook changed shape. Building
//! from the clone means the fixtures move when the content moves.
//!
//! Only compiled for tests and for the other crates' tests, never shipped.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use crate::catalogue::Catalogue;
use crate::inventory::Inventory;
use crate::paths;

/// Run a closure with `$HOME` pointed somewhere else.
///
/// The environment is process-wide and tests run in parallel, so this serialises every
/// caller behind one lock. Without it, two tests swapping `$HOME` at the same time would
/// each see the other's value and fail in a way that looks like a bug in the code under
/// test. The lock is deliberately poison-tolerant: one failing test must not cascade
/// into every later one failing on a poisoned mutex.
pub fn with_home<T>(home: &Path, body: impl FnOnce() -> T) -> T {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard: MutexGuard<'_, ()> = LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let previous = std::env::var_os("HOME");
    std::env::set_var("HOME", home);
    let outcome = body();
    match previous {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    outcome
}

pub struct Fixture {
    dir: tempfile::TempDir,
}

/// The clone this crate lives in - the same catalogue the binary would read.
pub fn catalogue_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the crate is inside the clone")
}

pub fn catalogue() -> Catalogue {
    Catalogue::load(catalogue_root()).expect("the shipped catalogue")
}

impl Fixture {
    /// An empty directory - a repo nobody has run the toolkit in.
    pub fn bare() -> Fixture {
        Fixture {
            dir: tempfile::tempdir().expect("a temp dir"),
        }
    }

    /// A repo configured for all three harnesses, in the canonical layout, with content
    /// copied out of the real catalogue.
    pub fn configured() -> Fixture {
        let fixture = Fixture::bare();
        let catalogue = catalogue();
        let root = fixture.path();

        std::fs::write(root.join("AGENTS.md"), "# Agents\n").unwrap();
        std::fs::write(root.join("CLAUDE.md"), "@AGENTS.md\n").unwrap();

        for hook in ["format-on-edit", "session-context"] {
            fixture.install_hook(&catalogue, hook);
        }
        for skill in ["pre-pr", "handoff"] {
            fixture.install_skill(&catalogue, skill);
        }
        fixture.write_mcp(
            serde_json::from_str(
                r#"{"mcpServers":{"context7":{"command":"npx","args":["-y","@upstash/context7-mcp@latest"]}}}"#,
            )
            .unwrap(),
        );

        // Claude Code: pointers only.
        std::fs::create_dir_all(root.join(".claude")).unwrap();
        std::os::unix::fs::symlink(paths::CLAUDE_SKILLS_TARGET, root.join(paths::CLAUDE_SKILLS))
            .unwrap();
        fixture.write_json(
            paths::CLAUDE_SETTINGS,
            serde_json::json!({
                "hooks": {
                    "PostToolUse": [{
                        "matcher": "Write|Edit",
                        "hooks": [{
                            "type": "command",
                            "command": "${CLAUDE_PROJECT_DIR}/.agents/hooks/format-on-edit.sh"
                        }]
                    }],
                    "SessionStart": [{
                        "hooks": [{
                            "type": "command",
                            "command": "${CLAUDE_PROJECT_DIR}/.agents/hooks/session-context.sh"
                        }]
                    }]
                }
            }),
        );

        // Codex: its own generated file, with cwd-relative hook commands.
        fixture.write(
            paths::CODEX_CONFIG,
            r#"[[hooks.PostToolUse]]
matcher = "Write|Edit"

[[hooks.PostToolUse.hooks]]
type = "command"
command = ".agents/hooks/format-on-edit.sh"

[[hooks.SessionStart]]
[[hooks.SessionStart.hooks]]
type = "command"
command = ".agents/hooks/session-context.sh"

[mcp_servers.context7]
command = "npx"
args = ["-y", "@upstash/context7-mcp@latest"]
"#,
        );

        fixture
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn inventory(&self) -> Inventory {
        Inventory::read(self.path()).expect("reading the fixture")
    }

    /// Copy a hook out of the catalogue, executable bit and all.
    pub fn install_hook(&self, catalogue: &Catalogue, name: &str) {
        let hook = catalogue
            .hook(name)
            .unwrap_or_else(|| panic!("no hook {name}"));
        let dest = self.path().join(paths::AGENTS_HOOKS);
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::copy(&hook.path, dest.join(format!("{name}.sh"))).unwrap();
        make_executable(&dest.join(format!("{name}.sh")));
    }

    pub fn install_skill(&self, catalogue: &Catalogue, key: &str) {
        let skill = catalogue
            .skill(key)
            .unwrap_or_else(|| panic!("no skill {key}"));
        let dest = self.path().join(paths::AGENTS_SKILLS).join(&skill.name);
        copy_tree(&skill.path, &dest);
    }

    pub fn install_helper(&self, catalogue: &Catalogue, name: &str) {
        let helper = catalogue
            .helper(name)
            .unwrap_or_else(|| panic!("no helper {name}"));
        let dest = self.path().join(paths::AGENTS_MCP);
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::copy(&helper.path, dest.join(name)).unwrap();
        make_executable(&dest.join(name));
    }

    /// A skill that was never in the catalogue - the "Local" case from D5.
    pub fn write_skill(&self, relative: &str, body: &str) {
        let dir = self.path().join(relative);
        std::fs::create_dir_all(&dir).unwrap();
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        std::fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {body}\n---\n\n{body}\n"),
        )
        .unwrap();
    }

    pub fn write(&self, relative: &str, contents: &str) {
        let path = self.path().join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    pub fn write_json(&self, relative: &str, value: serde_json::Value) {
        self.write(
            relative,
            &format!("{}\n", serde_json::to_string_pretty(&value).unwrap()),
        );
    }

    pub fn write_mcp(&self, value: serde_json::Value) {
        self.write_json(paths::SHARED_MCP, value);
    }

    /// Append to a file that is already there, for the "someone hand-edited it" cases.
    pub fn append(&self, relative: &str, extra: &str) {
        let path = self.path().join(relative);
        let mut text = std::fs::read_to_string(&path).unwrap_or_default();
        text.push_str(extra);
        std::fs::write(path, text).unwrap();
    }

    pub fn remove(&self, relative: &str) {
        let path = self.path().join(relative);
        if path.is_dir() {
            std::fs::remove_dir_all(path).unwrap();
        } else {
            std::fs::remove_file(path).unwrap();
        }
    }
}

fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

pub fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let source = entry.path();
        let target = to.join(entry.file_name());
        if source.is_dir() {
            copy_tree(&source, &target);
        } else {
            std::fs::copy(&source, &target).unwrap();
        }
    }
}
