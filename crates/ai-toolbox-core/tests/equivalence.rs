//! The Rust engine against the bash one, on the same inputs.
//!
//! This is the test that makes deleting `bin/ai-toolbox` safe (PR8). It installs every
//! shipped hook, every preset and every skill twice - once through the bash and its
//! python helpers, once through this crate - and compares what lands on disk.
//!
//! Byte-for-byte everywhere except the generated Codex TOML, which is compared by
//! meaning. See D8 for why that one is not a byte comparison.
//!
//! It skips rather than fails where bash or python3 is missing, since the point is to
//! check the port, not to require the thing being replaced.

use std::path::{Path, PathBuf};
use std::process::Command;

use ai_toolbox_core::{action, install, testing, Harness};

const HARNESSES: [Harness; 3] = [Harness::Claude, Harness::Codex, Harness::Pi];

/// A hand-written `.mcp.json` and `settings.json` to merge onto, so the comparison
/// covers the merge rules rather than only the empty-file case.
const EXISTING_MCP: &str = r#"{
  "mcpServers": {
    "our-internal": {
      "command": "node",
      "args": [
        "tools/mcp.js"
      ]
    }
  }
}
"#;

const EXISTING_SETTINGS: &str = r#"{
  "permissions": {
    "allow": [
      "Bash"
    ]
  },
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "command": "./my-own-check.sh"
          }
        ]
      }
    ]
  }
}
"#;

#[test]
fn the_rust_engine_writes_what_the_bash_engine_writes() {
    let Some(bash_cli) = bash_available() else {
        eprintln!("skipping: bash or python3 is not available");
        return;
    };
    let catalogue = testing::catalogue();
    let hooks: Vec<String> = catalogue.hooks.iter().map(|h| h.name.clone()).collect();
    let presets: Vec<String> = catalogue.presets.iter().map(|p| p.name.clone()).collect();
    let skills: Vec<String> = catalogue.skills.iter().map(|s| s.key.clone()).collect();

    let temp = tempfile::tempdir().unwrap();
    let from_bash = seeded(temp.path().join("bash"));
    let from_rust = seeded(temp.path().join("rust"));

    // The bash side.
    run_bash(&bash_cli, &from_bash, "hooks", &hooks);
    run_bash(&bash_cli, &from_bash, "mcp", &presets);
    run_bash(&bash_cli, &from_bash, "skill", &skills);

    // The Rust side: three plans, each applied before the next is built, which is what
    // the three bash invocations above do. Planning all three first and applying them
    // together would be wrong, and the staleness guard says so - `.codex/config.toml`
    // takes hook wiring from one and MCP servers from another. `everything` is the way
    // to get those in a single plan.
    apply_now(planned(|plan| {
        install::hooks(plan, &from_rust, &catalogue, &hooks, &HARNESSES)
    }));
    apply_now(planned(|plan| {
        install::mcp(plan, &from_rust, &catalogue, &presets, &HARNESSES)
    }));
    apply_now(planned(|plan| {
        install::skills(plan, &from_rust, &catalogue, &skills, &HARNESSES, true)
    }));

    let expected = tree(&from_bash);
    let actual = tree(&from_rust);
    assert_eq!(
        expected, actual,
        "the two engines produced different file sets"
    );
    assert!(!expected.is_empty(), "the fixture installed nothing at all");

    for relative in &expected {
        let a = from_bash.join(relative);
        let b = from_rust.join(relative);

        if a.is_dir() && !a.is_symlink() {
            continue;
        }
        if a.is_symlink() {
            assert_eq!(
                std::fs::read_link(&a).unwrap(),
                std::fs::read_link(&b).unwrap(),
                "{relative} points somewhere different"
            );
            continue;
        }
        if relative.ends_with("config.toml") {
            // Generated output, compared by meaning rather than by bytes (D8).
            let parse = |path: &Path| -> toml::Value {
                toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
            };
            assert_eq!(parse(&a), parse(&b), "{relative} differs in meaning");
            continue;
        }
        assert_eq!(
            std::fs::read(&a).unwrap(),
            std::fs::read(&b).unwrap(),
            "{relative} differs"
        );
        assert_eq!(
            is_executable(&a),
            is_executable(&b),
            "{relative} differs in its executable bit"
        );
    }
}

#[test]
fn installing_twice_changes_nothing_the_second_time() {
    let catalogue = testing::catalogue();
    let temp = tempfile::tempdir().unwrap();
    let repo = seeded(temp.path().join("repo"));

    let hooks: Vec<String> = catalogue.hooks.iter().map(|h| h.name.clone()).collect();
    let presets: Vec<String> = catalogue.presets.iter().map(|p| p.name.clone()).collect();
    let skills: Vec<String> = catalogue.skills.iter().map(|s| s.key.clone()).collect();

    // One plan for the lot, which is what `bootstrap` and the board both build.
    let plan_all = || {
        install::everything(
            &repo, &catalogue, &HARNESSES, &hooks, &presets, &skills, true,
        )
        .unwrap()
    };

    let outcome = action::apply(&plan_all().actions).unwrap();
    assert!(
        outcome.applied > 0,
        "the first install should write something"
    );
    assert!(
        outcome.stale.is_empty(),
        "two actions in one plan collided on: {:?}",
        outcome.stale
    );
    let after_first = tree(&repo);

    // Re-planning against the installed repo must see everything as already in place.
    let again = plan_all();
    assert!(
        again.is_noop(),
        "re-planning found {} change(s): {:?}",
        again.changes(),
        again
            .actions
            .iter()
            .filter(|a| !a.noop)
            .map(|a| a.summary.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(action::apply(&again.actions).unwrap().applied, 0);
    assert_eq!(after_first, tree(&repo), "a no-op install changed the tree");
}

#[test]
fn a_hand_written_server_and_hook_survive_the_install() {
    let catalogue = testing::catalogue();
    let temp = tempfile::tempdir().unwrap();
    let repo = seeded(temp.path().join("repo"));

    let plan = install::everything(
        &repo,
        &catalogue,
        &HARNESSES,
        &["guard-irreversible".to_string()],
        &["context7".to_string()],
        &[],
        false,
    )
    .unwrap();
    action::apply(&plan.actions).unwrap();

    let mcp: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(repo.join(".mcp.json")).unwrap()).unwrap();
    assert_eq!(
        mcp["mcpServers"]["our-internal"]["command"], "node",
        "the repo's own server was lost"
    );

    let settings: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(repo.join(".claude/settings.json")).unwrap())
            .unwrap();
    assert_eq!(
        settings["permissions"]["allow"][0], "Bash",
        "settings unrelated to hooks were lost"
    );
    let commands: Vec<&str> = settings["hooks"]["PreToolUse"][0]["hooks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["command"].as_str().unwrap())
        .collect();
    assert!(
        commands.contains(&"./my-own-check.sh"),
        "the repo's own hook was dropped: {commands:?}"
    );
}

/// Build a plan from one planner call, for the cases that install a single group.
fn planned(build: impl FnOnce(&mut install::Plan) -> ai_toolbox_core::Result<()>) -> install::Plan {
    let mut plan = install::Plan::default();
    build(&mut plan).unwrap();
    plan
}

fn apply_now(plan: install::Plan) {
    let outcome = action::apply(&plan.actions).unwrap();
    assert!(
        outcome.stale.is_empty(),
        "stale on a fresh repo: {:?}",
        outcome.stale
    );
}

fn seeded(repo: PathBuf) -> PathBuf {
    std::fs::create_dir_all(repo.join(".claude")).unwrap();
    std::fs::write(repo.join(".mcp.json"), EXISTING_MCP).unwrap();
    std::fs::write(repo.join(".claude/settings.json"), EXISTING_SETTINGS).unwrap();
    repo
}

fn bash_available() -> Option<PathBuf> {
    let cli = testing::catalogue_root().join("bin/ai-toolbox");
    let python = Command::new("python3")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    (cli.is_file() && python).then_some(cli)
}

fn run_bash(cli: &Path, repo: &Path, command: &str, args: &[String]) {
    let output = Command::new(cli)
        .arg(command)
        .args(args)
        .arg("--repo")
        .arg(repo)
        .arg("--harness")
        .arg("all")
        .output()
        .expect("running bin/ai-toolbox");
    assert!(
        output.status.success(),
        "bin/ai-toolbox {command} failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Every path under a repo, relative and sorted. `.claude/skills` is a symlink to
/// `.agents/skills`, so the walk does not follow it - descending would list every skill
/// twice and never terminate on a loop.
fn tree(root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            found.push(relative);
            if path.is_dir() && !path.is_symlink() {
                stack.push(path);
            }
        }
    }
    found.sort();
    found
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
}
