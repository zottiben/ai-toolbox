//! Merging into files someone else also owns.
//!
//! `.mcp.json` and `.claude/settings.json` are the user's files, not the toolkit's: they
//! hold hand-written servers, permissions, model settings and statusline config that
//! nothing here knows about. So every merge adds and replaces by key and never rewrites
//! the document - the rule the bash heredocs followed, and the reason python3 was needed
//! in the first place, since the hook merge is a union keyed on two levels that jq alone
//! could not express.
//!
//! Key order is insertion order throughout (`serde_json`'s `preserve_order`), so merging
//! into a file does not reshuffle the parts of it nobody touched.

use serde_json::{Map, Value};

/// Add or replace servers in a `.mcp.json`-shaped document, by name.
///
/// Replace rather than deep-merge: a preset is one definition, and merging a new one
/// into the remains of an old would produce a server that matches neither - an `args`
/// list from one version with an `env` from another.
pub fn mcp_servers(document: &mut Value, servers: &Map<String, Value>) {
    let target = document
        .as_object_mut()
        .expect("an mcp document is an object")
        .entry("mcpServers")
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(target) = target.as_object_mut() else {
        return;
    };
    for (name, definition) in servers {
        target.insert(name.clone(), definition.clone());
    }
}

/// Layer extra keys onto servers that are already in the document.
///
/// Pi's `lifecycle` and `directTools` go on this way. Servers not already present are
/// skipped on purpose: this adds knobs to a definition, it does not create one.
pub fn overlay_server_keys(document: &mut Value, extra: &Map<String, Value>) {
    let Some(servers) = document
        .get_mut("mcpServers")
        .and_then(|s| s.as_object_mut())
    else {
        return;
    };
    for (name, keys) in extra {
        let Some(server) = servers.get_mut(name).and_then(|s| s.as_object_mut()) else {
            continue;
        };
        for (key, value) in keys.as_object().into_iter().flatten() {
            server.insert(key.clone(), value.clone());
        }
    }
}

/// Union the hook wiring for `installed` scripts into a settings document.
///
/// Three nested levels, each with its own rule, all of them load-bearing:
///
/// - **event** (`PreToolUse`, …): created only once a hook is actually going in, so
///   installing one hook does not leave four empty event keys behind.
/// - **matcher** (`"Bash"`, `"Write|Edit"`, or absent): matched on exactly. A second
///   matcher for the same event is appended rather than merged into the first.
/// - **command**: deduplicated. Re-running an install must not wire the same script
///   twice, which is the whole reason this is a union and not an append.
pub fn hooks(document: &mut Value, wiring: &Value, installed: &[String]) {
    let Some(events) = wiring.get("hooks").and_then(|h| h.as_object()) else {
        return;
    };
    let target_root = document
        .as_object_mut()
        .expect("a settings document is an object")
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(target_root) = target_root.as_object_mut() else {
        return;
    };

    for (event, matchers) in events {
        for matcher in matchers.as_array().into_iter().flatten() {
            let wanted: Vec<&Value> = matcher
                .get("hooks")
                .and_then(|h| h.as_array())
                .into_iter()
                .flatten()
                .filter(|hook| {
                    hook.get("command")
                        .and_then(|c| c.as_str())
                        .is_some_and(|command| {
                            installed.iter().any(|name| script_matches(command, name))
                        })
                })
                .collect();
            if wanted.is_empty() {
                continue;
            }

            let list = target_root
                .entry(event.clone())
                .or_insert_with(|| Value::Array(Vec::new()));
            let Some(list) = list.as_array_mut() else {
                continue;
            };

            // Found by index rather than by holding a reference, because the miss case
            // has to push onto the same list the hit case borrows from.
            let key = matcher.get("matcher");
            let at = match list.iter().position(|entry| entry.get("matcher") == key) {
                Some(at) => at,
                None => {
                    // Everything about the matcher except its hooks - which are filled
                    // in below from the subset that was actually installed.
                    let mut fresh = matcher.clone();
                    if let Some(object) = fresh.as_object_mut() {
                        object.insert("hooks".to_string(), Value::Array(Vec::new()));
                    }
                    list.push(fresh);
                    list.len() - 1
                }
            };

            let Some(hooks) = list[at].get_mut("hooks").and_then(|h| h.as_array_mut()) else {
                continue;
            };
            for hook in wanted {
                let already = hooks
                    .iter()
                    .any(|present| present.get("command") == hook.get("command"));
                if !already {
                    hooks.push(hook.clone());
                }
            }
        }
    }
}

/// Whether a wiring command runs one of the named scripts.
///
/// Compared on the basename: Claude Code's wiring is `${CLAUDE_PROJECT_DIR}`-prefixed
/// and Codex's is relative to the session root, and both have to match the same list.
fn script_matches(command: &str, script: &str) -> bool {
    command
        .split_whitespace()
        .next()
        .unwrap_or(command)
        .rsplit('/')
        .next()
        .is_some_and(|base| base == script)
}

/// Serialise the way the bash's `json.dump(..., indent=2)` did, newline included, so a
/// file the toolkit rewrites does not churn against one it wrote before.
pub fn to_string(document: &Value) -> String {
    format!(
        "{}\n",
        serde_json::to_string_pretty(document).expect("a json document serialises")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(text: &str) -> Value {
        serde_json::from_str(text).unwrap()
    }

    fn wiring() -> Value {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../hooks/settings.hooks.json");
        json(&std::fs::read_to_string(path).unwrap())
    }

    #[test]
    fn a_server_is_added_without_disturbing_what_was_already_there() {
        let mut document = json(r#"{"mcpServers":{"ours":{"command":"node"}},"other":{"keep":1}}"#);
        mcp_servers(
            &mut document,
            json(r#"{"context7":{"command":"npx"}}"#)
                .as_object()
                .unwrap(),
        );

        assert!(document["mcpServers"]["ours"].is_object());
        assert_eq!(document["mcpServers"]["context7"]["command"], "npx");
        assert_eq!(document["other"]["keep"], 1);
    }

    #[test]
    fn re_adding_a_server_replaces_it_rather_than_blending_two_versions() {
        let mut document = json(r#"{"mcpServers":{"s":{"command":"old","args":["--stale"]}}}"#);
        mcp_servers(
            &mut document,
            json(r#"{"s":{"command":"new"}}"#).as_object().unwrap(),
        );
        assert_eq!(document["mcpServers"]["s"]["command"], "new");
        assert!(
            document["mcpServers"]["s"].get("args").is_none(),
            "a leftover args from the old definition would match neither version"
        );
    }

    #[test]
    fn the_first_server_creates_the_mcp_servers_object() {
        let mut document = json("{}");
        mcp_servers(
            &mut document,
            json(r#"{"s":{"command":"x"}}"#).as_object().unwrap(),
        );
        assert_eq!(document["mcpServers"]["s"]["command"], "x");
    }

    #[test]
    fn pis_knobs_land_on_servers_that_exist_and_nowhere_else() {
        let mut document = json(r#"{"mcpServers":{"here":{"command":"npx"}}}"#);
        overlay_server_keys(
            &mut document,
            json(r#"{"here":{"lifecycle":"eager","directTools":true},"absent":{"lifecycle":"lazy"}}"#)
                .as_object()
                .unwrap(),
        );

        assert_eq!(document["mcpServers"]["here"]["lifecycle"], "eager");
        assert_eq!(document["mcpServers"]["here"]["directTools"], true);
        assert_eq!(document["mcpServers"]["here"]["command"], "npx");
        assert!(
            document["mcpServers"].get("absent").is_none(),
            "overlaying must not conjure a server with no transport"
        );
    }

    #[test]
    fn only_the_events_a_hook_needs_are_created() {
        let mut document = json("{}");
        hooks(
            &mut document,
            &wiring(),
            &["session-context.sh".to_string()],
        );

        let events: Vec<&String> = document["hooks"].as_object().unwrap().keys().collect();
        assert_eq!(events, vec!["SessionStart"]);
    }

    #[test]
    fn wiring_the_same_hook_twice_does_not_wire_it_twice() {
        let mut document = json("{}");
        let installed = vec!["format-on-edit.sh".to_string()];
        hooks(&mut document, &wiring(), &installed);
        let first = document.clone();
        hooks(&mut document, &wiring(), &installed);

        assert_eq!(document, first);
        assert_eq!(
            document["hooks"]["PostToolUse"][0]["hooks"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn two_hooks_sharing_a_matcher_end_up_in_one_entry() {
        let mut document = json("{}");
        hooks(
            &mut document,
            &wiring(),
            &[
                "guard-irreversible.sh".to_string(),
                "conventional-commit.sh".to_string(),
            ],
        );

        let bash = &document["hooks"]["PreToolUse"];
        assert_eq!(
            bash.as_array().unwrap().len(),
            1,
            "one Bash matcher, not two"
        );
        assert_eq!(bash[0]["matcher"], "Bash");
        assert_eq!(bash[0]["hooks"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn a_second_matcher_for_one_event_is_appended_rather_than_merged() {
        let mut document = json("{}");
        hooks(
            &mut document,
            &wiring(),
            &[
                "guard-irreversible.sh".to_string(),
                "protect-generated.sh".to_string(),
            ],
        );

        let entries = document["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(entries.len(), 2);
        let matchers: Vec<&str> = entries
            .iter()
            .map(|e| e["matcher"].as_str().unwrap())
            .collect();
        assert!(matchers.contains(&"Bash"));
        assert!(matchers.contains(&"Write|Edit"));
    }

    #[test]
    fn a_users_own_hook_on_the_same_matcher_survives() {
        let mut document = json(
            r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"./my-own-check.sh"}]}]},
                "permissions":{"allow":["Bash"]}}"#,
        );
        hooks(
            &mut document,
            &wiring(),
            &["guard-irreversible.sh".to_string()],
        );

        let commands: Vec<&str> = document["hooks"]["PreToolUse"][0]["hooks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| h["command"].as_str().unwrap())
            .collect();
        assert!(commands.contains(&"./my-own-check.sh"));
        assert!(commands
            .iter()
            .any(|c| c.ends_with("guard-irreversible.sh")));
        // Everything in the file that is not hooks is none of this merge's business.
        assert_eq!(document["permissions"]["allow"][0], "Bash");
    }

    #[test]
    fn an_event_with_no_matcher_key_is_handled_like_any_other() {
        let mut document = json("{}");
        hooks(
            &mut document,
            &wiring(),
            &["session-context.sh".to_string()],
        );

        let entry = &document["hooks"]["SessionStart"][0];
        assert!(entry.get("matcher").is_none());
        assert_eq!(entry["hooks"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn a_hook_that_was_not_installed_is_not_wired() {
        let mut document = json("{}");
        hooks(&mut document, &wiring(), &["format-on-edit.sh".to_string()]);

        let text = document.to_string();
        assert!(!text.contains("guard-irreversible"), "got {text}");
    }
}
