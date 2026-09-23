//! Installing, end to end.
//!
//! This file used to hold the comparison that made deleting `bin/ai-toolbox` safe: it
//! installed every hook, preset and skill twice - once through the bash and its python
//! helpers, once through this crate - and asserted the results matched. They did, byte
//! for byte, on every commit from the port until the bash was removed.
//!
//! That test is **gone rather than kept**, because `bin/ai-toolbox` is now a shim over
//! this engine, so the comparison would run the same code down both paths and pass
//! without testing a thing. A test that passes vacuously is worse than no test: it
//! reads like evidence. The evidence is in the history instead.
//!
//! What remains are the two properties that were never about the bash - that installing
//! twice changes nothing, and that a repo's own hand-written content survives.

use std::path::{Path, PathBuf};

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

/// A merge re-serialises the document, so a config with comments in it is refused rather
/// than rewritten without them. Reading the same file still works - only the write is
/// strict, because only the write can destroy something.
#[test]
fn a_merge_refuses_to_delete_the_comments_in_a_hand_written_config() {
    let catalogue = testing::catalogue();
    let temp = tempfile::tempdir().unwrap();
    let repo = seeded(temp.path().join("repo"));
    let mcp = repo.join(".mcp.json");
    let commented = format!(
        "{{\n  // our-internal is wired by hand, do not replace it\n{}",
        &EXISTING_MCP[2..]
    );
    std::fs::write(&mcp, &commented).unwrap();

    let error = install::everything(
        &repo,
        &catalogue,
        &HARNESSES,
        &[],
        &["context7".to_string()],
        &[],
        false,
    )
    .unwrap_err();

    assert!(
        matches!(error, ai_toolbox_core::Error::JsonComments { .. }),
        "got {error:?}"
    );
    assert!(
        error.to_string().contains("Remove them"),
        "the error should say what to do: {error}"
    );
    assert_eq!(
        std::fs::read_to_string(&mcp).unwrap(),
        commented,
        "a refused merge must leave the file exactly as it was"
    );

    // The same file is still readable - a comment stops a rewrite, not an inspection.
    let inventory = ai_toolbox_core::inventory::Inventory::read(&repo).unwrap();
    assert!(inventory.servers.iter().any(|s| s.name == "our-internal"));
}

fn seeded(repo: PathBuf) -> PathBuf {
    std::fs::create_dir_all(repo.join(".claude")).unwrap();
    std::fs::write(repo.join(".mcp.json"), EXISTING_MCP).unwrap();
    std::fs::write(repo.join(".claude/settings.json"), EXISTING_SETTINGS).unwrap();
    repo
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
