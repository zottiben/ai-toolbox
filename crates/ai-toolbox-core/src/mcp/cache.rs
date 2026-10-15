//! Versioned adapter cache reader. A cache miss is unknown, never a zero-tool server.
use super::config::{Backend, Context};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

pub(super) fn tools(
    context: &Context,
    definition: &Value,
    entry: &Value,
) -> (Option<Vec<String>>, String) {
    let unknown = |why: &str| (None, why.to_string());
    if context.backend == Backend::Native {
        return unknown("Native Pi has no compatible persisted tool catalogue; not connected.");
    }
    let manifest = context.adapter_manifest_path();
    let version = std::fs::read_to_string(manifest)
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|d| d["version"].as_str().map(str::to_owned));
    if !version.is_some_and(|v| v.starts_with("5.3.")) {
        return unknown("Cache identity format not verified for this adapter version.");
    }
    if entry.is_null() {
        return unknown("No cached metadata; no server was started.");
    }
    if entry["cacheScope"] == "private" || entry["discoveryFailed"] == true {
        return unknown("Private or failed discovery metadata is not reusable.");
    }
    let Some(cached_at) = entry["cachedAt"].as_u64() else {
        return unknown("Metadata has no capture time.");
    };
    if cached_at == 0 {
        return unknown("Metadata has no capture time.");
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    if entry["ttlMs"]
        .as_u64()
        .is_some_and(|ttl| ttl == 0 || now.saturating_sub(cached_at) >= ttl)
    {
        return unknown("Cached metadata expired.");
    }
    let Some(identity) = identity(context, definition) else {
        return unknown(
            "Dynamic credentials, paths or plugin identity cannot be verified offline.",
        );
    };
    if entry["configHash"].as_str() != Some(&identity) {
        return unknown("Cache belongs to a different effective server definition.");
    }
    if entry["resources"].as_array().is_some_and(|r| !r.is_empty()) {
        return unknown("Resource-generated tools require live catalogue verification.");
    }
    let Some(tools) = entry["tools"].as_array() else {
        return unknown("No cached tool list.");
    };
    for selector in definition["includeTools"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(definition["excludeTools"].as_array().into_iter().flatten())
    {
        if selector != "*" && !tools.iter().any(|t| t["name"] == *selector) {
            return unknown(
                "Prefixed or patterned tool filters require live selector verification.",
            );
        }
    }
    let mut names = Vec::new();
    for tool in tools {
        let Some(name) = tool["name"].as_str() else {
            return unknown("Malformed cached tool name.");
        };
        if tool.get("uiVisibility").is_some() {
            return unknown("UI-specific tool visibility requires live catalogue verification.");
        }
        let includes = definition["includeTools"].as_array();
        let excludes = definition["excludeTools"].as_array();
        if includes.is_some_and(|items| {
            !items.is_empty()
                && !items
                    .iter()
                    .any(|v| v.as_str().is_some_and(|p| matches(p, name)))
        }) {
            continue;
        }
        if excludes.is_some_and(|items| {
            items
                .iter()
                .any(|v| v.as_str().is_some_and(|p| matches(p, name)))
        }) {
            continue;
        }
        names.push(name.to_string());
    }
    (
        Some(names),
        "Valid cached metadata (offline; not a live connection).".into(),
    )
}

pub(super) fn matches(pattern: &str, text: &str) -> bool {
    let parts: Vec<_> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == text;
    }
    let mut remaining = text;
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            let Some(rest) = remaining.strip_prefix(part) else {
                return false;
            };
            remaining = rest;
        } else if i == parts.len() - 1 {
            return remaining.ends_with(part);
        } else if let Some(at) = remaining.find(part) {
            remaining = &remaining[at + part.len()..];
        } else {
            return false;
        }
    }
    true
}

pub(super) fn identity(context: &Context, definition: &Value) -> Option<String> {
    if definition.get("bearerTokenStore").is_some() {
        return None;
    }
    let mut value = Map::new();
    let mut keys = vec![
        "command",
        "args",
        "socket",
        "env",
        "cwd",
        "url",
        "headers",
        "requestHeadersCommand",
        "auth",
        "protocolVersion",
        "bearerToken",
        "bearerTokenEnv",
        "exposeResources",
        "includeTools",
        "excludeTools",
    ];
    for key in keys.iter().copied() {
        if let Some(field) = definition.get(key) {
            let text = field.to_string();
            if text.contains("${")
                || text.contains("\"!")
                || key == "bearerTokenEnv"
                || key == "requestHeadersCommand"
            {
                return None;
            }
            let field = if ["command", "socket", "cwd"].contains(&key) {
                if let Some(path) = field.as_str().and_then(|s| s.strip_prefix("~/")) {
                    json!(context.home.join(path))
                } else {
                    field.clone()
                }
            } else {
                field.clone()
            };
            value.insert(key.into(), field);
        }
    }
    if definition["command"]
        .as_str()
        .is_some_and(|s| !s.is_empty())
    {
        keys.extend(["inheritEnv", "literalEnv"]);
        value.insert(
            "inheritEnv".into(),
            json!(definition["inheritEnv"] != false),
        );
        value.insert("literalEnv".into(), json!(definition["literalEnv"] == true));
    }
    fn sorted(value: Value) -> Value {
        match value {
            Value::Object(map) => {
                let mut pairs: Vec<_> = map.into_iter().collect();
                pairs.sort_by(|a, b| a.0.cmp(&b.0));
                Value::Object(pairs.into_iter().map(|(k, v)| (k, sorted(v))).collect())
            }
            Value::Array(values) => Value::Array(values.into_iter().map(sorted).collect()),
            other => other,
        }
    }
    // Adapter stableStringify deliberately includes undefined object fields. Ordinary
    // JSON.stringify omits them and produces a different, unusable cache identity.
    keys.sort();
    let fields: Vec<_> = keys
        .into_iter()
        .map(|key| {
            format!(
                "{}:{}",
                json!(key),
                value
                    .get(key)
                    .map(|v| sorted(v.clone()).to_string())
                    .unwrap_or_else(|| "undefined".into())
            )
        })
        .collect();
    let normalized = format!("{{{}}}", fields.join(","));
    Some(format!("{:x}", Sha256::digest(normalized.as_bytes())))
}
