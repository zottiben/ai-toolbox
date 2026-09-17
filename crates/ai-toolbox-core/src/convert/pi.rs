//! Pi's two knobs on the shared file, and the one thing that cannot be shared.
//!
//! `.mcp.json` is the cross-harness MCP file: Claude Code reads it natively, and so does
//! Pi's MCP client (the `pi-mcp-adapter` package - verified: a repo with a `.mcp.json`
//! and no `.pi/` directory at all connects). Only Codex needs a copy in its own format.
//!
//! So the job here is *not* to write a parallel Pi config. It is to add Pi's two knobs to
//! the shared file - Claude Code ignores unknown server keys, verified against 2.1.220 -
//! and to fall back to `.pi/mcp.json` (the adapter's highest-precedence layer, merged per
//! server key by key) only for a server whose Pi definition would be *wrong* for Claude
//! Code.
//!
//! Verified against pi-mcp-adapter 2.21.2 (`types.ts` `ServerEntry`, `server-manager.ts`,
//! `utils.ts`):
//!
//! - stdio when `command` is set, HTTP when `url` is set. A server must end up with
//!   exactly one of command/url/socket, so a layer must never change its transport kind.
//! - `lifecycle`: `lazy` (default), `eager`, `keep-alive`, `lazy-keep-alive`.
//! - `directTools: true` registers each tool individually instead of behind the adapter's
//!   one-tool proxy. The adapter warns past about 75 direct tools, and every one of them
//!   sits in the prompt.
//! - a remote server with no credentials needs no `auth` key at all: `auth === undefined`
//!   means deferred auto-detected OAuth, which is what we want. Setting it is noise in a
//!   shared file.
//! - a `headers` value starting with `!` is run as a command at connect time and its
//!   stdout becomes the value (`!!` escapes). That is Pi's answer to Claude's
//!   `headersHelper`, and the one thing that cannot be shared - Claude Code would send
//!   the literal string.

use serde_json::{Map, Value};

use super::{canonical_helper_path, is_ambient, passthrough_and_static, placeholders, WRAPPER};

/// stdio servers are local, cheap to spawn and small, so they connect at session start
/// like Claude Code does and their tools go straight in the prompt.
const LIFECYCLE_STDIO: &str = "eager";
/// A remote server may block on a network round trip or an OAuth browser flow, and the
/// SaaS ones are the tool-heavy ones (measured 2026-08-11: pixellab 72 tools, ClickUp
/// 55), so those stay lazy behind the proxy.
const LIFECYCLE_REMOTE: &str = "lazy";

pub struct Converted {
    /// Keys to layer onto the servers already in `.mcp.json`.
    pub shared: Map<String, Value>,
    /// Servers whose Pi definition would be wrong for Claude Code. Usually empty.
    pub overrides: Map<String, Value>,
    pub warnings: Vec<String>,
}

pub fn convert(presets: &[(&str, &Map<String, Value>)]) -> Converted {
    let mut out = Converted {
        shared: Map::new(),
        overrides: Map::new(),
        warnings: Vec::new(),
    };
    for (_, definitions) in presets {
        for (name, definition) in definitions.iter() {
            let (shared, overrides, warnings) = server(name, definition);
            if !shared.is_empty() {
                out.shared.insert(name.clone(), Value::Object(shared));
            }
            if !overrides.is_empty() {
                out.overrides.insert(name.clone(), Value::Object(overrides));
            }
            out.warnings.extend(warnings);
        }
    }
    out
}

pub fn server(
    name: &str,
    definition: &Value,
) -> (Map<String, Value>, Map<String, Value>, Vec<String>) {
    let mut shared = Map::new();
    let mut overrides = Map::new();
    let mut warnings = Vec::new();

    let kind = definition.get("type").and_then(|t| t.as_str());
    if kind == Some("http") || kind == Some("sse") || definition.get("url").is_some() {
        if kind == Some("sse") {
            overrides.insert(
                "httpTransport".to_string(),
                Value::String("sse".to_string()),
            );
        }
        if let Some(helper) = definition.get("headersHelper").and_then(|h| h.as_str()) {
            let (header, command, var) = header_from_helper(helper);
            let mut headers = Map::new();
            headers.insert(header, Value::String(command));
            overrides.insert("headers".to_string(), Value::Object(headers));
            warnings.push(format!(
                "{name}: {} stays in the repo .env - Pi reads it at connect time via a '!' command \
                 header in .pi/mcp.json (Claude Code keeps using headersHelper in .mcp.json).",
                var.unwrap_or_else(|| "the API token".to_string())
            ));
        }
        shared.insert(
            "lifecycle".to_string(),
            Value::String(LIFECYCLE_REMOTE.to_string()),
        );
        return (shared, overrides, warnings);
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
    let (passthrough, statics) =
        passthrough_and_static(definition.get("env").and_then(|e| e.as_object()));
    let in_args = placeholders(&args);

    // Pi never interpolates command/args, so a ${VAR} there has to be resolved by the
    // wrapper at launch. The shipped presets already use it, so this only fires for a
    // hand-written server - and it goes in the override file because rewriting the
    // command in the shared one would change what Claude Code launches.
    if !command.ends_with("with-dotenv.sh") && (!in_args.is_empty() || !passthrough.is_empty()) {
        let mut needs: Vec<String> = in_args.iter().filter(|v| !is_ambient(v)).cloned().collect();
        for var in &passthrough {
            if !needs.contains(var) {
                needs.push(var.clone());
            }
        }
        let mut wrapped = Vec::new();
        for var in &needs {
            wrapped.push(Value::String("--need".to_string()));
            wrapped.push(Value::String(var.clone()));
        }
        wrapped.push(Value::String("--".to_string()));
        wrapped.push(Value::String(command));
        wrapped.extend(args.iter().map(|a| Value::String(a.clone())));

        overrides.insert("command".to_string(), Value::String(WRAPPER.to_string()));
        overrides.insert("args".to_string(), Value::Array(wrapped));
        if !statics.is_empty() {
            overrides.insert("env".to_string(), Value::Object(statics));
        }

        let mut named = in_args.clone();
        for var in &passthrough {
            if !named.contains(var) {
                named.push(var.clone());
            }
        }
        warnings.push(format!(
            "{name}: Pi gets a {WRAPPER} wrap in .pi/mcp.json so ${{{}}} resolve from the repo .env \
             at launch (Pi never interpolates command/args). Move the server to that wrapper in \
             .mcp.json to share one definition.",
            named.join("}, ${")
        ));
    }

    shared.insert(
        "lifecycle".to_string(),
        Value::String(LIFECYCLE_STDIO.to_string()),
    );
    shared.insert("directTools".to_string(), Value::Bool(true));
    (shared, overrides, warnings)
}

/// Map a Claude `headersHelper` command to a Pi `headers` entry.
///
/// The helper prints a JSON headers object for Claude; `--raw` makes it print just the
/// value, which is what Pi's `!command` secret resolution wants.
fn header_from_helper(helper: &str) -> (String, String, Option<String>) {
    let tokens = split_words(helper);
    let (script, rest) = tokens
        .split_first()
        .map_or(("", &[][..]), |(s, r)| (s.as_str(), r));
    let script = canonical_helper_path(script);

    let mut header = "Authorization".to_string();
    for (index, token) in rest.iter().enumerate() {
        if token == "--header" {
            if let Some(name) = rest.get(index + 1) {
                header.clone_from(name);
            }
        }
    }
    let var = rest.iter().find(|t| is_upper_var(t)).cloned();

    let mut command = String::from("!");
    for (index, word) in std::iter::once(&script)
        .chain(std::iter::once(&"--raw".to_string()))
        .chain(rest.iter())
        .enumerate()
    {
        if index > 0 {
            command.push(' ');
        }
        command.push_str(&quote(word));
    }
    (header, command, var)
}

/// Shell-style word splitting, good enough for the helper commands presets carry: single
/// and double quotes group, and nothing else is special. A helper needing more than this
/// would be a helper that should be a script.
fn split_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    for ch in text.chars() {
        match quote {
            Some(open) if ch == open => quote = None,
            Some(_) => current.push(ch),
            None if ch == '\'' || ch == '"' => {
                quote = Some(ch);
                started = true;
            }
            None if ch.is_whitespace() => {
                if started || !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            None => current.push(ch),
        }
    }
    if started || !current.is_empty() {
        words.push(current);
    }
    words
}

/// Quote a word for the shell Pi runs a `!` header command in. Single quotes with the
/// embedded-quote escape, which is what Python's `shlex.join` produced before.
fn quote(word: &str) -> String {
    let safe = !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "@%+=:,./-_".contains(c));
    if safe {
        return word.to_string();
    }
    format!("'{}'", word.replace('\'', r#"'"'"'"#))
}

fn is_upper_var(text: &str) -> bool {
    !text.is_empty()
        && text.starts_with(|c: char| c.is_ascii_uppercase() || c == '_')
        && text
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(text: &str) -> Value {
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn a_stdio_server_gets_both_knobs_and_no_override() {
        let (shared, overrides, warnings) =
            server("s", &json(r#"{"command":"npx","args":["-y","p"]}"#));
        assert_eq!(shared["lifecycle"], "eager");
        assert_eq!(shared["directTools"], true);
        assert!(
            overrides.is_empty(),
            "nothing here is wrong for Claude Code"
        );
        assert!(warnings.is_empty());
    }

    #[test]
    fn a_remote_server_stays_lazy_and_does_not_get_direct_tools() {
        let (shared, overrides, _) = server("s", &json(r#"{"type":"http","url":"https://x/api"}"#));
        assert_eq!(shared["lifecycle"], "lazy");
        assert!(
            shared.get("directTools").is_none(),
            "the tool-heavy servers are the remote ones; every direct tool sits in the prompt"
        );
        assert!(
            overrides.is_empty(),
            "a remote server with no credentials needs no auth key - undefined means deferred OAuth"
        );
    }

    #[test]
    fn an_sse_server_records_its_transport_as_an_override() {
        let (_, overrides, _) = server("s", &json(r#"{"type":"sse","url":"https://x/sse"}"#));
        assert_eq!(overrides["httpTransport"], "sse");
    }

    #[test]
    fn a_headers_helper_becomes_a_bang_command_in_the_override_file() {
        let (shared, overrides, warnings) = server(
            "sentry",
            &json(
                r#"{"type":"http","url":"https://x/api","headersHelper":".agents/mcp/dotenv-header.sh SENTRY_AUTH_TOKEN"}"#,
            ),
        );
        let command = overrides["headers"]["Authorization"].as_str().unwrap();
        assert!(command.starts_with('!'), "got {command}");
        assert!(command.contains("--raw"));
        assert!(command.contains("SENTRY_AUTH_TOKEN"));
        // The shared file keeps headersHelper for Claude Code; only Pi sees this.
        assert!(shared.get("headers").is_none());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("SENTRY_AUTH_TOKEN"));
    }

    #[test]
    fn a_custom_header_name_is_honoured() {
        let (_, overrides, _) = server(
            "s",
            &json(r#"{"url":"https://x","headersHelper":"h.sh --header X-Api-Key API_KEY"}"#),
        );
        assert!(overrides["headers"].get("X-Api-Key").is_some());
    }

    #[test]
    fn a_hand_written_server_with_a_placeholder_gets_a_wrapper_override() {
        let (shared, overrides, warnings) = server(
            "custom",
            &json(r#"{"command":"node","args":["server.js","--key=${API_KEY}"]}"#),
        );
        assert_eq!(overrides["command"], WRAPPER);
        let args: Vec<&str> = overrides["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_str().unwrap())
            .collect();
        assert_eq!(
            args,
            vec![
                "--need",
                "API_KEY",
                "--",
                "node",
                "server.js",
                "--key=${API_KEY}"
            ]
        );
        // The shared knobs still apply - it is still a stdio server.
        assert_eq!(shared["lifecycle"], "eager");
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn a_shipped_preset_already_on_the_wrapper_needs_no_override() {
        let catalogue = crate::testing::catalogue();
        let preset = catalogue.preset("supabase").unwrap();
        let definition = preset.servers.get("supabase").unwrap();
        let (_, overrides, warnings) = server("supabase", definition);
        assert!(
            overrides.is_empty(),
            "the shipped presets already use the wrapper, so nothing has to be rewritten"
        );
        assert!(warnings.is_empty());
    }

    #[test]
    fn word_splitting_handles_quotes_the_way_a_shell_does() {
        assert_eq!(split_words("a b  c"), vec!["a", "b", "c"]);
        assert_eq!(
            split_words(r#"h.sh --header "X Api Key" TOKEN"#),
            vec!["h.sh", "--header", "X Api Key", "TOKEN"]
        );
        assert_eq!(split_words("''"), vec![""]);
    }

    #[test]
    fn a_word_needing_quoting_is_quoted_and_one_that_does_not_is_left_alone() {
        assert_eq!(
            quote(".agents/mcp/dotenv-header.sh"),
            ".agents/mcp/dotenv-header.sh"
        );
        assert_eq!(quote("X Api Key"), "'X Api Key'");
        assert_eq!(quote("it's"), r#"'it'"'"'s'"#);
    }

    #[test]
    fn every_shipped_preset_keeps_its_transport_intact_through_the_override() {
        // A Pi override layers onto the shared definition, so one that set `command` on
        // a server the shared file gives a `url` would produce a server with both - which
        // the adapter refuses.
        let catalogue = crate::testing::catalogue();
        for preset in &catalogue.presets {
            for (name, definition) in &preset.servers {
                let (_, overrides, _) = server(name, definition);
                let remote = definition.get("url").is_some();
                assert!(
                    !(remote && overrides.contains_key("command")),
                    "{}/{name}: an override must never change the transport kind",
                    preset.name
                );
            }
        }
    }
}
