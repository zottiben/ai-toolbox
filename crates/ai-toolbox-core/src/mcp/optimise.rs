use super::config::{inspect_loaded, load, load_global, Backend, Context, Report};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Scope {
    #[default]
    Project,
    Worktrees,
    All,
    Machine,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Options {
    pub scope: Scope,
    #[serde(default)]
    pub keep_direct: Vec<String>,
    #[serde(default)]
    pub keep_tools: BTreeMap<String, Vec<String>>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Change {
    pub path: PathBuf,
    pub server: String,
    pub before: Value,
    pub after: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preview {
    pub id: String,
    pub retained: bool,
    pub scope: Scope,
    pub reports: Vec<Report>,
    pub changes: Vec<Change>,
    pub warnings: Vec<String>,
}
#[derive(Debug, Serialize, Deserialize)]
struct Edit {
    path: PathBuf,
    before: Option<String>,
    after: String,
    mode: u32,
}
#[derive(Debug, Serialize, Deserialize)]
struct Saved {
    preview: Preview,
    edits: Vec<Edit>,
    inputs: BTreeMap<PathBuf, Option<String>>,
    applied: bool,
    rolled_back: bool,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Applied {
    pub id: String,
    pub files: usize,
    pub backup: PathBuf,
}

fn failure(message: impl Into<String>) -> Error {
    Error::Catalogue(message.into())
}
fn bytes(path: &Path) -> Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::io(path, e)),
    }
}
fn digest(path: &Path) -> Result<Option<String>> {
    Ok(bytes(path)?.map(|b| crate::hash::bytes(&b).to_string()))
}
fn no_links(path: &Path) -> Result<()> {
    for ancestor in path.ancestors() {
        if ancestor.is_symlink() {
            return Err(failure(format!(
                "Refusing symlink configuration path {}",
                ancestor.display()
            )));
        }
    }
    Ok(())
}
fn project_source(path: &Path, root: &Path) -> bool {
    [".mcp.json", ".pi/mcp.json", ".pi/mcp-adapter.json"]
        .iter()
        .any(|name| path == root.join(name))
}
fn state_dir(context: &Context) -> PathBuf {
    context.home.join(".ai-toolbox/mcp-optimisation")
}
fn state_path(context: &Context, id: &str) -> Result<PathBuf> {
    if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(failure("Invalid MCP preview id"));
    }
    Ok(state_dir(context).join(format!("{id}.json")))
}
fn atomic(path: &Path, data: &[u8], mode: u32) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    no_links(path)?;
    let parent = path
        .parent()
        .ok_or_else(|| failure("Missing parent directory"))?;
    std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| Error::io(parent, e))?;
    temp.as_file()
        .set_permissions(std::fs::Permissions::from_mode(mode))
        .map_err(|e| Error::io(path, e))?;
    temp.write_all(data).map_err(|e| Error::io(path, e))?;
    temp.as_file().sync_all().map_err(|e| Error::io(path, e))?;
    temp.persist(path).map_err(|e| Error::io(path, e.error))?;
    Ok(())
}
fn save(context: &Context, saved: &Saved) -> Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let dir = state_dir(context);
    no_links(&dir)?;
    std::fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| Error::io(&dir, e))?;
    let path = state_path(context, &saved.preview.id)?;
    atomic(
        &path,
        &serde_json::to_vec_pretty(saved).map_err(|e| Error::json(&path, e))?,
        0o600,
    )?;
    Ok(path)
}
struct ApplyLock {
    file: std::fs::File,
}
impl Drop for ApplyLock {
    fn drop(&mut self) {
        // Closing alone retains flock until every inherited/duplicated descriptor is
        // closed, so a surviving copy must not extend a completed operation's lock.
        if let Err(error) = self.file.unlock() {
            eprintln!("ai-toolbox: could not release MCP optimisation lock: {error}");
        }
    }
}
fn lock(context: &Context) -> Result<ApplyLock> {
    use std::os::unix::fs::OpenOptionsExt;
    let path = state_dir(context).join("apply.lock");
    no_links(&path)?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(&path)
        .map_err(|e| Error::io(&path, e))?;
    file.try_lock().map_err(|error| lock_error(&path, error))?;
    Ok(ApplyLock { file })
}
fn lock_error(path: &Path, error: std::fs::TryLockError) -> Error {
    match error {
        std::fs::TryLockError::WouldBlock => {
            failure("Another MCP apply/rollback is in progress. Try again once it finishes.")
        }
        std::fs::TryLockError::Error(error) => Error::io(path, error),
    }
}
#[cfg(test)]
#[test]
fn lock_io_errors_are_not_reported_as_another_writer() {
    let error = lock_error(
        Path::new("fixture.lock"),
        std::fs::TryLockError::Error(std::io::Error::other("fixture I/O failure")),
    );
    assert!(error.to_string().contains("fixture I/O failure"));
    assert!(!error.to_string().contains("in progress"));
}
#[cfg(test)]
#[test]
fn finishing_an_operation_releases_its_lock_even_if_a_child_inherits_the_descriptor() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path().canonicalize().unwrap();
    let context = Context {
        agent_dir: home.join(".pi/agent"),
        home,
        backend: Backend::Native,
        adapter_manifest: None,
    };
    std::fs::create_dir_all(state_dir(&context)).unwrap();
    let held = lock(&context).unwrap();
    let inherited = held.file.try_clone().unwrap();
    assert!(
        lock(&context).is_err(),
        "a concurrent writer must still be refused"
    );
    drop(held);
    let next = lock(&context)
        .expect("a forked child's descriptor must not extend the completed operation's lock");
    drop(inherited);
    assert!(
        lock(&context).is_err(),
        "closing the inherited descriptor cannot release the next writer's lock"
    );
    drop(next);
    assert!(lock(&context).is_ok());
}
fn read_saved(context: &Context, id: &str) -> Result<Saved> {
    let path = state_path(context, id)?;
    let bytes = std::fs::read(&path).map_err(|e| Error::io(&path, e))?;
    serde_json::from_slice(&bytes).map_err(|e| Error::json(&path, e))
}

/// Create a retained exact preview. Only private toolbox state is written; configuration
/// is not touched until `apply(id)`. `registered` is an explicit registry/scan selection.
pub fn preview(
    context: &Context,
    repo: &Path,
    registered: &[PathBuf],
    options: Options,
) -> Result<Preview> {
    prepare(context, repo, registered, options, true)
}
pub fn preview_read_only(
    context: &Context,
    repo: &Path,
    registered: &[PathBuf],
    options: Options,
) -> Result<Preview> {
    prepare(context, repo, registered, options, false)
}
fn prepare(
    context: &Context,
    repo: &Path,
    registered: &[PathBuf],
    options: Options,
    retain: bool,
) -> Result<Preview> {
    use std::os::unix::fs::PermissionsExt;
    let mut roots = BTreeSet::new();
    let candidates = if matches!(options.scope, Scope::All | Scope::Machine) {
        registered.to_vec()
    } else {
        vec![repo.into()]
    };
    for candidate in candidates {
        let root = candidate
            .canonicalize()
            .map_err(|e| Error::io(&candidate, e))?;
        roots.insert(root.clone());
        if options.scope != Scope::Project {
            for tree in crate::git::worktrees(&root) {
                if tree.is_usable() {
                    roots.insert(
                        tree.path
                            .canonicalize()
                            .map_err(|e| Error::io(&tree.path, e))?,
                    );
                }
            }
        }
    }
    if roots.is_empty() {
        return Err(failure(
            "No projects selected. Scan/register projects first.",
        ));
    }
    // The home cwd is a real report scope for global defaults; never infer global writes
    // from selecting a project whose server happens to come from a global file.
    let mut reports = Vec::new();
    let mut changes = Vec::new();
    let mut warnings = Vec::new();
    let mut inputs = BTreeMap::new();
    for relative in [
        "settings.json",
        "mcp-cache.json",
        "npm/node_modules/pi-mcp-adapter/package.json",
    ] {
        let path = context.agent_dir.join(relative);
        inputs.insert(path.clone(), digest(&path)?);
    }
    let mut edits = Vec::new();
    let mut targets: Vec<(PathBuf, bool)> = roots.into_iter().map(|p| (p, false)).collect();
    if options.scope == Scope::Machine {
        targets.insert(0, (context.home.clone(), true));
    }
    let mut names_seen = BTreeSet::new();
    let mut globally_changed = BTreeSet::new();
    for (root, global) in targets {
        let resolved = if global {
            context.detected(None)?
        } else {
            context.for_project(&root)?
        };
        if resolved.backend != context.backend {
            return Err(failure(format!("{} selects a different MCP backend. Mixed-backend scopes must be optimised separately; no configuration was changed.", root.display())));
        }
        let context = &resolved;
        if context.backend == Backend::Adapter
            && !context
                .adapter_version()
                .is_some_and(|v| v.major == 5 && v.minor == 3 && v.pre.is_empty())
        {
            return Err(failure("Optimisation requires a verified adapter 5.3.x installation; other config semantics are not certified. Inspect with mcp-report or configure exposure in Pi."));
        }
        let manifest = context.adapter_manifest_path();
        inputs.insert(manifest.clone(), digest(&manifest)?);
        let loaded = if global {
            load_global(context)?
        } else {
            load(context, &root)?
        };
        if !loaded.warnings.is_empty() {
            return Err(failure(loaded.warnings.join("\n")));
        }
        for path in &loaded.files {
            inputs.insert(path.clone(), digest(path)?);
        }
        let report = inspect_loaded(context, &root, &loaded)?;
        let path = if global {
            context
                .agent_dir
                .join(if context.backend == Backend::Adapter {
                    "mcp-adapter.json"
                } else {
                    "mcp.json"
                })
        } else {
            root.join(if context.backend == Backend::Adapter {
                ".pi/mcp-adapter.json"
            } else {
                ".pi/mcp.json"
            })
        };
        no_links(&path)?;
        let before = bytes(&path)?
            .map(|b| String::from_utf8(b).map_err(|_| failure("Configuration must be UTF-8")))
            .transpose()?;
        // Writes are deliberately strict: don't erase comments in someone's config.
        let mut doc: Value = match &before {
            Some(s) => serde_json::from_str(s).map_err(|_| {
                failure(format!(
                    "{} contains comments or invalid JSON; refusing to rewrite it",
                    path.display()
                ))
            })?,
            None => json!({}),
        };
        crate::merge::require_canonical_mcp(&doc, &path)?;
        let mut changed = false;
        for server in &report.servers {
            names_seen.insert(server.name.clone());
            if !global
                && globally_changed.contains(&server.name)
                && !project_source(&server.exposure_source, &root)
            {
                continue;
            }
            if !server.enabled || options.keep_direct.contains(&server.name) {
                continue;
            }
            let chosen = options.keep_tools.get(&server.name);
            if !server.recommend_search && chosen.is_none() {
                continue;
            }
            if let Some(chosen) = chosen {
                if chosen.is_empty() || chosen.iter().any(|name| !server.tool_names.contains(name))
                {
                    return Err(failure(format!(
                        "{}: selected tools must be present in valid cached metadata",
                        server.name
                    )));
                }
            }
            if context.backend == Backend::Adapter
                && !global
                && !server
                    .sources
                    .iter()
                    .any(|source| project_source(source, &root))
            {
                let warning = format!("{}: global-only entries were kept unchanged to preserve trust and credential authority. Use --scope machine for explicit global optimisation.", server.name);
                if !warnings.contains(&warning) {
                    warnings.push(warning);
                }
                continue;
            }
            if doc.get("mcpServers").is_none() {
                doc["mcpServers"] = json!({});
            }
            if !doc["mcpServers"].is_object() {
                return Err(failure("mcpServers must be an object"));
            }
            if doc["mcpServers"].get(&server.name).is_none() {
                doc["mcpServers"][&server.name] = json!({});
            }
            let entry = &mut doc["mcpServers"][&server.name];
            if !entry.is_object() {
                return Err(failure("Server configuration must be an object"));
            }
            if context.backend == Backend::Adapter {
                let default = match context.install_target() {
                    crate::convert::pi::Target::Adapter { search: true } => json!("search"),
                    _ => json!(false),
                };
                let after = chosen.map_or(default, |tools| json!(tools));
                let old = entry
                    .get("directTools")
                    .cloned()
                    .unwrap_or(json!("inherited"));
                entry["directTools"] = after.clone();
                changes.push(Change {
                    path: path.clone(),
                    server: server.name.clone(),
                    before: old,
                    after,
                });
            } else {
                // Defer the catalogue and literal wildcard, retaining deliberate
                // exact/pattern per-tool policies and disabled/hidden entries.
                let old = json!({"exposure": entry.get("exposure"), "toolExposure": entry.get("toolExposure")});
                if loaded.entries[&server.name].value["exposure"] == "direct" {
                    entry["exposure"] = json!("deferred");
                }
                if let Some(exposures) = loaded.entries[&server.name].value.get("toolExposure") {
                    entry["toolExposure"] = exposures.clone();
                }
                if let Some(exposures) =
                    entry.get_mut("toolExposure").and_then(Value::as_object_mut)
                {
                    if exposures.get("*") == Some(&json!("direct")) {
                        exposures.insert("*".into(), json!("deferred"));
                    }
                }
                if let Some(chosen) = chosen {
                    if entry.get("toolExposure").is_none() {
                        entry["toolExposure"] = json!({});
                    }
                    for name in chosen {
                        entry["toolExposure"][name] = json!("direct");
                    }
                }
                changes.push(Change { path: path.clone(), server: server.name.clone(), before: old, after: json!({"exposure": entry["exposure"], "toolExposure": entry.get("toolExposure")}) });
            }
            if global {
                globally_changed.insert(server.name.clone());
            }
            changed = true;
        }
        if changed {
            let mode = std::fs::metadata(&path)
                .map(|m| m.permissions().mode() & 0o777)
                .unwrap_or(0o600);
            edits.push(Edit {
                path,
                before,
                after: format!(
                    "{}\n",
                    serde_json::to_string_pretty(&doc).expect("JSON value")
                ),
                mode,
            });
        }
        reports.push(report);
    }
    for name in options.keep_direct.iter().chain(options.keep_tools.keys()) {
        if !names_seen.contains(name) {
            return Err(failure(format!("Unknown server selection {name}")));
        }
    }
    // Entropy from a private tempfile name plus time; no predictable sequence or data in id.
    let nonce = tempfile::NamedTempFile::new().map_err(|e| Error::io(std::env::temp_dir(), e))?;
    let id = format!(
        "{}{}",
        crate::hash::bytes(nonce.path().as_os_str().as_encoded_bytes()),
        crate::hash::bytes(format!("{:?}", std::time::SystemTime::now()).as_bytes())
    );
    warnings.push("Exposure changes invalidate adapter server approvals. Reapprove in interactive Pi before headless/RPC use; those sessions may block the tools until then. Reload/restart Pi afterwards. Lifecycle, credentials, permission policy and filters are not rewritten.".into());
    let preview = Preview {
        id,
        retained: retain,
        scope: options.scope,
        reports,
        changes,
        warnings,
    };
    if retain {
        save(
            context,
            &Saved {
                preview: preview.clone(),
                edits,
                inputs,
                applied: false,
                rolled_back: false,
            },
        )?;
    }
    Ok(preview)
}

pub fn apply(context: &Context, id: &str) -> Result<Applied> {
    let _lock = lock(context)?;
    let mut saved = read_saved(context, id)?;
    if saved.applied || saved.rolled_back {
        return Err(failure(
            "This preview was already used; create a new preview.",
        ));
    }
    for (path, expected) in &saved.inputs {
        if digest(path)? != *expected {
            return Err(failure(format!(
                "{} changed since preview; nothing applied. Preview again.",
                path.display()
            )));
        }
    }
    for edit in &saved.edits {
        no_links(&edit.path)?;
        if bytes(&edit.path)?.as_deref() != edit.before.as_ref().map(String::as_bytes) {
            return Err(failure(format!(
                "{} changed since preview; nothing applied.",
                edit.path.display()
            )));
        }
    }
    // Persist recovery intent before the first write. Rollback accepts untouched entries
    // too, so even a disk error after a partial apply has a usable recovery record.
    saved.applied = true;
    let backup = save(context, &saved)?;
    for edit in &saved.edits {
        let written = (|| -> Result<()> {
            if bytes(&edit.path)?.as_deref() != edit.before.as_ref().map(String::as_bytes) {
                return Err(failure(format!(
                    "{} changed during apply",
                    edit.path.display()
                )));
            }
            atomic(&edit.path, edit.after.as_bytes(), edit.mode)
        })();
        if let Err(error) = written {
            return Err(failure(format!("{error}. Apply may be partial. Private backup: {}. Recover with: ai-toolbox mcp-optimise rollback {id}", backup.display())));
        }
    }
    Ok(Applied {
        id: id.into(),
        files: saved.edits.len(),
        backup,
    })
}

pub fn rollback(context: &Context, id: &str) -> Result<Applied> {
    let _lock = lock(context)?;
    let mut saved = read_saved(context, id)?;
    if !saved.applied || saved.rolled_back {
        return Err(failure("This preview has no applied changes to roll back."));
    }
    for edit in &saved.edits {
        no_links(&edit.path)?;
        let current = bytes(&edit.path)?;
        if current.as_deref() != Some(edit.after.as_bytes())
            && current.as_deref() != edit.before.as_ref().map(String::as_bytes)
        {
            return Err(failure(format!(
                "{} changed after optimisation; nothing rolled back.",
                edit.path.display()
            )));
        }
    }
    for edit in &saved.edits {
        match &edit.before {
            Some(before) => atomic(&edit.path, before.as_bytes(), edit.mode)?,
            None => {
                if edit.path.exists() {
                    std::fs::remove_file(&edit.path).map_err(|e| Error::io(&edit.path, e))?;
                }
            }
        }
    }
    saved.rolled_back = true;
    let backup = save(context, &saved)?;
    Ok(Applied {
        id: id.into(),
        files: saved.edits.len(),
        backup,
    })
}
