//! Pi's MCP config, for both of the clients Pi can use.
//!
//! The two read different files, which is the whole shape of this module:
//!
//! - **pi-mcp-adapter** reads the shared `.mcp.json` (verified: a repo with a `.mcp.json`
//!   and no `.pi/` directory at all connects) and layers `.pi/mcp-adapter.json` on top,
//!   merged per server key by key. Its knobs are inert to Claude Code - unknown server
//!   keys are ignored, verified against 2.1.220 - so they go on the shared file, and the
//!   adapter file exists only for a server whose Pi definition would be *wrong* for
//!   Claude Code.
//! - **Pi's built-in MCP** (Pi 0.99 and later) reads only `.pi/mcp.json`, in Pi's own
//!   format, and never `.mcp.json`. So it gets complete definitions there: the one place
//!   a second copy is a translation rather than duplication. Adapter keys never go in it.
//!
//! Verified against pi-mcp-adapter 5.3.0 (`types.ts` `ServerEntry`, `config.ts`
//! `mergeServerMaps`) and Pi 1.2.0 (`docs/mcp.md`):
//!
//! - stdio when `command` is set, HTTP when `url` is set. A layer naming the other kind
//!   replaces the transport, and the adapter drops the lower layer's url-bound
//!   credentials when the url changes. A reinstall here follows the same rule.
//! - exposure. The adapter's `directTools` is `true`, a list of tool names, `false` (the
//!   one-tool proxy) or `"search"` (registered but inactive until a search finds them,
//!   since adapter 2.33). Pi's `exposure` is `direct`, `deferred`, `codemode` or
//!   `hidden`. A direct tool sits in the prompt on every turn, so on-demand is the
//!   default and direct is kept for small sets named in [`DIRECT_BY_DEFAULT`].
//! - `lifecycle` (adapter only): `lazy` (default), `eager`, `keep-alive`,
//!   `lazy-keep-alive`. It decides when a server runs, not how its tools are exposed.
//! - a remote server with no credentials needs no `auth` key at all: both clients detect
//!   OAuth for themselves. Setting it is noise in a shared file.
//! - a `headers` value starting with `!` is run as a command at connect time and its
//!   stdout becomes the value, in both clients. That is Pi's answer to Claude's
//!   `headersHelper`, and the one thing that cannot be shared - Claude Code would send
//!   the literal string.

use serde_json::{Map, Value};

use super::{canonical_helper_path, is_ambient, passthrough_and_static, placeholders, WRAPPER};
use crate::paths;

/// stdio servers are local and cheap to spawn, so they connect at session start like
/// Claude Code does.
const LIFECYCLE_STDIO: &str = "eager";
/// A remote server may block on a network round trip or an OAuth browser flow, so it
/// connects when it is first used.
const LIFECYCLE_REMOTE: &str = "lazy";

/// Servers whose tool set is small and fixed enough to sit in the prompt from the first
/// turn. Context7 and an existing Excel server are the curated exceptions; other
/// servers are reached on demand.
pub const DIRECT_BY_DEFAULT: [&str; 2] = ["context7", "excel"];

/// Which of Pi's MCP clients a repo is configured for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "client", rename_all = "kebab-case")]
pub enum Target {
    /// pi-mcp-adapter. `search` is whether it is known to take `directTools: "search"`;
    /// without that, on-demand means the proxy rather than syntax it may not know.
    Adapter { search: bool },
    /// Pi's built-in MCP.
    Native,
}

pub struct Converted {
    /// Adapter defaults for the servers in `.mcp.json`. Only fills a choice nobody made.
    pub shared: Map<String, Value>,
    /// Adapter keys that would be wrong for Claude Code, for `.pi/mcp-adapter.json`.
    pub overrides: Map<String, Value>,
    /// Complete Pi definitions for `.pi/mcp.json`, without any choice in them.
    pub native: Map<String, Value>,
    /// Pi exposure defaults for `native`. Only fills a choice nobody made.
    pub native_defaults: Map<String, Value>,
    pub warnings: Vec<String>,
}

pub fn convert(presets: &[(&str, &Map<String, Value>)], target: Target) -> Converted {
    let mut out = Converted {
        shared: Map::new(),
        overrides: Map::new(),
        native: Map::new(),
        native_defaults: Map::new(),
        warnings: Vec::new(),
    };
    for (_, definitions) in presets {
        for (name, definition) in definitions.iter() {
            match target {
                Target::Adapter { search } => {
                    let (shared, overrides, warnings) = adapter(name, definition, search);
                    if !shared.is_empty() {
                        out.shared.insert(name.clone(), Value::Object(shared));
                    }
                    if !overrides.is_empty() {
                        out.overrides.insert(name.clone(), Value::Object(overrides));
                    }
                    out.warnings.extend(warnings);
                }
                Target::Native => {
                    let (converted, warnings) = native(name, definition);
                    if let Some(converted) = converted {
                        out.native.insert(name.clone(), Value::Object(converted));
                        out.native_defaults
                            .insert(name.clone(), Value::Object(native_defaults(name)));
                    }
                    out.warnings.extend(warnings);
                }
            }
        }
    }
    out
}

/// The adapter's view of one server: defaults for the shared file, and overrides.
pub fn adapter(
    name: &str,
    definition: &Value,
    search: bool,
) -> (Map<String, Value>, Map<String, Value>, Vec<String>) {
    let mut shared = Map::new();
    let mut overrides = Map::new();
    let mut warnings = Vec::new();

    if DIRECT_BY_DEFAULT.contains(&name) {
        shared.insert("directTools".to_string(), Value::Bool(true));
    } else if search {
        shared.insert(
            "directTools".to_string(),
            Value::String("search".to_string()),
        );
    }

    if is_remote(definition) {
        if definition.get("type").and_then(|t| t.as_str()) == Some("sse") {
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
                 header in {} (Claude Code keeps using headersHelper in .mcp.json).",
                var.unwrap_or_else(|| "the API token".to_string()),
                paths::PI_MCP_ADAPTER
            ));
        }
        shared.insert(
            "lifecycle".to_string(),
            Value::String(LIFECYCLE_REMOTE.to_string()),
        );
        return (shared, overrides, warnings);
    }

    // The override goes in the adapter file because rewriting the command in the shared
    // one would change what Claude Code launches.
    if let Some(launch) = wrapped(definition) {
        overrides.insert("command".to_string(), Value::String(WRAPPER.to_string()));
        overrides.insert("args".to_string(), Value::Array(launch.args));
        if !launch.statics.is_empty() {
            overrides.insert("env".to_string(), Value::Object(launch.statics));
        }
        warnings.push(format!(
            "{name}: Pi gets a {WRAPPER} wrap in {} so ${{{}}} resolve from the repo .env \
             at launch (Pi never interpolates command/args). Move the server to that wrapper in \
             .mcp.json to share one definition.",
            paths::PI_MCP_ADAPTER,
            launch.named.join("}, ${")
        ));
    }

    shared.insert(
        "lifecycle".to_string(),
        Value::String(LIFECYCLE_STDIO.to_string()),
    );
    (shared, overrides, warnings)
}

/// One server as Pi's built-in MCP needs it, or `None` for one it cannot run.
pub fn native(name: &str, definition: &Value) -> (Option<Map<String, Value>>, Vec<String>) {
    let mut out = Map::new();
    let mut warnings = Vec::new();

    if is_remote(definition) {
        if definition.get("type").and_then(|t| t.as_str()) == Some("sse") {
            warnings.push(format!(
                "{name}: Pi's built-in MCP does not support the legacy SSE transport, so it is \
                 left out of {}. Servers with an SSE endpoint often serve streamable HTTP at /mcp.",
                paths::PI_MCP
            ));
            return (None, warnings);
        }
        if let Some(url) = definition.get("url") {
            out.insert("url".to_string(), url.clone());
        }
        let mut headers = definition
            .get("headers")
            .and_then(|h| h.as_object())
            .cloned()
            .unwrap_or_default();
        if let Some(helper) = definition.get("headersHelper").and_then(|h| h.as_str()) {
            let (header, command, var) = header_from_helper(helper);
            headers.insert(header, Value::String(command));
            warnings.push(format!(
                "{name}: {} stays in the repo .env - Pi reads it at connect time via a '!' command \
                 header in {} (Claude Code keeps using headersHelper in .mcp.json).",
                var.unwrap_or_else(|| "the API token".to_string()),
                paths::PI_MCP
            ));
        }
        if !headers.is_empty() {
            out.insert("headers".to_string(), Value::Object(headers));
        }
        if let Some(oauth) = definition.get("oauth") {
            out.insert("oauth".to_string(), oauth.clone());
        }
        return (Some(out), warnings);
    }

    match wrapped(definition) {
        Some(launch) => {
            out.insert("command".to_string(), Value::String(WRAPPER.to_string()));
            out.insert("args".to_string(), Value::Array(launch.args));
            if !launch.statics.is_empty() {
                out.insert("env".to_string(), Value::Object(launch.statics));
            }
        }
        None => {
            for key in ["command", "args", "env"] {
                if let Some(value) = definition.get(key) {
                    out.insert(key.to_string(), value.clone());
                }
            }
        }
    }
    if let Some(cwd) = definition.get("cwd") {
        out.insert("cwd".to_string(), cwd.clone());
    }
    (Some(out), warnings)
}

fn native_defaults(name: &str) -> Map<String, Value> {
    let exposure = if DIRECT_BY_DEFAULT.contains(&name) {
        "direct"
    } else {
        "deferred"
    };
    let mut defaults = Map::new();
    defaults.insert("exposure".to_string(), Value::String(exposure.to_string()));
    defaults
}

fn is_remote(definition: &Value) -> bool {
    let kind = definition.get("type").and_then(|t| t.as_str());
    kind == Some("http") || kind == Some("sse") || definition.get("url").is_some()
}

struct Launch {
    args: Vec<Value>,
    statics: Map<String, Value>,
    named: Vec<String>,
}

/// The wrapper launch for a stdio server with a `${VAR}` Pi would not resolve, if it
/// needs one. Neither of Pi's clients interpolates command/args, so a `${VAR}` there has
/// to be resolved by the wrapper. The shipped presets already use it, so this only fires
/// for a hand-written server.
fn wrapped(definition: &Value) -> Option<Launch> {
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
    if command.ends_with("with-dotenv.sh") || (in_args.is_empty() && passthrough.is_empty()) {
        return None;
    }

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

    let mut named = in_args;
    for var in &passthrough {
        if !named.contains(var) {
            named.push(var.clone());
        }
    }
    Some(Launch {
        args: wrapped,
        statics,
        named,
    })
}

/// Keys a preset owns outright. A reinstall takes them from the preset and never from
/// what was there, or a server whose transport changed would keep launching the old one.
const DEFINITION_KEYS: [&str; 7] = [
    "type",
    "command",
    "args",
    "url",
    "socket",
    "headersHelper",
    "transport",
];
/// Keys that only mean something to a stdio server.
const STDIO_KEYS: [&str; 5] = ["env", "cwd", "inheritEnv", "literalEnv", "pluginDataDir"];
/// Credentials and transport details bound to one url. Inheriting them across a url
/// change would send one endpoint's credentials to another.
const URL_KEYS: [&str; 9] = [
    "headers",
    "oauth",
    "auth",
    "bearerToken",
    "bearerTokenEnv",
    "bearerTokenStore",
    "requestHeadersCommand",
    "caFile",
    "httpTransport",
];

#[derive(Debug, PartialEq, Eq)]
enum Transport<'a> {
    Stdio,
    Remote(Option<&'a str>),
}

fn transport(entry: &Value) -> Option<Transport<'_>> {
    if is_remote(entry) {
        return Some(Transport::Remote(entry.get("url").and_then(|u| u.as_str())));
    }
    entry.get("command").map(|_| Transport::Stdio)
}

/// Carry what a reinstall found in `previous` onto `fresh`, the entry it is about to
/// write for `definition`.
///
/// Everything that is a choice survives - exposure, lifecycle, filters, approvals,
/// enablement - and `fresh` wins only for the keys it sets. What is bound to a transport
/// survives only while the server keeps that transport (and, for credentials, that url),
/// and anything dropped or kept across a change is said out loud.
pub fn carry(
    name: &str,
    file: &str,
    previous: &Value,
    definition: &Value,
    fresh: &mut Map<String, Value>,
) -> Vec<String> {
    let mut warnings = Vec::new();
    let Some(entries) = previous.as_object() else {
        return warnings;
    };
    let before = transport(previous);
    let now = transport(definition);
    let changed = match (&before, &now) {
        (Some(Transport::Stdio), Some(Transport::Stdio)) => false,
        (Some(Transport::Remote(a)), Some(Transport::Remote(b))) => a != b,
        (Some(_), Some(_)) => true,
        _ => false,
    };

    let mut dropped = Vec::new();
    let mut kept = Vec::new();
    for (key, value) in entries {
        let key = key.as_str();
        if DEFINITION_KEYS.contains(&key) || fresh.contains_key(key) {
            continue;
        }
        let fits = if STDIO_KEYS.contains(&key) {
            now.as_ref().is_none_or(|t| *t == Transport::Stdio) && !changed
        } else if URL_KEYS.contains(&key) {
            now.as_ref()
                .is_none_or(|t| matches!(t, Transport::Remote(_)))
                && !changed
        } else {
            if changed {
                kept.push(key.to_string());
            }
            true
        };
        if fits {
            fresh.insert(key.to_string(), value.clone());
        } else {
            dropped.push(key.to_string());
        }
    }

    if !dropped.is_empty() {
        warnings.push(format!(
            "{name}: {file} no longer carries {} - the server's transport or url changed and \
             they belonged to the old one. Set them again if the new server needs them.",
            dropped.join(", ")
        ));
    }
    if !kept.is_empty() {
        warnings.push(format!(
            "{name}: {file} kept {} from before the server's transport or url changed - check \
             they still suit it.",
            kept.join(", ")
        ));
    }
    warnings
}

/// Add each default whose key neither `entry` nor `chosen` (a higher layer the same
/// client reads) already sets. A default never replaces a choice.
pub fn fill(entry: &mut Map<String, Value>, defaults: Option<&Value>, chosen: Option<&Value>) {
    for (key, value) in defaults.and_then(|d| d.as_object()).into_iter().flatten() {
        if entry.contains_key(key) || chosen.is_some_and(|c| c.get(key).is_some()) {
            continue;
        }
        entry.insert(key.clone(), value.clone());
    }
}

/// Top-level keys only the adapter reads. Pi's built-in MCP ignores them, and adapter 5
/// ignores them in `.pi/mcp.json` too.
const ADAPTER_TOP_KEYS: [&str; 4] = ["settings", "imports", "claudePlugins", "mcp-servers"];

/// Server keys only the adapter understands (`types.ts` `ServerEntry`, less the keys Pi's
/// built-in MCP shares with it).
const ADAPTER_SERVER_KEYS: [&str; 26] = [
    "socket",
    "inheritEnv",
    "caFile",
    "requestHeadersCommand",
    "bearerToken",
    "bearerTokenEnv",
    "bearerTokenStore",
    "lifecycle",
    "idleTimeout",
    "requestTimeoutMs",
    "exposeResources",
    "directTools",
    "toolPrefix",
    "includeTools",
    "excludeTools",
    "searchKeywords",
    "approveTools",
    "openUi",
    "debug",
    "trace",
    "httpTransport",
    "pluginDataDir",
    "literalEnv",
    "protocolVersion",
    "tasks",
    "disabled",
];

/// The only keys a Pi entry without `command`, `url` or `type` applies: it adjusts a
/// user-level server of the same name and nothing else.
const NATIVE_PARTIAL_KEYS: [&str; 3] = ["enabled", "exposure", "toolExposure"];

/// Keys an adapter override applies to the server below it, which a Pi partial entry
/// would ignore.
const OVERRIDE_KEYS: [&str; 6] = ["args", "env", "cwd", "headers", "oauth", "description"];

/// What an older `.pi/mcp.json` holds that is adapter configuration, split from what is
/// valid for Pi's built-in MCP.
#[derive(Debug, Default, PartialEq)]
pub struct Split {
    /// What stays in `.pi/mcp.json`: the document less everything moved or dropped.
    pub native: Map<String, Value>,
    /// Top-level adapter keys, for `.pi/mcp-adapter.json`.
    pub settings: Map<String, Value>,
    /// Per-server adapter keys, for `.pi/mcp-adapter.json`.
    pub servers: Map<String, Value>,
    /// Keys no client accepts, removed rather than moved, as `server.key`.
    pub dropped: Vec<String>,
}

impl Split {
    pub fn is_empty(&self) -> bool {
        self.settings.is_empty() && self.servers.is_empty() && self.dropped.is_empty()
    }
}

/// Split a `.pi/mcp.json` from before the adapter had a file of its own (adapter 3.0).
///
/// It was the adapter's file then, and it is Pi's now. A complete definition keeps its
/// Pi keys and loses the adapter's; an entry that names no transport is a Pi override
/// only for `enabled`, `exposure` and `toolExposure`, so anything else in it was an
/// adapter override and moves. Keys neither client knows stay where they are: this is
/// not the place to decide what someone else's key means.
///
/// Two shapes older versions of this toolbox wrote are dropped outright: `transport`,
/// which no client reads (the adapter's key is `httpTransport`), and
/// `auth: {"type": "oauth"}`, which disables the OAuth it looks like it enables. Leaving
/// `auth` out is deferred auto-detected OAuth, the behaviour that was wanted. Any other
/// `auth` is left as it is - Pi's `{"provider": …}` is valid in both clients.
pub fn split_legacy(document: &Value) -> Split {
    let mut split = Split::default();
    let Some(top) = document.as_object() else {
        return split;
    };
    for (key, value) in top {
        if ADAPTER_TOP_KEYS.contains(&key.as_str()) {
            split.settings.insert(key.clone(), value.clone());
        } else if key != "mcpServers" {
            split.native.insert(key.clone(), value.clone());
        }
    }
    let Some(servers) = top.get("mcpServers") else {
        return split;
    };
    let Some(servers) = servers.as_object() else {
        split
            .native
            .insert("mcpServers".to_string(), servers.clone());
        return split;
    };

    let mut kept_servers = Map::new();
    for (name, entry) in servers {
        let Some(fields) = entry.as_object() else {
            kept_servers.insert(name.clone(), entry.clone());
            continue;
        };
        let complete = ["command", "url", "type"]
            .iter()
            .any(|k| fields.contains_key(*k));
        let mut native = Map::new();
        let mut adapter = Map::new();
        for (key, value) in fields {
            let key_str = key.as_str();
            if key_str == "transport" || (key_str == "auth" && is_legacy_oauth(value)) {
                split.dropped.push(format!("{name}.{key}"));
                continue;
            }
            let moves = ADAPTER_SERVER_KEYS.contains(&key_str)
                || (key_str == "auth" && !value.is_object())
                || (key_str == "oauth" && value == &Value::Bool(false))
                || (!complete && (OVERRIDE_KEYS.contains(&key_str) || key_str == "auth"));
            if moves && !NATIVE_PARTIAL_KEYS.contains(&key_str) {
                adapter.insert(key.clone(), value.clone());
            } else {
                native.insert(key.clone(), value.clone());
            }
        }
        if !adapter.is_empty() {
            split.servers.insert(name.clone(), Value::Object(adapter));
        }
        if !native.is_empty() {
            kept_servers.insert(name.clone(), Value::Object(native));
        }
    }
    split
        .native
        .insert("mcpServers".to_string(), Value::Object(kept_servers));
    split.dropped.sort();
    split
}

fn is_legacy_oauth(value: &Value) -> bool {
    value.as_object().is_some_and(|auth| {
        auth.len() == 1 && auth.get("type").and_then(|t| t.as_str()) == Some("oauth")
    })
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
#[cfg(test)]
mod tests {
    use super::*;

    fn json(text: &str) -> Value {
        serde_json::from_str(text).unwrap()
    }

    const SEARCH: bool = true;

    #[test]
    fn a_stdio_server_is_reached_on_demand_and_needs_no_override() {
        let (shared, overrides, warnings) =
            adapter("s", &json(r#"{"command":"npx","args":["-y","p"]}"#), SEARCH);
        assert_eq!(shared["lifecycle"], "eager");
        assert_eq!(
            shared["directTools"], "search",
            "every direct tool sits in the prompt on every turn"
        );
        assert!(
            overrides.is_empty(),
            "nothing here is wrong for Claude Code"
        );
        assert!(warnings.is_empty());
    }

    #[test]
    fn an_adapter_not_known_to_take_search_falls_back_to_the_proxy() {
        let (shared, _, _) = adapter("s", &json(r#"{"command":"npx"}"#), false);
        assert!(
            shared.get("directTools").is_none(),
            "an older adapter would not know \"search\"; the proxy is its on-demand"
        );
    }

    #[test]
    fn a_small_named_set_stays_direct() {
        let (shared, _, _) = adapter("context7", &json(r#"{"command":"npx"}"#), SEARCH);
        assert_eq!(shared["directTools"], true);
        let (_, defaults) = (
            native("context7", &json(r#"{"command":"npx"}"#)),
            native_defaults("context7"),
        );
        assert_eq!(defaults["exposure"], "direct");
    }

    #[test]
    fn lifecycle_follows_the_transport_and_not_the_exposure() {
        let (stdio, _, _) = adapter("s", &json(r#"{"command":"npx"}"#), false);
        let (remote, _, _) = adapter("r", &json(r#"{"url":"https://x/mcp"}"#), SEARCH);
        assert_eq!(stdio["lifecycle"], "eager");
        assert_eq!(remote["lifecycle"], "lazy");
        assert_eq!(remote["directTools"], "search");
    }

    #[test]
    fn a_remote_server_needs_no_auth_override() {
        let (_, overrides, _) = adapter(
            "s",
            &json(r#"{"type":"http","url":"https://x/api"}"#),
            SEARCH,
        );
        assert!(
            overrides.is_empty(),
            "a remote server with no credentials needs no auth key - undefined means deferred OAuth"
        );
    }

    #[test]
    fn an_sse_server_records_its_transport_as_an_override() {
        let (_, overrides, _) = adapter(
            "s",
            &json(r#"{"type":"sse","url":"https://x/sse"}"#),
            SEARCH,
        );
        assert_eq!(overrides["httpTransport"], "sse");
    }

    #[test]
    fn a_headers_helper_becomes_a_bang_command_in_the_adapter_file() {
        let (shared, overrides, warnings) = adapter(
            "sentry",
            &json(
                r#"{"type":"http","url":"https://x/api","headersHelper":".agents/mcp/dotenv-header.sh SENTRY_AUTH_TOKEN"}"#,
            ),
            SEARCH,
        );
        let command = overrides["headers"]["Authorization"].as_str().unwrap();
        assert!(command.starts_with('!'), "got {command}");
        assert!(command.contains("--raw"));
        assert!(command.contains("SENTRY_AUTH_TOKEN"));
        // The shared file keeps headersHelper for Claude Code; only Pi sees this.
        assert!(shared.get("headers").is_none());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("SENTRY_AUTH_TOKEN"));
        assert!(warnings[0].contains(paths::PI_MCP_ADAPTER));
    }

    #[test]
    fn a_custom_header_name_is_honoured() {
        let (_, overrides, _) = adapter(
            "s",
            &json(r#"{"url":"https://x","headersHelper":"h.sh --header X-Api-Key API_KEY"}"#),
            SEARCH,
        );
        assert!(overrides["headers"].get("X-Api-Key").is_some());
    }

    #[test]
    fn a_hand_written_server_with_a_placeholder_gets_a_wrapper_override() {
        let (shared, overrides, warnings) = adapter(
            "custom",
            &json(r#"{"command":"node","args":["server.js","--key=${API_KEY}"]}"#),
            SEARCH,
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
        let (_, overrides, warnings) = adapter("supabase", definition, SEARCH);
        assert!(
            overrides.is_empty(),
            "the shipped presets already use the wrapper, so nothing has to be rewritten"
        );
        assert!(warnings.is_empty());
    }

    #[test]
    fn a_native_definition_is_complete_and_carries_no_adapter_key() {
        let (stdio, _) = native(
            "custom",
            &json(r#"{"command":"node","args":["s.js","--key=${API_KEY}"],"env":{"MODE":"x"}}"#),
        );
        let stdio = stdio.unwrap();
        assert_eq!(
            stdio["command"], WRAPPER,
            "Pi never interpolates args either"
        );
        assert_eq!(stdio["env"]["MODE"], "x");

        let (remote, warnings) = native(
            "pixellab",
            &json(
                r#"{"type":"http","url":"https://x/mcp","headersHelper":".agents/mcp/dotenv-header.sh TOKEN"}"#,
            ),
        );
        let remote = remote.unwrap();
        assert_eq!(remote["url"], "https://x/mcp");
        assert!(remote["headers"]["Authorization"]
            .as_str()
            .unwrap()
            .starts_with('!'));
        assert!(remote.get("type").is_none());
        assert!(warnings[0].contains(paths::PI_MCP));

        for entry in [&stdio, &remote] {
            for key in entry.keys() {
                assert!(
                    !ADAPTER_SERVER_KEYS.contains(&key.as_str()),
                    "{key} is an adapter key"
                );
            }
        }
    }

    #[test]
    fn native_leaves_out_an_sse_server_it_cannot_run_and_says_so() {
        let (converted, warnings) = native("s", &json(r#"{"type":"sse","url":"https://x/sse"}"#));
        assert!(converted.is_none());
        assert!(warnings[0].contains("SSE"));
    }

    #[test]
    fn every_shipped_preset_converts_for_both_clients() {
        let catalogue = crate::testing::catalogue();
        for preset in &catalogue.presets {
            let pairs = [(preset.name.as_str(), &preset.servers)];
            let native = convert(&pairs, Target::Native);
            for (name, entry) in &native.native {
                let remote = entry.get("url").is_some();
                assert!(
                    remote != entry.get("command").is_some(),
                    "{}/{name}: exactly one transport",
                    preset.name
                );
                assert!(native.native_defaults[name].get("exposure").is_some());
            }
            let adapter = convert(&pairs, Target::Adapter { search: true });
            for (name, definition) in &preset.servers {
                let remote = definition.get("url").is_some();
                let overrides = adapter.overrides.get(name);
                assert!(
                    !(remote && overrides.is_some_and(|o| o.get("command").is_some())),
                    "{}/{name}: an override must never change the transport kind",
                    preset.name
                );
            }
        }
    }

    #[test]
    fn carry_keeps_every_choice_and_lets_the_preset_own_the_definition() {
        let previous = json(
            r#"{"command":"old","args":["a"],"directTools":false,"lifecycle":"lazy",
                "approveTools":["x_*"],"includeTools":["a"],"env":{"K":"v"}}"#,
        );
        let definition = json(r#"{"command":"new","args":["b"]}"#);
        let mut fresh = definition.as_object().unwrap().clone();
        let warnings = carry("s", ".mcp.json", &previous, &definition, &mut fresh);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(fresh["command"], "new");
        assert_eq!(fresh["args"][0], "b");
        assert_eq!(fresh["directTools"], false);
        assert_eq!(fresh["lifecycle"], "lazy");
        assert_eq!(fresh["approveTools"][0], "x_*");
        assert_eq!(fresh["env"]["K"], "v", "still stdio, so env still applies");

        fill(
            &mut fresh,
            Some(&json(
                r#"{"directTools":"search","lifecycle":"eager","exposeResources":true}"#,
            )),
            None,
        );
        assert_eq!(
            fresh["directTools"], false,
            "a default never replaces a choice"
        );
        assert_eq!(
            fresh["exposeResources"], true,
            "it only fills a missing one"
        );
    }

    #[test]
    fn a_choice_in_a_higher_layer_is_not_shadowed_by_a_default() {
        let mut entry = Map::new();
        fill(
            &mut entry,
            Some(&json(r#"{"directTools":"search"}"#)),
            Some(&json(r#"{"directTools":["one"]}"#)),
        );
        assert!(entry.get("directTools").is_none());
    }

    #[test]
    fn a_transport_change_drops_the_old_transport_and_its_credentials_and_says_so() {
        let previous =
            json(r#"{"command":"npx","args":["x"],"env":{"TOKEN":"${T}"},"directTools":false}"#);
        let definition = json(r#"{"type":"http","url":"https://x/mcp"}"#);
        let mut fresh = definition.as_object().unwrap().clone();
        let warnings = carry("s", ".mcp.json", &previous, &definition, &mut fresh);
        assert!(fresh.get("command").is_none());
        assert!(fresh.get("args").is_none());
        assert!(fresh.get("env").is_none());
        assert_eq!(fresh["directTools"], false, "an exposure choice survives");
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(warnings[0].contains("env"));
        assert!(warnings[1].contains("directTools"));
    }

    #[test]
    fn a_new_url_does_not_inherit_the_old_urls_credentials() {
        let previous = json(
            r#"{"url":"https://old/mcp","headers":{"Authorization":"!secret"},"oauth":{"clientId":"c"},"exposure":"hidden"}"#,
        );
        let definition = json(r#"{"url":"https://new/mcp"}"#);
        let mut fresh = definition.as_object().unwrap().clone();
        let warnings = carry("s", paths::PI_MCP, &previous, &definition, &mut fresh);
        assert!(fresh.get("headers").is_none());
        assert!(fresh.get("oauth").is_none());
        assert_eq!(fresh["exposure"], "hidden");
        assert!(warnings.iter().any(|w| w.contains("headers")));

        let same = json(r#"{"url":"https://old/mcp"}"#);
        let mut fresh = same.as_object().unwrap().clone();
        assert!(carry("s", paths::PI_MCP, &previous, &same, &mut fresh).is_empty());
        assert_eq!(
            fresh["oauth"]["clientId"], "c",
            "same url keeps its own credentials"
        );
    }

    #[test]
    fn an_adapter_override_keeps_its_choices_but_not_a_stale_wrapper() {
        let previous = json(
            r#"{"command":".agents/mcp/with-dotenv.sh","args":["--","old"],"directTools":["a"]}"#,
        );
        let definition = json(r#"{"command":"npx","args":["new"]}"#);
        let mut fresh = Map::new();
        carry(
            "s",
            paths::PI_MCP_ADAPTER,
            &previous,
            &definition,
            &mut fresh,
        );
        assert!(
            fresh.get("command").is_none(),
            "the old wrapper names the old args"
        );
        assert_eq!(fresh["directTools"][0], "a");
    }

    #[test]
    fn a_legacy_adapter_file_splits_into_pi_and_adapter_halves() {
        let split = split_legacy(&json(
            r#"{
                "settings": {"toolPrefix": "short"},
                "imports": ["cursor"],
                "autoEnableCodemode": false,
                "mcpServers": {
                    "remote": {
                        "lifecycle": "lazy",
                        "headers": {"Authorization": "!h.sh --raw TOKEN"}
                    },
                    "local": {
                        "command": "node",
                        "args": ["x.js"],
                        "directTools": true,
                        "approveTools": "destructive",
                        "exposure": "deferred"
                    },
                    "old": {"url": "https://x", "transport": "sse", "auth": {"type": "oauth"}},
                    "toggled": {"enabled": false}
                }
            }"#,
        ));
        assert_eq!(split.settings["settings"]["toolPrefix"], "short");
        assert_eq!(split.settings["imports"][0], "cursor");
        assert_eq!(split.native["autoEnableCodemode"], false);

        let native = &split.native["mcpServers"];
        assert!(
            native.get("remote").is_none(),
            "a partial adapter override moves whole"
        );
        assert_eq!(
            split.servers["remote"]["headers"]["Authorization"],
            "!h.sh --raw TOKEN"
        );
        assert_eq!(native["local"]["command"], "node");
        assert_eq!(native["local"]["exposure"], "deferred");
        assert!(native["local"].get("directTools").is_none());
        assert_eq!(split.servers["local"]["directTools"], true);
        assert_eq!(split.servers["local"]["approveTools"], "destructive");
        assert_eq!(native["old"], json(r#"{"url":"https://x"}"#));
        assert_eq!(split.dropped, vec!["old.auth", "old.transport"]);
        assert_eq!(
            native["toggled"]["enabled"], false,
            "a Pi override is not moved"
        );
        assert!(split.servers.get("toggled").is_none());
    }

    #[test]
    fn valid_native_config_is_not_split_at_all() {
        for text in [
            r#"{"mcpServers":{"docs":{"url":"https://x/mcp","exposure":"deferred","oauth":{"clientId":"c"}}}}"#,
            r#"{"mcpServers":{"radius":{"url":"https://x/mcp","auth":{"provider":"radius"}}}}"#,
            r#"{"mcpServers":{"fs":{"command":"npx","args":["fs"],"toolExposure":{"read_*":"direct"}},"user-one":{"enabled":false}}}"#,
        ] {
            let split = split_legacy(&json(text));
            assert!(split.is_empty(), "{text}: {split:?}");
            assert_eq!(Value::Object(split.native), json(text));
        }
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
}
