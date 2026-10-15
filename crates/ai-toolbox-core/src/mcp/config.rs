use crate::{catalogue::read_json, machine, Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Backend {
    Adapter,
    Native,
}

#[derive(Debug, Clone)]
pub struct Context {
    pub home: PathBuf,
    pub agent_dir: PathBuf,
    pub backend: Backend,
    pub adapter_manifest: Option<PathBuf>,
}
impl Context {
    /// Storage paths only. Configuration is detected afresh for each project request.
    pub fn storage() -> Self {
        let home = machine::home();
        let home = home.canonicalize().unwrap_or(home);
        let agent_dir = machine::pi_agent_dir(&home);
        let agent_dir = agent_dir.canonicalize().unwrap_or(agent_dir);
        Self {
            home,
            agent_dir,
            backend: Backend::Native,
            adapter_manifest: None,
        }
    }
    pub fn current_for(repo: &Path) -> Result<Self> {
        Self::storage().for_project(repo)
    }
    pub fn for_project(&self, repo: &Path) -> Result<Self> {
        let context = self.detected(Some(repo))?;
        if context.backend == Backend::Native
            && crate::machine::native_pi_version()
                .is_some_and(|v| v < semver::Version::new(0, 99, 0))
        {
            return Err(Error::Catalogue(
                "This Pi predates built-in MCP; run ai-toolbox pi-init first".into(),
            ));
        }
        Ok(context)
    }
    pub fn detected(&self, repo: Option<&Path>) -> Result<Self> {
        let detected = super::detect::detect(self, repo)?;
        Ok(Self {
            backend: detected.backend,
            adapter_manifest: detected.manifest,
            ..self.clone()
        })
    }
    pub fn adapter_manifest_path(&self) -> PathBuf {
        self.adapter_manifest.clone().unwrap_or_else(|| {
            self.agent_dir
                .join("npm/node_modules/pi-mcp-adapter/package.json")
        })
    }
    pub fn install_target(&self) -> crate::convert::pi::Target {
        if self.backend == Backend::Native {
            return crate::convert::pi::Target::Native;
        }
        crate::convert::pi::Target::Adapter {
            search: self
                .adapter_version()
                .is_some_and(|v| v >= semver::Version::new(2, 33, 0)),
        }
    }
    pub fn adapter_version(&self) -> Option<semver::Version> {
        document(&self.adapter_manifest_path()).ok().and_then(|d| {
            d["version"]
                .as_str()
                .and_then(|s| semver::Version::parse(s).ok())
        })
    }
    pub fn with_backend(mut self, backend: Option<Backend>) -> Self {
        if let Some(backend) = backend {
            self.backend = backend;
        }
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    pub name: String,
    pub enabled: bool,
    pub exposure: String,
    pub source: PathBuf,
    pub exposure_source: PathBuf,
    pub sources: Vec<PathBuf>,
    pub tools: Option<usize>,
    pub direct_tools: Option<usize>,
    pub tool_names: Vec<String>,
    pub cache_note: String,
    pub recommend_search: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    /// File configuration only, before Pi's project trust and runtime/session overrides.
    pub complete: bool,
    pub repo: PathBuf,
    pub backend: Backend,
    pub servers: Vec<Server>,
    pub known_direct_tools: usize,
    pub unknown_catalogues: usize,
    pub warnings: Vec<String>,
}
#[derive(Clone)]
pub(super) struct Entry {
    pub value: Value,
    pub source: PathBuf,
    pub exposure_source: PathBuf,
    pub sources: Vec<PathBuf>,
}
pub(super) struct Loaded {
    pub entries: BTreeMap<String, Entry>,
    pub settings: Value,
    pub files: Vec<PathBuf>,
    pub warnings: Vec<String>,
}

pub(super) fn document(path: &Path) -> Result<Value> {
    if path.is_file() {
        read_json(path)
    } else {
        Ok(json!({}))
    }
}

pub(super) fn load(context: &Context, repo: &Path) -> Result<Loaded> {
    load_scope(context, repo, true)
}
pub(super) fn load_global(context: &Context) -> Result<Loaded> {
    load_scope(context, &context.home, false)
}
fn load_scope(context: &Context, repo: &Path, project: bool) -> Result<Loaded> {
    let mut projects = if project {
        vec![
            repo.join(".mcp.json"),
            repo.join(".pi/mcp.json"),
            repo.join(".pi/mcp-adapter.json"),
        ]
    } else {
        vec![]
    };
    let mut out = Loaded {
        entries: BTreeMap::new(),
        settings: json!({}),
        files: vec![context.agent_dir.join("settings.json")],
        warnings: vec![],
    };
    if project {
        out.files.push(repo.join(".pi/settings.json"));
    }
    if let Some(manifest) = &context.adapter_manifest {
        out.files.push(manifest.clone());
    }
    if context.backend == Backend::Native {
        for path in std::iter::once(context.agent_dir.join("mcp.json"))
            .chain(project.then(|| repo.join(".pi/mcp.json")))
        {
            let doc = document(&path)?;
            layer(&mut out, &path, &doc, true, false)?;
        }
        return Ok(out);
    }
    flag_package_catalogues(context, repo, project, &mut out)?;
    let native_enabled =
        crate::machine::native_pi_version().is_none_or(|v| v >= semver::Version::new(0, 99, 0));
    if !native_enabled {
        projects.retain(|p| p != &repo.join(".pi/mcp.json"));
    }
    let globals: Vec<_> = [
        context.home.join(".config/mcp/mcp.json"),
        context.home.join(".agents/mcp.json"),
        context.home.join(".agents/mcp/mcp.json"),
        context.agent_dir.join("mcp.json"),
        context.agent_dir.join("mcp-adapter.json"),
    ]
    .into_iter()
    .filter(|p| native_enabled || p != &context.agent_dir.join("mcp.json"))
    .collect();
    // Imports are compatibility inputs below file-defined servers. Only explicitly opted-in
    // imports are read; unknown/plugin discovery is reported rather than executed.
    let mut imports = Vec::new();
    if std::env::var_os("PI_MCP_CONFIG").is_some()
        || std::env::var_os("PI_MCP_CONFIG_MODE").is_some()
        || std::env::var_os("MCP_DIRECT_TOOLS").is_some()
    {
        out.warnings.push("Adapter config/exposure environment overrides are active; file-only inventory is incomplete and optimisation is unavailable.".into());
    }
    for path in globals
        .iter()
        .chain(projects.iter())
        .filter(|p| *p != &context.agent_dir.join("mcp.json") && *p != &repo.join(".pi/mcp.json"))
    {
        let doc = document(path)?;
        for import in doc["imports"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if !imports.contains(&import.to_string()) {
                imports.push(import.to_string());
            }
        }
    }
    if !imports.is_empty() {
        out.warnings.push("Imported host configuration is shown as a partial inventory. Its per-source precedence is not yet certified; optimisation is unavailable.".into());
    }
    for import in imports {
        let paths = match import.as_str() {
            "claude-code" => vec![
                context.home.join(".claude/mcp.json"),
                context.home.join(".claude.json"),
                context.home.join(".claude/claude_desktop_config.json"),
            ],
            "claude-desktop" => vec![context
                .home
                .join("Library/Application Support/Claude/claude_desktop_config.json")],
            "codex" => vec![
                context.home.join(".codex/config.toml"),
                context.home.join(".codex/config.json"),
            ],
            "cursor" => vec![context.home.join(".cursor/mcp.json")],
            _ => {
                out.warnings.push(format!("Unsupported import {import}: effective inventory may be incomplete; optimisation is unavailable."));
                vec![]
            }
        };
        for path in paths {
            if path.extension().is_some_and(|v| v == "toml") {
                out.files.push(path.clone());
                if !path.is_file() {
                    continue;
                }
                let text = std::fs::read_to_string(&path).map_err(|e| Error::io(&path, e))?;
                let toml: toml::Value = toml::from_str(&text).map_err(|e| Error::toml(&path, e))?;
                let mut doc = json!({"mcpServers":{}});
                if let Some(servers) = toml.get("mcp_servers").and_then(toml::Value::as_table) {
                    for (name, server) in servers {
                        let mut value =
                            serde_json::to_value(server).map_err(|e| Error::json(&path, e))?;
                        if let Some(token) = value.get("bearer_token_env_var").cloned() {
                            value["bearerTokenEnv"] = token;
                        }
                        doc["mcpServers"][name] = value;
                    }
                }
                layer(&mut out, &path, &doc, false, true)?;
            } else {
                let doc = document(&path)?;
                layer(&mut out, &path, &doc, false, true)?;
                if let Some(project) = doc["projects"].get(repo.to_string_lossy().as_ref()) {
                    layer(&mut out, &path, project, false, true)?;
                }
            }
        }
    }
    let project_native = if project && native_enabled {
        document(&repo.join(".pi/mcp.json"))?
    } else {
        json!({})
    };
    for path in &globals {
        let native = path == &context.agent_dir.join("mcp.json");
        let mut doc = document(path)?;
        if native {
            if let Some(servers) = doc.get_mut("mcpServers").and_then(Value::as_object_mut) {
                for (name, definition) in project_native["mcpServers"]
                    .as_object()
                    .into_iter()
                    .flatten()
                {
                    if definition.get("command").is_some() || definition.get("url").is_some() {
                        servers.remove(name);
                    }
                }
            }
        }
        layer(&mut out, path, &doc, native, true)?;
    }
    // Ancestors are opt-in. Refuse unsupported effective layers instead of presenting a
    // reassuring but incorrect report or writing around them.
    if out
        .settings
        .get("ancestorConfigRoots")
        .is_some_and(|v| v.as_array().is_some_and(|v| !v.is_empty()))
    {
        out.warnings.push("Ancestor config discovery is configured: inspect in Pi; optimisation is unavailable until these layers are explicitly supported.".into());
    }
    let project_sources = projects.clone();
    for path in projects {
        layer(
            &mut out,
            &path,
            &document(&path)?,
            path == repo.join(".pi/mcp.json"),
            true,
        )?;
    }
    let warnings = &mut out.warnings;
    out.entries.retain(|name, entry| {
        if entry.value["auth"].is_object() && entry.sources.iter().any(|p| project_sources.contains(p)) {
            warnings.push(format!("{name}: the adapter removes provider-auth servers named by a project file. Resolve its global/project authority in Pi; optimisation is unavailable."));
            false
        } else { true }
    });
    Ok(out)
}

fn flag_package_catalogues(
    context: &Context,
    repo: &Path,
    project: bool,
    out: &mut Loaded,
) -> Result<()> {
    for directory in
        std::iter::once(context.agent_dir.clone()).chain(project.then(|| repo.join(".pi")))
    {
        let path = directory.join("settings.json");
        out.files.push(path.clone());
        let settings = document(&path)?;
        for package in settings["packages"].as_array().into_iter().flatten() {
            let Some(source) = package.as_str().or_else(|| package["source"].as_str()) else {
                continue;
            };
            let root = if let Some(npm) = source.strip_prefix("npm:") {
                let end = npm.rfind('@').filter(|i| *i > 0).unwrap_or(npm.len());
                directory.join("npm/node_modules").join(&npm[..end])
            } else if source.contains("://")
                || source.starts_with("git:")
                || source.starts_with("git@")
            {
                out.warnings.push("Git-sourced Pi packages may provide additional MCP definitions; file-only inventory is incomplete and optimisation is unavailable.".into());
                continue;
            } else if let Some(path) = source.strip_prefix("~/") {
                context.home.join(path)
            } else {
                directory.join(source)
            };
            let manifest = root.join("package.json");
            out.files.push(manifest.clone());
            if document(&manifest)?["pi"].get("mcp").is_some() {
                out.warnings.push(format!("{} supplies package-owned MCP definitions; inventory is incomplete and optimisation is unavailable.", manifest.display()));
            }
        }
    }
    Ok(())
}

fn layer(out: &mut Loaded, path: &Path, doc: &Value, native: bool, adapter: bool) -> Result<()> {
    if !out.files.contains(&path.to_path_buf()) {
        out.files.push(path.into());
    }
    if !doc.is_object() {
        return Err(Error::Catalogue(format!(
            "{} must contain an object",
            path.display()
        )));
    }
    if let Err(error) = crate::merge::require_canonical_mcp(doc, path) {
        out.warnings.push(error.to_string());
    }
    if doc.get("claudePlugins").is_some()
        || doc["settings"].get("agentPluginPaths").is_some()
        || doc["settings"]["hostConfigDiscovery"] == "on"
    {
        out.warnings.push(format!("{} enables dynamic plugin/host discovery: inventory is incomplete; optimisation is unavailable.", path.display()));
    }
    if !native {
        if let Some(settings) = doc["settings"].as_object() {
            for (key, value) in settings {
                out.settings[key] = value.clone();
            }
            if let Some(exposure) = settings.get("directTools") {
                // A settings default is filled only after all per-server layers have merged.
                out.settings["_exposureSource"] = json!(path);
                out.settings["directTools"] = exposure.clone();
            }
        }
    }
    let Some(servers) = doc.get("mcpServers") else {
        return Ok(());
    };
    let servers = servers.as_object().ok_or_else(|| {
        Error::Catalogue(format!("{}.mcpServers must be an object", path.display()))
    })?;
    for (name, raw) in servers {
        if !raw.is_object() {
            return Err(Error::Catalogue(format!(
                "{}: {name} must be an object",
                path.display()
            )));
        }
        if native
            && raw.get("exposure").is_some_and(|v| {
                ![
                    "direct",
                    "deferred",
                    "hidden",
                    "codemode",
                    "codemode-deferred",
                ]
                .iter()
                .any(|s| v.as_str() == Some(s))
            })
        {
            out.warnings.push(format!("{name}: invalid native exposure; Pi skips this definition and optimisation is unavailable."));
            continue;
        }
        let mut value = raw.clone();
        let complete = value.get("command").is_some() || value.get("url").is_some();
        if native && !adapter && !complete {
            value
                .as_object_mut()
                .expect("object")
                .retain(|key, _| ["enabled", "exposure", "toolExposure"].contains(&key.as_str()));
        }
        if native && adapter {
            if !complete {
                out.warnings.push(format!("{name}: adapter 5.3 skips partial native mcp.json entries; move adapter overrides to mcp-adapter.json."));
                continue;
            }
            if raw.get("oauth").is_some_and(|v| !v.is_object()) {
                out.warnings.push(format!("{name}: native OAuth must be an object; migrate this legacy definition before optimising."));
                continue;
            }
            if value["type"] == "sse" {
                out.warnings
                    .push(format!("{name}: native SSE definition is rejected by Pi."));
                continue;
            }
            let fields = value.as_object_mut().expect("checked object");
            fields.retain(|key, _| {
                [
                    "command",
                    "args",
                    "env",
                    "cwd",
                    "url",
                    "headers",
                    "oauth",
                    "auth",
                    "enabled",
                    "exposure",
                    "toolExposure",
                    "description",
                ]
                .contains(&key.as_str())
            });
            if raw.get("auth").is_some() || raw.get("oauth").is_some() {
                out.warnings.push(format!("{name}: native credential translation needs live Pi verification; optimisation is unavailable."));
            }
            if fields.get("enabled") == Some(&Value::Bool(false)) {
                fields.insert("disabled".into(), json!(true));
            }
            if let Some(exposure) = fields
                .get("exposure")
                .and_then(Value::as_str)
                .map(str::to_owned)
            {
                match exposure.as_str() {
                    "direct" => {
                        fields.insert("directTools".into(), json!(true));
                    }
                    "deferred" => {
                        fields.insert("directTools".into(), json!("search"));
                    }
                    "hidden" => {
                        fields.insert("disabled".into(), json!(true));
                    }
                    _ => {} // Adapter codemode sets no override; lower direct choices still apply.
                }
            }
            if fields.get("toolExposure").is_some() {
                out.warnings.push(format!("{name}: native per-tool exposure requires inspection in Pi; optimisation is unavailable."));
            }
            for key in ["enabled", "exposure", "toolExposure"] {
                fields.remove(key);
            }
        }
        let mut entry = out.entries.remove(name).unwrap_or_else(|| Entry {
            value: json!({}),
            source: path.into(),
            exposure_source: path.into(),
            sources: vec![],
        });
        if native && complete && !adapter {
            entry.value = json!({});
            entry.exposure_source = path.into();
        }
        // Mirror transport replacement without inheriting URL-bound credentials.
        let strip: &[&str] = if value.get("command").is_some() {
            &[
                "url",
                "headers",
                "requestHeadersCommand",
                "caFile",
                "auth",
                "bearerToken",
                "bearerTokenEnv",
                "bearerTokenStore",
                "oauth",
                "httpTransport",
                "socket",
            ]
        } else if value.get("url").is_some() {
            &[
                "command",
                "args",
                "env",
                "cwd",
                "pluginDataDir",
                "literalEnv",
                "inheritEnv",
                "socket",
            ]
        } else if value.get("socket").is_some() {
            &[
                "command",
                "args",
                "env",
                "cwd",
                "pluginDataDir",
                "literalEnv",
                "inheritEnv",
                "url",
                "headers",
                "requestHeadersCommand",
                "caFile",
                "auth",
                "bearerToken",
                "bearerTokenEnv",
                "bearerTokenStore",
                "oauth",
                "httpTransport",
            ]
        } else {
            &[]
        };
        for key in strip {
            entry.value.as_object_mut().expect("object").remove(*key);
        }
        if value
            .get("url")
            .is_some_and(|url| entry.value.get("url") != Some(url))
        {
            for key in [
                "headers",
                "bearerToken",
                "bearerTokenEnv",
                "bearerTokenStore",
                "requestHeadersCommand",
                "caFile",
            ] {
                entry.value.as_object_mut().expect("object").remove(key);
            }
            if entry.value["auth"].is_object() {
                entry.value.as_object_mut().expect("object").remove("auth");
            }
            if entry.value["oauth"] != false {
                entry.value.as_object_mut().expect("object").remove("oauth");
            }
        }
        for (key, value) in value.as_object().expect("checked object") {
            entry.value[key] = value.clone();
        }
        entry.source = path.into();
        if (adapter && value.get("directTools").is_some())
            || (!adapter && (raw.get("exposure").is_some() || raw.get("toolExposure").is_some()))
        {
            entry.exposure_source = path.into();
        }
        if !entry.sources.contains(&path.to_path_buf()) {
            entry.sources.push(path.into());
        }
        out.entries.insert(name.clone(), entry);
    }
    Ok(())
}

pub(crate) struct InstallChoices {
    pub servers: BTreeMap<String, Value>,
    pub defaults: Value,
    pub native_globals: std::collections::BTreeSet<String>,
}
pub(crate) fn install_choices(context: &Context, repo: &Path) -> Result<InstallChoices> {
    let loaded = load(context, repo)?;
    if !loaded.warnings.is_empty() {
        return Err(Error::Catalogue(format!(
            "Cannot safely preserve inherited Pi choices: {}",
            loaded.warnings.join("; ")
        )));
    }
    let mut defaults = json!({});
    if context.backend == Backend::Adapter {
        if let Some(value) = loaded.settings.get("directTools") {
            defaults["directTools"] = value.clone();
        }
    }
    let servers = loaded
        .entries
        .into_iter()
        .map(|(name, entry)| {
            let mut value = entry.value;
            if value.get("directTools").is_none() {
                if let Some(default) = defaults.get("directTools") {
                    value["directTools"] = default.clone();
                }
            }
            (name, value)
        })
        .collect();
    let native_globals = if context.backend == Backend::Native {
        load_global(context)?.entries.into_keys().collect()
    } else {
        Default::default()
    };
    Ok(InstallChoices {
        servers,
        defaults,
        native_globals,
    })
}

pub fn inspect(context: &Context, repo: &Path) -> Result<Report> {
    inspect_loaded(context, repo, &load(context, repo)?)
}
pub(super) fn inspect_loaded(context: &Context, repo: &Path, loaded: &Loaded) -> Result<Report> {
    let cache = document(&context.agent_dir.join("mcp-cache.json")).unwrap_or_else(|_| json!({}));
    let cache = if cache["version"] == 1 {
        cache
    } else {
        json!({})
    };
    let mut servers = Vec::new();
    for (name, entry) in &loaded.entries {
        let enabled = entry.value["disabled"] != true && entry.value["enabled"] != false;
        let exposure_value = if context.backend == Backend::Adapter {
            entry
                .value
                .get("directTools")
                .or_else(|| loaded.settings.get("directTools"))
                .cloned()
                .unwrap_or(json!(false))
        } else {
            entry
                .value
                .get("exposure")
                .cloned()
                .unwrap_or(json!("codemode"))
        };
        let exposure = match &exposure_value {
            Value::Bool(true) => "direct",
            Value::Array(_) => "selected",
            Value::String(s) if s == "direct" => "direct",
            Value::String(s) if s == "search" || s == "deferred" => "search",
            Value::String(s) if s == "hidden" => "hidden",
            Value::String(s) if s == "codemode" => "codemode",
            _ => "proxy",
        };
        let (names, note) = super::cache::tools(context, &entry.value, &cache["servers"][name]);
        let direct_tools =
            if !enabled || ["search", "proxy", "codemode", "hidden"].contains(&exposure) {
                Some(0)
            } else {
                names.as_ref().and_then(|names| {
                    if let Some(selected) = exposure_value.as_array() {
                        if selected
                            .iter()
                            .any(|v| !names.iter().any(|n| v.as_str() == Some(n)))
                        {
                            return None;
                        }
                        Some(
                            names
                                .iter()
                                .filter(|n| selected.iter().any(|v| v.as_str() == Some(n)))
                                .count(),
                        )
                    } else {
                        Some(names.len())
                    }
                })
            };
        let native_patterns = context.backend == Backend::Native
            && entry.value["toolExposure"]
                .as_object()
                .is_some_and(|m| m.values().any(|v| v == "direct"));
        servers.push(Server {
            name: name.clone(),
            enabled,
            exposure: if enabled {
                exposure.into()
            } else {
                "hidden".into()
            },
            source: entry.source.clone(),
            exposure_source: if entry.value.get("directTools").is_none()
                && context.backend == Backend::Adapter
            {
                loaded.settings["_exposureSource"]
                    .as_str()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| entry.exposure_source.clone())
            } else {
                entry.exposure_source.clone()
            },
            sources: entry.sources.clone(),
            tools: names.as_ref().map(Vec::len),
            direct_tools: if native_patterns { None } else { direct_tools },
            tool_names: names.unwrap_or_default(),
            cache_note: note,
            recommend_search: enabled
                && !crate::convert::pi::DIRECT_BY_DEFAULT.contains(&name.as_str())
                && ((context.backend == Backend::Native
                    && entry.value["toolExposure"]["*"] == "direct")
                    || (exposure == "direct" && direct_tools.is_none_or(|n| n > 8))),
        });
    }
    let known_direct_tools = servers.iter().filter_map(|s| s.direct_tools).sum();
    let unknown_catalogues = servers
        .iter()
        .filter(|s| s.enabled && s.tools.is_none())
        .count();
    Ok(Report {
        complete: loaded.warnings.is_empty(),
        repo: repo.into(),
        backend: context.backend,
        servers,
        known_direct_tools,
        unknown_catalogues,
        warnings: loaded.warnings.clone(),
    })
}
