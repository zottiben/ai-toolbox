//! Which secrets a preset needs, and where they have to be put.
//!
//! Ported from `_mcp_report_secrets`. The distinction it draws is the useful one and is
//! kept: a `${VAR}` placeholder is resolved by the harness from the process environment,
//! so it has to be exported before the harness starts, while a var named by `--need` or
//! by `dotenv-header.sh` is read out of the repo's `.env` at connect time and must not be
//! exported at all.

use std::collections::BTreeSet;

/// Set by any shell, so a preset referencing one is not asking for a secret.
const AMBIENT: [&str; 9] = [
    "HOME", "USER", "LOGNAME", "PATH", "PWD", "SHELL", "TMPDIR", "LANG", "TERM",
];

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub struct Secret {
    pub name: String,
    pub source: Source,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    /// Exported in the environment before the harness starts.
    Environment,
    /// Read from the repo's `.env` when the server launches.
    DotEnv,
}

impl Secret {
    /// The line the CLI prints and the GUI shows next to the preset.
    pub fn advice(&self) -> &'static str {
        match self.source {
            Source::Environment => "export it before starting the harness",
            Source::DotEnv => "put it in the repo's .env - no export needed",
        }
    }
}

/// Every secret a preset's JSON asks for.
pub fn scan(value: &serde_json::Value) -> Vec<Secret> {
    let mut found: BTreeSet<Secret> = BTreeSet::new();
    walk(value, &mut found);
    found.into_iter().collect()
}

fn walk(value: &serde_json::Value, found: &mut BTreeSet<Secret>) {
    match value {
        serde_json::Value::String(text) => {
            for name in placeholders(text) {
                found.insert(Secret {
                    name,
                    source: Source::Environment,
                });
            }
            for name in dotenv_header_args(text) {
                found.insert(Secret {
                    name,
                    source: Source::DotEnv,
                });
            }
        }
        serde_json::Value::Array(items) => {
            for name in needs(items) {
                found.insert(Secret {
                    name,
                    source: Source::DotEnv,
                });
            }
            for item in items {
                walk(item, found);
            }
        }
        serde_json::Value::Object(map) => {
            for item in map.values() {
                walk(item, found);
            }
        }
        _ => {}
    }
}

/// `${SUPABASE_ACCESS_TOKEN}` anywhere in a string.
fn placeholders(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else { break };
        let name = &after[..end];
        if is_var_name(name) && !AMBIENT.contains(&name) {
            out.push(name.to_string());
        }
        rest = &after[end + 1..];
    }
    out
}

/// `["--need", "TOKEN", "--", ...]` - the flag and its value are separate array
/// elements, so this reads pairs rather than searching within one string.
fn needs(items: &[serde_json::Value]) -> Vec<String> {
    items
        .windows(2)
        .filter(|pair| pair[0].as_str() == Some("--need"))
        .filter_map(|pair| pair[1].as_str())
        .filter(|name| is_var_name(name))
        .map(|name| name.to_string())
        .collect()
}

/// `dotenv-header.sh SENTRY_TOKEN` - a command string with the var appended.
fn dotenv_header_args(text: &str) -> Vec<String> {
    let Some(tail) = text.split("dotenv-header.sh").nth(1) else {
        return Vec::new();
    };
    tail.split_whitespace()
        .take_while(|word| is_var_name(word))
        .map(|word| word.to_string())
        .collect()
}

/// Upper snake case, the shape every secret in the presets has. Narrow on purpose: it is
/// what keeps `${HOME}/bin` and a stray `--need` at the end of a list out of the report.
fn is_var_name(text: &str) -> bool {
    !text.is_empty()
        && text.starts_with(|c: char| c.is_ascii_uppercase() || c == '_')
        && text
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan_str(json: &str) -> Vec<Secret> {
        scan(&serde_json::from_str(json).unwrap())
    }

    #[test]
    fn a_placeholder_has_to_be_exported() {
        let found = scan_str(r#"{"env": {"TOKEN": "${FIGMA_TOKEN}"}}"#);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "FIGMA_TOKEN");
        assert_eq!(found[0].source, Source::Environment);
    }

    #[test]
    fn a_need_flag_comes_from_the_repo_dotenv() {
        let found = scan_str(
            r#"{"args": ["--need", "SUPABASE_ACCESS_TOKEN", "--", "npx", "-y", "server"]}"#,
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "SUPABASE_ACCESS_TOKEN");
        assert_eq!(found[0].source, Source::DotEnv);
    }

    #[test]
    fn ambient_shell_variables_are_not_secrets() {
        assert!(scan_str(r#"{"command": "${HOME}/bin/thing", "cwd": "${PWD}"}"#).is_empty());
    }

    #[test]
    fn the_same_variable_named_twice_is_reported_once() {
        let found = scan_str(r#"{"a": "${TOKEN}", "b": {"c": "${TOKEN}"}}"#);
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn a_dotenv_header_command_names_its_variable() {
        let found = scan_str(r#"{"command": ".agents/mcp/dotenv-header.sh SENTRY_AUTH_TOKEN"}"#);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].source, Source::DotEnv);
    }

    #[test]
    fn every_shipped_preset_scans_without_reporting_junk() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../mcp/presets");
        let mut total = 0;
        for entry in std::fs::read_dir(root).unwrap() {
            let path = entry.unwrap().path();
            let value: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            for secret in scan(&value) {
                assert!(
                    is_var_name(&secret.name),
                    "{}: {} is not a variable name",
                    path.display(),
                    secret.name
                );
                total += 1;
            }
        }
        // The shipped presets do ask for secrets; a scanner that found none would pass
        // every assertion above while being broken.
        assert!(
            total > 0,
            "no preset asked for a secret - the scan found nothing"
        );
    }
}
