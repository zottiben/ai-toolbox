//! One preset, three harnesses.
//!
//! A preset is authored once, in Claude Code's `.mcp.json` shape, and converted at
//! install time into whatever each harness needs. Nothing is authored twice, which is
//! why a new preset only ever needs adding to `mcp/presets/`.
//!
//! What each harness needs, and why:
//!
//! - **Claude Code** reads `.mcp.json` natively. No conversion at all.
//! - **Codex** cannot read it, and does not interpolate `${VAR}` in config strings. See
//!   [`codex`].
//! - **Pi** reads `.mcp.json` too, through its adapter, so it needs only two extra keys
//!   on the shared file - and a separate override for the one thing that genuinely
//!   cannot be shared. See [`pi`].

pub mod codex;
pub mod pi;

use serde_json::{Map, Value};

use crate::error::{Error, Result};

/// The launcher that loads a repo `.env` and expands `${VAR}` before exec'ing the real
/// command. It is the bridge that lets one server definition work under a harness that
/// interpolates config strings and one that does not.
pub const WRAPPER: &str = ".agents/mcp/with-dotenv.sh";

/// Set by any shell. Not secrets - but a harness that will not interpolate a config
/// string will not interpolate these either, so they still need the wrapper; they just
/// must not be declared as things to find in a `.env`.
const AMBIENT: [&str; 9] = [
    "HOME", "USER", "LOGNAME", "PATH", "PWD", "SHELL", "TMPDIR", "LANG", "TERM",
];

pub fn is_ambient(name: &str) -> bool {
    AMBIENT.contains(&name)
}

/// Older presets and configs pointed helpers at a per-harness directory; there is one
/// copy now, and rewriting on the way through is what lets an old repo convert cleanly.
pub fn canonical_helper_path(text: &str) -> String {
    let mut out = text.to_string();
    for old in [".claude/mcp/", ".codex/mcp/", ".pi/mcp/"] {
        out = out.replace(old, ".agents/mcp/");
    }
    out
}

/// Split an `env` map the way both Codex and Pi need it: a value that is *exactly*
/// `${NAME}` means "pass the host's value through", while anything else is a literal.
pub fn passthrough_and_static(
    env: Option<&Map<String, Value>>,
) -> (Vec<String>, Map<String, Value>) {
    let mut passthrough = Vec::new();
    let mut statics = Map::new();
    for (key, value) in env.into_iter().flatten() {
        match value.as_str().map(str::trim).and_then(sole_placeholder) {
            Some(_) => passthrough.push(key.clone()),
            None => {
                statics.insert(key.clone(), value.clone());
            }
        }
    }
    (passthrough, statics)
}

/// `${NAME}` and nothing else. `"${A}${B}"` and `"prefix-${A}"` are literals, because
/// passing them through by name would send the wrong value.
fn sole_placeholder(text: &str) -> Option<&str> {
    let inner = text.strip_prefix("${")?.strip_suffix('}')?;
    is_var_name(inner).then_some(inner)
}

/// Every `${VAR}` appearing anywhere in a list of strings, in first-seen order.
pub fn placeholders(args: &[String]) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for arg in args {
        let mut rest = arg.as_str();
        while let Some(start) = rest.find("${") {
            let after = &rest[start + 2..];
            let Some(end) = after.find('}') else { break };
            let name = &after[..end];
            if is_var_name(name) && !found.iter().any(|f| f == name) {
                found.push(name.to_string());
            }
            rest = &after[end + 1..];
        }
    }
    found
}

fn is_var_name(text: &str) -> bool {
    !text.is_empty()
        && text.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// JSON to TOML, for the Codex side.
///
/// Null has no TOML spelling, so a definition carrying one is a real problem rather than
/// something to paper over - it would silently drop a key the user wrote.
pub fn to_toml(value: &Value) -> Result<toml::Value> {
    if contains_null(value) {
        return Err(Error::Catalogue(
            "a server definition contains null, which TOML cannot express - use a string, \
             or leave the key out"
                .to_string(),
        ));
    }
    serde_json::from_value(value.clone())
        .map_err(|e| Error::Catalogue(format!("converting a server definition to TOML: {e}")))
}

fn contains_null(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Array(items) => items.iter().any(contains_null),
        Value::Object(map) => map.values().any(contains_null),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(text: &str) -> Value {
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn only_a_bare_placeholder_counts_as_host_passthrough() {
        let env =
            json(r#"{"A":"${A}","B":" ${B} ","C":"prefix-${C}","D":"${E}${F}","G":"literal"}"#);
        let (passthrough, statics) = passthrough_and_static(env.as_object());

        assert_eq!(passthrough, vec!["A".to_string(), "B".to_string()]);
        // Anything compound is a literal: passing C through by name would send the
        // value of C, losing the prefix.
        assert!(statics.contains_key("C"));
        assert!(statics.contains_key("D"));
        assert!(statics.contains_key("G"));
    }

    #[test]
    fn placeholders_are_found_in_order_and_only_once() {
        let args = vec![
            "--a=${ONE}".to_string(),
            "--b=${TWO}/${ONE}".to_string(),
            "plain".to_string(),
        ];
        assert_eq!(
            placeholders(&args),
            vec!["ONE".to_string(), "TWO".to_string()]
        );
    }

    #[test]
    fn an_unclosed_placeholder_does_not_hang_or_match() {
        assert!(placeholders(&["${UNCLOSED".to_string()]).is_empty());
        assert_eq!(
            placeholders(&["${A}${UNCLOSED".to_string()]),
            vec!["A".to_string()]
        );
    }

    #[test]
    fn legacy_helper_paths_are_rewritten_wherever_they_appear() {
        assert_eq!(
            canonical_helper_path(".codex/mcp/with-dotenv.sh"),
            ".agents/mcp/with-dotenv.sh"
        );
        assert_eq!(canonical_helper_path("npx"), "npx");
    }

    #[test]
    fn a_null_in_a_definition_is_refused_rather_than_silently_dropped() {
        let err = to_toml(&json(r#"{"command":"x","env":{"K":null}}"#)).unwrap_err();
        assert!(format!("{err}").contains("null"), "got: {err}");
    }

    #[test]
    fn a_normal_definition_converts_to_toml() {
        let value = to_toml(&json(
            r#"{"command":"npx","args":["-y","p"],"env":{"K":"v"}}"#,
        ))
        .unwrap();
        assert_eq!(value["command"].as_str(), Some("npx"));
        assert_eq!(value["args"].as_array().unwrap().len(), 2);
    }
}
