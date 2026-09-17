//! Turning a preset into Codex's `[mcp_servers.*]` tables.
//!
//! Codex is the one harness that cannot read the repo's canonical `.mcp.json` (verified
//! against 0.144.1: with the project trusted, only the `.codex/config.toml` server
//! appears), so its config is generated output rather than a second source to maintain.
//! Note that Codex ignores a project `.codex/` entirely until the project is trusted.
//!
//! The conversion rules, each verified against the Codex MCP docs and a live
//! `config.toml`, and carried over unchanged from `bin/lib/mcp_codex.py`:
//!
//! | preset | codex |
//! | --- | --- |
//! | stdio `command`/`args` | the same keys |
//! | `env {K: "${K}"}` (host passthrough) | `env_vars = ["K"]` |
//! | `env {K: "literal"}` | an `[mcp_servers.<name>.env]` table |
//! | `${VAR}` inside args | routed through `with-dotenv.sh`, which expands at launch |
//! | `command` already the wrapper | kept as-is |
//! | `type: "http"` + `url` | `url = "..."`, OAuth via `codex mcp login` |
//! | `headersHelper` (Claude-only) | `bearer_token_env_var` + a warning |

use serde_json::{Map, Value};

use super::{canonical_helper_path, is_ambient, passthrough_and_static, placeholders, WRAPPER};

/// One converted server, plus anything the user has to be told about it.
pub struct Converted {
    pub servers: Map<String, Value>,
    pub warnings: Vec<String>,
}

pub fn servers(presets: &[(&str, &Map<String, Value>)]) -> Converted {
    let mut out = Converted {
        servers: Map::new(),
        warnings: Vec::new(),
    };
    for (_, definitions) in presets {
        for (name, definition) in definitions.iter() {
            let (converted, warnings) = server(name, definition);
            out.servers.insert(name.clone(), converted);
            out.warnings.extend(warnings);
        }
    }
    out
}

pub fn server(name: &str, definition: &Value) -> (Value, Vec<String>) {
    let mut out = Map::new();
    let mut warnings = Vec::new();
    let env = definition.get("env").and_then(|e| e.as_object());
    let (passthrough, statics) = passthrough_and_static(env);

    let remote = definition.get("type").and_then(|t| t.as_str()) == Some("http")
        || definition.get("url").is_some();
    if remote {
        if let Some(url) = definition.get("url") {
            out.insert("url".to_string(), url.clone());
        }
        if let Some(helper) = definition.get("headersHelper").and_then(|h| h.as_str()) {
            // Codex has no per-request header hook. The nearest thing is a bearer token
            // read from the environment, which is a real behaviour change and so gets
            // said out loud rather than quietly applied.
            if let Some(var) = last_upper_token(helper) {
                out.insert(
                    "bearer_token_env_var".to_string(),
                    Value::String(var.clone()),
                );
                warnings.push(format!(
                    "{name}: Codex has no headersHelper equivalent for a remote MCP; mapped to \
                     bearer_token_env_var={var}. Export {var} in the environment you launch codex \
                     from (Claude Code reads it from the repo .env; Codex does not)."
                ));
            }
        }
        insert_env(&mut out, &passthrough, &statics);
        return (Value::Object(out), warnings);
    }

    let command = canonical_helper_path(
        definition
            .get("command")
            .and_then(|c| c.as_str())
            .unwrap_or(""),
    );
    let args: Vec<String> = definition
        .get("args")
        .and_then(|a| a.as_array())
        .into_iter()
        .flatten()
        .filter_map(|a| a.as_str())
        .map(canonical_helper_path)
        .collect();

    // Already going through the wrapper: it resolves ${VAR} itself, so there is nothing
    // to rewrite and wrapping it twice would only add a layer.
    if command.ends_with("with-dotenv.sh") {
        out.insert("command".to_string(), Value::String(command));
        out.insert("args".to_string(), string_array(&args));
        insert_env(&mut out, &passthrough, &statics);
        return (Value::Object(out), warnings);
    }

    let in_args = placeholders(&args);
    if !in_args.is_empty() {
        // Codex does not interpolate config strings, so a ${VAR} in args would reach the
        // server as six literal characters. The wrapper loads .env and expands them at
        // launch, which is the one bridge that works under both harnesses.
        let mut needs: Vec<String> = in_args.iter().filter(|v| !is_ambient(v)).cloned().collect();
        for var in &passthrough {
            if !needs.contains(var) {
                needs.push(var.clone());
            }
        }
        let mut wrapped: Vec<String> = Vec::new();
        for var in &needs {
            wrapped.push("--need".to_string());
            wrapped.push(var.clone());
        }
        wrapped.push("--".to_string());
        wrapped.push(command);
        wrapped.extend(args);

        out.insert("command".to_string(), Value::String(WRAPPER.to_string()));
        out.insert("args".to_string(), string_array(&wrapped));
        if !statics.is_empty() {
            out.insert("env".to_string(), Value::Object(statics));
        }
        warnings.push(format!(
            "{name}: routed through {WRAPPER} so ${{{}}} expand from the repo .env at launch \
             (Codex does not interpolate config strings).",
            in_args.join("}, ${")
        ));
        return (Value::Object(out), warnings);
    }

    out.insert("command".to_string(), Value::String(command));
    out.insert("args".to_string(), string_array(&args));
    insert_env(&mut out, &passthrough, &statics);
    (Value::Object(out), warnings)
}

/// Codex splits what Claude keeps in one `env` map: names to pass through from the host
/// go in `env_vars`, literal values go in an `env` table.
fn insert_env(out: &mut Map<String, Value>, passthrough: &[String], statics: &Map<String, Value>) {
    if !statics.is_empty() {
        out.insert("env".to_string(), Value::Object(statics.clone()));
    }
    if !passthrough.is_empty() {
        out.insert("env_vars".to_string(), string_array(passthrough));
    }
}

fn string_array(items: &[String]) -> Value {
    Value::Array(items.iter().map(|i| Value::String(i.clone())).collect())
}

/// The env var a `headersHelper` command reads, taken as the last all-caps word in it.
/// Last rather than first because the script path comes before its arguments.
fn last_upper_token(helper: &str) -> Option<String> {
    helper
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .rfind(|token| {
            !token.is_empty()
                && token
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
                && token.chars().any(|c| c.is_ascii_uppercase())
        })
        .map(str::to_string)
}

/// Add or replace `[mcp_servers.*]` in a parsed Codex config, by name - the same
/// overwrite-by-name rule as the `.mcp.json` merge.
pub fn merge_servers(config: &mut toml::Value, servers: &Map<String, Value>) -> crate::Result<()> {
    let table = config
        .as_table_mut()
        .expect("a codex config is a table")
        .entry("mcp_servers".to_string())
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    let Some(table) = table.as_table_mut() else {
        return Ok(());
    };
    for (name, definition) in servers {
        table.insert(name.clone(), super::to_toml(definition)?);
    }
    Ok(())
}

/// Union hook wiring into a parsed Codex config, by the same two-level rule as the
/// Claude merge: matchers matched exactly, commands deduplicated.
pub fn merge_hooks(config: &mut toml::Value, wiring: &Value) -> crate::Result<()> {
    let Some(events) = wiring.as_object() else {
        return Ok(());
    };
    let root = config
        .as_table_mut()
        .expect("a codex config is a table")
        .entry("hooks".to_string())
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    let Some(root) = root.as_table_mut() else {
        return Ok(());
    };

    for (event, matchers) in events {
        for matcher in matchers.as_array().into_iter().flatten() {
            let converted = super::to_toml(matcher)?;
            let Some(converted) = converted.as_table() else {
                continue;
            };
            let list = root
                .entry(event.clone())
                .or_insert_with(|| toml::Value::Array(Vec::new()));
            let Some(list) = list.as_array_mut() else {
                continue;
            };

            let key = converted.get("matcher");
            let at = match list.iter().position(|entry| entry.get("matcher") == key) {
                Some(at) => at,
                None => {
                    let mut fresh = converted.clone();
                    fresh.insert("hooks".to_string(), toml::Value::Array(Vec::new()));
                    list.push(toml::Value::Table(fresh));
                    list.len() - 1
                }
            };

            let incoming: Vec<toml::Value> = converted
                .get("hooks")
                .and_then(|h| h.as_array())
                .cloned()
                .unwrap_or_default();
            let Some(hooks) = list[at].get_mut("hooks").and_then(|h| h.as_array_mut()) else {
                continue;
            };
            for hook in incoming {
                let already = hooks
                    .iter()
                    .any(|present| present.get("command") == hook.get("command"));
                if !already {
                    hooks.push(hook);
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(text: &str) -> Value {
        serde_json::from_str(text).unwrap()
    }

    fn convert(text: &str) -> Value {
        server("s", &json(text)).0
    }

    #[test]
    fn a_plain_stdio_server_converts_to_the_same_keys() {
        let out = convert(r#"{"command":"npx","args":["-y","pkg@latest"]}"#);
        assert_eq!(out["command"], "npx");
        assert_eq!(out["args"][1], "pkg@latest");
        assert!(out.get("env").is_none());
    }

    #[test]
    fn host_passthrough_env_becomes_env_vars_and_literals_stay_a_table() {
        let out = convert(r#"{"command":"x","env":{"TOKEN":"${TOKEN}","MODE":"strict"}}"#);
        assert_eq!(out["env_vars"][0], "TOKEN");
        assert_eq!(out["env"]["MODE"], "strict");
        assert!(out["env"].get("TOKEN").is_none());
    }

    #[test]
    fn a_placeholder_in_args_is_routed_through_the_wrapper() {
        let (out, warnings) = server(
            "supabase",
            &json(r#"{"command":"npx","args":["--project-ref=${SUPABASE_PROJECT_REF}"]}"#),
        );
        assert_eq!(out["command"], WRAPPER);
        let args: Vec<&str> = out["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_str().unwrap())
            .collect();
        assert_eq!(args[0], "--need");
        assert_eq!(args[1], "SUPABASE_PROJECT_REF");
        assert_eq!(args[2], "--");
        assert_eq!(args[3], "npx");
        // The original arg survives untouched - the wrapper expands it at launch.
        assert_eq!(args[4], "--project-ref=${SUPABASE_PROJECT_REF}");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("does not interpolate"));
    }

    #[test]
    fn an_ambient_variable_is_expanded_by_the_wrapper_without_being_declared_a_need() {
        // Codex will not interpolate ${HOME} either, so it still has to go through the
        // wrapper - but --need HOME would be asking the user to put HOME in a .env.
        let out = convert(r#"{"command":"x","args":["${HOME}/bin/thing"]}"#);
        assert_eq!(out["command"], WRAPPER);
        let args: Vec<&str> = out["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_str().unwrap())
            .collect();
        assert_eq!(args[0], "--", "no --need for an ambient variable");
        assert!(args.contains(&"${HOME}/bin/thing"));
    }

    #[test]
    fn a_server_already_on_the_wrapper_is_not_wrapped_again() {
        let (out, warnings) = server(
            "s",
            &json(r#"{"command":".agents/mcp/with-dotenv.sh","args":["--need","T","--","npx"]}"#),
        );
        assert_eq!(out["command"], ".agents/mcp/with-dotenv.sh");
        assert_eq!(out["args"].as_array().unwrap().len(), 4);
        assert!(warnings.is_empty());
    }

    #[test]
    fn a_helper_path_from_the_old_per_harness_layout_is_rewritten() {
        let out = convert(r#"{"command":".claude/mcp/with-dotenv.sh","args":["--","x"]}"#);
        assert_eq!(out["command"], ".agents/mcp/with-dotenv.sh");
    }

    #[test]
    fn a_remote_server_keeps_its_url_and_maps_a_headers_helper_to_a_bearer_token() {
        let (out, warnings) = server(
            "sentry",
            &json(
                r#"{"type":"http","url":"https://mcp.example/api","headersHelper":".agents/mcp/dotenv-header.sh SENTRY_AUTH_TOKEN"}"#,
            ),
        );
        assert_eq!(out["url"], "https://mcp.example/api");
        assert_eq!(out["bearer_token_env_var"], "SENTRY_AUTH_TOKEN");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("no headersHelper equivalent"));
    }

    #[test]
    fn every_shipped_preset_converts_without_losing_its_transport() {
        // A converted server must end up with exactly one of command or url, or Codex
        // has nothing to launch. Worth asserting across the whole catalogue rather than
        // on the two examples above.
        let catalogue = crate::testing::catalogue();
        for preset in &catalogue.presets {
            for (name, definition) in &preset.servers {
                let (out, _) = server(name, definition);
                let has_command = out.get("command").is_some();
                let has_url = out.get("url").is_some();
                assert!(
                    has_command ^ has_url,
                    "{}/{name}: command={has_command} url={has_url}",
                    preset.name
                );
            }
        }
    }

    #[test]
    fn hook_wiring_merges_into_the_config_and_does_not_duplicate_on_a_re_run() {
        let wiring = json(
            r#"{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":".agents/hooks/a.sh"}]}]}"#,
        );
        let mut config = toml::Value::Table(toml::Table::new());
        merge_hooks(&mut config, &wiring).unwrap();
        merge_hooks(&mut config, &wiring).unwrap();

        let entries = config["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["hooks"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn a_second_hook_on_the_same_matcher_joins_the_existing_entry() {
        let mut config = toml::Value::Table(toml::Table::new());
        merge_hooks(
            &mut config,
            &json(r#"{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"a.sh"}]}]}"#),
        )
        .unwrap();
        merge_hooks(
            &mut config,
            &json(r#"{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"b.sh"}]}]}"#),
        )
        .unwrap();

        let entries = config["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["hooks"].as_array().unwrap().len(), 2);
    }
}
