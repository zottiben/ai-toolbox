//! What a repo actually has on disk.
//!
//! Facts only: this module knows nothing about the catalogue and passes no judgement.
//! That separation is what lets `classify` compare a repo against the catalogue and
//! `worktree` compare two repos against each other, from the one reader.
//!
//! The layout it reads is the canonical one: a single copy of everything under
//! `.agents/`, plus `AGENTS.md` and `.mcp.json` at the root, with each harness's own
//! directory holding pointers into it.

use std::path::{Path, PathBuf};

use crate::catalogue::read_json;
use crate::error::{Error, Result};
use crate::hash::{self, Hash};
use crate::paths;

#[derive(Debug, Clone, serde::Serialize)]
pub struct Inventory {
    pub repo: PathBuf,
    pub agents_md: bool,
    pub claude_md: bool,
    pub skills: Vec<InstalledSkill>,
    pub hooks: Vec<InstalledHook>,
    pub helpers: Vec<InstalledHelper>,
    pub servers: Vec<InstalledServer>,
    pub claude: ClaudeView,
    pub codex: CodexView,
    pub pi: PiView,
    /// Per-harness copies from before the `.agents/` layout, as `_st_legacy` reports
    /// them. Their presence is the signal to migrate.
    pub legacy: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct InstalledSkill {
    pub name: String,
    pub path: PathBuf,
    pub hash: Hash,
    pub description: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct InstalledHook {
    pub name: String,
    pub path: PathBuf,
    pub hash: Hash,
    /// Cloning can drop the bit, and a hook that is not executable does not run - so it
    /// is a fact worth carrying rather than one to discover later.
    pub executable: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct InstalledHelper {
    pub name: String,
    pub path: PathBuf,
    pub hash: Hash,
    pub executable: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct InstalledServer {
    pub name: String,
    pub definition: serde_json::Value,
    pub hash: Hash,
    /// `.agents/mcp/*.sh` launchers this server's command depends on. A server whose
    /// helper is missing will not start, and the message it gives will not say so.
    pub helpers: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ClaudeView {
    pub settings: bool,
    pub skills: SkillsLink,
    pub wired: Vec<WiredHook>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CodexView {
    pub config: bool,
    pub servers: Vec<String>,
    pub wired: Vec<WiredHook>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PiView {
    /// `.pi/mcp.json` exists only for a server whose Pi definition would be wrong for
    /// Claude Code. Everything else Pi reads straight out of `.mcp.json`.
    pub overrides: Vec<InstalledServer>,
}

/// How `.claude/skills` is standing. Claude Code reads only its own directory but does
/// follow a symlink, which is why the canonical layout can have one copy at all.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum SkillsLink {
    Missing,
    /// Pointing somewhere. `resolves` is false for a dangling link - the failure that
    /// silently costs Claude Code every skill in the repo.
    Link {
        target: String,
        resolves: bool,
    },
    /// A real directory with its own copies, from before the layout was consolidated.
    OwnCopy {
        skills: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WiredHook {
    pub event: String,
    pub matcher: Option<String>,
    pub command: String,
    /// The script the command resolves to, which is what ties a wiring entry back to a
    /// file in `.agents/hooks`.
    pub script: String,
}

impl Inventory {
    pub fn read(repo: impl AsRef<Path>) -> Result<Inventory> {
        let repo = repo.as_ref().to_path_buf();
        let servers = read_servers(&repo.join(paths::SHARED_MCP))?;
        Ok(Inventory {
            agents_md: repo.join("AGENTS.md").is_file(),
            claude_md: repo.join("CLAUDE.md").is_file(),
            skills: read_skills(&repo.join(paths::AGENTS_SKILLS))?,
            hooks: read_hooks(&repo.join(paths::AGENTS_HOOKS))?,
            helpers: read_helpers(&repo.join(paths::AGENTS_MCP))?,
            servers,
            claude: read_claude(&repo)?,
            codex: read_codex(&repo)?,
            pi: PiView {
                overrides: read_servers(&repo.join(".pi/mcp.json"))?,
            },
            legacy: read_legacy(&repo),
            repo,
        })
    }

    pub fn skill(&self, name: &str) -> Option<&InstalledSkill> {
        self.skills.iter().find(|s| s.name == name)
    }

    pub fn hook(&self, name: &str) -> Option<&InstalledHook> {
        self.hooks.iter().find(|h| h.name == name)
    }

    pub fn helper(&self, name: &str) -> Option<&InstalledHelper> {
        self.helpers.iter().find(|h| h.name == name)
    }

    pub fn server(&self, name: &str) -> Option<&InstalledServer> {
        self.servers.iter().find(|s| s.name == name)
    }

    /// Nothing from the toolkit is here at all. Distinguishing this from "configured and
    /// broken" is the difference between a fresh worktree and a damaged one (D6).
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
            && self.hooks.is_empty()
            && self.servers.is_empty()
            && self.helpers.is_empty()
            && !self.agents_md
            && self.legacy.is_empty()
            && self.claude.wired.is_empty()
            && self.claude.skills == SkillsLink::Missing
    }

    /// Every hook wiring entry across the harnesses, with the harness that holds it.
    pub fn all_wired(&self) -> Vec<(crate::harness::Harness, &WiredHook)> {
        let claude = self
            .claude
            .wired
            .iter()
            .map(|w| (crate::harness::Harness::Claude, w));
        let codex = self
            .codex
            .wired
            .iter()
            .map(|w| (crate::harness::Harness::Codex, w));
        claude.chain(codex).collect()
    }
}

fn read_skills(dir: &Path) -> Result<Vec<InstalledSkill>> {
    let mut skills = Vec::new();
    for path in subdirectories(dir)? {
        // A directory without a SKILL.md is not a skill, however it got there.
        if !path.join("SKILL.md").is_file() {
            continue;
        }
        let manifest = path.join("SKILL.md");
        let text = std::fs::read_to_string(&manifest).map_err(|e| Error::io(&manifest, e))?;
        skills.push(InstalledSkill {
            name: file_name(&path),
            hash: hash::dir(&path)?,
            description: crate::catalogue::description_of(&text),
            path,
        });
    }
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(skills)
}

fn read_hooks(dir: &Path) -> Result<Vec<InstalledHook>> {
    let mut hooks = Vec::new();
    for path in files_with_extension(dir, "sh")? {
        let name = stem(&path);
        if name == "_lib" {
            continue;
        }
        hooks.push(InstalledHook {
            hash: hash::file(&path)?,
            executable: is_executable(&path)?,
            name,
            path,
        });
    }
    hooks.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(hooks)
}

fn read_helpers(dir: &Path) -> Result<Vec<InstalledHelper>> {
    let mut helpers = Vec::new();
    for path in files_with_extension(dir, "sh")? {
        helpers.push(InstalledHelper {
            name: file_name(&path),
            hash: hash::file(&path)?,
            executable: is_executable(&path)?,
            path,
        });
    }
    helpers.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(helpers)
}

/// The `mcpServers` of any file in `.mcp.json`'s shape. A missing file is no servers; a
/// malformed one is an error, because silently reporting zero servers for a file that is
/// there would hide exactly the breakage this tool is for.
fn read_servers(path: &Path) -> Result<Vec<InstalledServer>> {
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let value = read_json(path)?;
    let Some(map) = value.get("mcpServers").and_then(|s| s.as_object()) else {
        return Ok(Vec::new());
    };
    let mut servers: Vec<InstalledServer> = map
        .iter()
        .map(|(name, definition)| InstalledServer {
            name: name.clone(),
            hash: hash::json(definition),
            helpers: helper_references(definition),
            definition: definition.clone(),
        })
        .collect();
    servers.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(servers)
}

/// The `.agents/mcp/*.sh` launchers a server definition names anywhere inside it.
fn helper_references(definition: &serde_json::Value) -> Vec<String> {
    let mut found = Vec::new();
    let text = definition.to_string();
    let needle = format!("{}/", paths::AGENTS_MCP);
    let mut rest = text.as_str();
    while let Some(at) = rest.find(&needle) {
        let after = &rest[at + needle.len()..];
        let end = after
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.'))
            .unwrap_or(after.len());
        let name = &after[..end];
        if name.ends_with(".sh") && !found.contains(&name.to_string()) {
            found.push(name.to_string());
        }
        rest = &after[end..];
    }
    found.sort();
    found
}

fn read_claude(repo: &Path) -> Result<ClaudeView> {
    let settings_path = repo.join(".claude/settings.json");
    let wired = if settings_path.is_file() {
        wired_from_json(&read_json(&settings_path)?)
    } else {
        Vec::new()
    };
    Ok(ClaudeView {
        settings: settings_path.is_file(),
        skills: read_skills_link(&repo.join(".claude/skills"))?,
        wired,
    })
}

fn read_skills_link(path: &Path) -> Result<SkillsLink> {
    // symlink_metadata, not metadata: the whole point is to see the link itself, and a
    // dangling one makes `metadata` fail rather than report.
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return Ok(SkillsLink::Missing);
    };
    if meta.file_type().is_symlink() {
        let target = std::fs::read_link(path).map_err(|e| Error::io(path, e))?;
        return Ok(SkillsLink::Link {
            target: target.to_string_lossy().into_owned(),
            // metadata follows the link, so it failing is exactly a dangling link.
            resolves: std::fs::metadata(path).is_ok(),
        });
    }
    if meta.is_dir() {
        return Ok(SkillsLink::OwnCopy {
            skills: read_skills(path)?.len(),
        });
    }
    Ok(SkillsLink::Missing)
}

/// Flatten Claude Code's nested hook wiring into one entry per command.
fn wired_from_json(settings: &serde_json::Value) -> Vec<WiredHook> {
    let mut out = Vec::new();
    let Some(events) = settings.get("hooks").and_then(|h| h.as_object()) else {
        return out;
    };
    for (event, matchers) in events {
        for matcher in matchers.as_array().into_iter().flatten() {
            let matcher_text = matcher
                .get("matcher")
                .and_then(|m| m.as_str())
                .map(str::to_string);
            for hook in matcher
                .get("hooks")
                .and_then(|h| h.as_array())
                .into_iter()
                .flatten()
            {
                let Some(command) = hook.get("command").and_then(|c| c.as_str()) else {
                    continue;
                };
                out.push(WiredHook {
                    event: event.clone(),
                    matcher: matcher_text.clone(),
                    script: script_name(command),
                    command: command.to_string(),
                });
            }
        }
    }
    out.sort_by(|a, b| (&a.event, &a.script).cmp(&(&b.event, &b.script)));
    out
}

/// The script a hook command runs, whatever prefix it carries. Claude Code's wiring uses
/// `${CLAUDE_PROJECT_DIR}/...` and Codex's is cwd-relative, so comparing the basename is
/// the only comparison that works across both.
fn script_name(command: &str) -> String {
    command
        .split_whitespace()
        .next()
        .unwrap_or(command)
        .rsplit('/')
        .next()
        .unwrap_or(command)
        .to_string()
}

fn read_codex(repo: &Path) -> Result<CodexView> {
    let path = repo.join(".codex/config.toml");
    if !path.is_file() {
        return Ok(CodexView {
            config: false,
            servers: Vec::new(),
            wired: Vec::new(),
        });
    }
    let text = std::fs::read_to_string(&path).map_err(|e| Error::io(&path, e))?;
    // toml::from_str, never text.parse(): `FromStr` on `Value` parses a single TOML
    // *value*, so a document starting with a table header fails on its first line.
    let config: toml::Value = toml::from_str(&text).map_err(|e| Error::toml(&path, e))?;

    let mut servers: Vec<String> = config
        .get("mcp_servers")
        .and_then(|s| s.as_table())
        .map(|t| t.keys().cloned().collect())
        .unwrap_or_default();
    servers.sort();

    // Codex's hook tables carry the same shape as Claude's JSON, so they are converted
    // once and flattened by the same code rather than walked a second way.
    let wired = config
        .get("hooks")
        .and_then(|h| serde_json::to_value(h).ok())
        .map(|json| wired_from_json(&serde_json::json!({ "hooks": json })))
        .unwrap_or_default();

    Ok(CodexView {
        config: true,
        servers,
        wired,
    })
}

/// The same path means different things at the two levels: `.claude/hooks` inside a
/// repo is a pre-`.agents/` copy to migrate, while `~/.claude/hooks` is where Claude
/// Code's own user-wide hooks are supposed to live. Reporting the second as legacy would
/// tell someone to migrate a directory that is exactly where it belongs.
fn read_legacy(repo: &Path) -> Vec<String> {
    if is_user_root(repo) {
        return Vec::new();
    }
    paths::LEGACY_DIRS
        .iter()
        .filter(|dir| repo.join(dir).is_dir())
        .map(|dir| dir.to_string())
        .collect()
}

fn is_user_root(path: &Path) -> bool {
    let home = crate::machine::home();
    // Canonicalised on both sides so `/Users/x` and a symlinked `/home/x` are one place.
    std::fs::canonicalize(path).ok() == std::fs::canonicalize(&home).ok() && home.is_dir()
}

fn entries(dir: &Path) -> Result<Vec<PathBuf>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let read = std::fs::read_dir(dir).map_err(|e| Error::io(dir, e))?;
    let mut out = Vec::new();
    for entry in read {
        out.push(entry.map_err(|e| Error::io(dir, e))?.path());
    }
    out.sort();
    Ok(out)
}

fn files_with_extension(dir: &Path, extension: &str) -> Result<Vec<PathBuf>> {
    Ok(entries(dir)?
        .into_iter()
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == extension))
        .collect())
}

fn subdirectories(dir: &Path) -> Result<Vec<PathBuf>> {
    Ok(entries(dir)?.into_iter().filter(|p| p.is_dir()).collect())
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

fn is_executable(path: &Path) -> Result<bool> {
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::metadata(path).map_err(|e| Error::io(path, e))?;
    Ok(meta.permissions().mode() & 0o111 != 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Fixture;

    #[test]
    fn a_fresh_directory_is_empty_rather_than_broken() {
        let repo = tempfile::tempdir().unwrap();
        let inventory = Inventory::read(repo.path()).unwrap();
        assert!(inventory.is_empty());
        assert_eq!(inventory.claude.skills, SkillsLink::Missing);
    }

    #[test]
    fn reads_a_configured_repo_across_all_three_harnesses() {
        let fixture = Fixture::configured();
        let inventory = fixture.inventory();

        assert!(!inventory.is_empty());
        assert!(inventory.agents_md);
        assert_eq!(
            inventory.hook("format-on-edit").map(|h| h.executable),
            Some(true)
        );
        assert!(inventory.skill("pre-pr").is_some());
        assert!(inventory.server("context7").is_some());
        assert_eq!(inventory.codex.servers, vec!["context7".to_string()]);
        assert!(inventory.legacy.is_empty());
    }

    #[test]
    fn the_skills_symlink_is_reported_as_a_link_and_as_resolving() {
        let fixture = Fixture::configured();
        let inventory = fixture.inventory();
        match &inventory.claude.skills {
            SkillsLink::Link { target, resolves } => {
                assert_eq!(target, "../.agents/skills");
                assert!(resolves);
            }
            other => panic!("expected a link, got {other:?}"),
        }
    }

    #[test]
    fn a_dangling_skills_symlink_is_reported_as_not_resolving() {
        let fixture = Fixture::configured();
        std::fs::remove_dir_all(fixture.path().join(".agents/skills")).unwrap();
        match fixture.inventory().claude.skills {
            SkillsLink::Link { resolves, .. } => assert!(!resolves),
            other => panic!("expected a dangling link, got {other:?}"),
        }
    }

    #[test]
    fn a_real_claude_skills_directory_is_reported_as_its_own_copy() {
        let fixture = Fixture::configured();
        std::fs::remove_file(fixture.path().join(".claude/skills")).unwrap();
        fixture.write_skill(".claude/skills/local-only", "# local");
        assert_eq!(
            fixture.inventory().claude.skills,
            SkillsLink::OwnCopy { skills: 1 }
        );
    }

    #[test]
    fn hook_wiring_is_flattened_and_matched_by_script_name_across_harnesses() {
        let fixture = Fixture::configured();
        let inventory = fixture.inventory();

        let claude: Vec<&str> = inventory
            .claude
            .wired
            .iter()
            .map(|w| w.script.as_str())
            .collect();
        assert!(claude.contains(&"format-on-edit.sh"));
        // Claude's wiring is ${CLAUDE_PROJECT_DIR}-prefixed and Codex's is relative;
        // both have to reduce to the same script name or nothing can be cross-checked.
        let codex: Vec<&str> = inventory
            .codex
            .wired
            .iter()
            .map(|w| w.script.as_str())
            .collect();
        assert!(codex.contains(&"format-on-edit.sh"));
    }

    #[test]
    fn a_server_declares_the_helper_script_it_launches_through() {
        let fixture = Fixture::configured();
        fixture.write_mcp(serde_json::json!({
            "mcpServers": {
                "supabase": {
                    "command": ".agents/mcp/with-dotenv.sh",
                    "args": ["--need", "SUPABASE_ACCESS_TOKEN", "--", "npx", "server"]
                }
            }
        }));
        let inventory = fixture.inventory();
        assert_eq!(
            inventory.server("supabase").unwrap().helpers,
            vec!["with-dotenv.sh".to_string()]
        );
    }

    #[test]
    fn legacy_per_harness_directories_are_named() {
        let fixture = Fixture::configured();
        std::fs::create_dir_all(fixture.path().join(".claude/hooks")).unwrap();
        std::fs::create_dir_all(fixture.path().join(".codex/mcp")).unwrap();
        let legacy = fixture.inventory().legacy;
        assert!(legacy.contains(&".claude/hooks".to_string()));
        assert!(legacy.contains(&".codex/mcp".to_string()));
    }

    #[test]
    fn the_home_directory_is_not_a_repo_with_a_legacy_layout() {
        let home = tempfile::tempdir().unwrap();
        // Where Claude Code's user-wide hooks actually live.
        std::fs::create_dir_all(home.path().join(".claude/hooks")).unwrap();

        let inventory = crate::testing::with_home(home.path(), || Inventory::read(home.path()));
        assert!(
            inventory.unwrap().legacy.is_empty(),
            "~/.claude/hooks is where global hooks belong, not something to migrate"
        );
    }

    #[test]
    fn a_malformed_mcp_json_is_an_error_rather_than_zero_servers() {
        let repo = tempfile::tempdir().unwrap();
        std::fs::write(repo.path().join(".mcp.json"), "{ not json").unwrap();
        let err = Inventory::read(repo.path()).unwrap_err();
        assert!(matches!(err, Error::Json { .. }), "got {err:?}");
    }

    #[test]
    fn a_hook_without_its_executable_bit_is_reported_as_such() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = Fixture::configured();
        let hook = fixture.path().join(".agents/hooks/format-on-edit.sh");
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(
            fixture
                .inventory()
                .hook("format-on-edit")
                .map(|h| h.executable),
            Some(false)
        );
    }
}
