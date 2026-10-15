//! One read-only backend detector for CLI, UI and machine status. It resolves standard
//! registrations without loading extension code. Ambiguous registrations fail closed.
use super::config::{document, Backend, Context};
use crate::{Error, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub(super) struct Detection {
    pub backend: Backend,
    pub manifest: Option<PathBuf>,
}
pub(super) fn detect(context: &Context, repo: Option<&Path>) -> Result<Detection> {
    let mut manifests = Vec::new();
    let mut disabled_package = false;
    let mut native_disabled = false;
    for directory in std::iter::once(context.agent_dir.clone()).chain(repo.map(|p| p.join(".pi"))) {
        let path = directory.join("settings.json");
        let settings = document(&path)?;
        if !settings.is_object()
            || ["packages", "extensions"]
                .iter()
                .any(|key| settings.get(key).is_some_and(|v| !v.is_array()))
        {
            return Err(ambiguous(
                "Invalid Pi settings shape; backend detection is unavailable",
            ));
        }
        for extension in settings["extensions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if extension == "-builtin:mcp" {
                native_disabled = true;
            }
            if extension.contains("pi-mcp-adapter") {
                return Err(ambiguous("Direct/filtered adapter extension registration needs manual verification; use a standard npm package registration before optimising"));
            }
        }
        for package in settings["packages"].as_array().into_iter().flatten() {
            let Some(source) = package.as_str().or_else(|| package["source"].as_str()) else {
                continue;
            };
            let is_adapter = source
                .strip_prefix("npm:")
                .is_some_and(|s| s.split('@').next() == Some("pi-mcp-adapter"));
            if !is_adapter {
                if source.contains("pi-mcp-adapter") {
                    return Err(ambiguous(
                        "Nonstandard adapter package source needs manual backend verification",
                    ));
                }
                continue;
            }
            if let Some(extensions) = package.get("extensions") {
                if extensions.as_array().is_some_and(Vec::is_empty) {
                    disabled_package = true;
                    continue;
                }
                return Err(ambiguous("Adapter package extension filters make the backend ambiguous; use an unfiltered registration or disable it with extensions: []"));
            }
            let manifest = directory.join("npm/node_modules/pi-mcp-adapter/package.json");
            if !manifest.is_file() {
                return Err(ambiguous("The adapter is registered but not installed; repair its installation before configuring MCP"));
            }
            let package = document(&manifest)?;
            if package.get("name").is_some_and(|v| v != "pi-mcp-adapter") {
                return Err(ambiguous(
                    "The installed adapter manifest names a different package",
                ));
            }
            let manifest = manifest
                .canonicalize()
                .map_err(|e| Error::io(&manifest, e))?;
            if !manifests.contains(&manifest) {
                manifests.push(manifest);
            }
        }
    }
    if manifests.len() > 1 || (disabled_package && !manifests.is_empty()) {
        return Err(ambiguous("Conflicting global/project adapter registrations; choose one active adapter before configuring MCP"));
    }
    if let Some(manifest) = manifests.pop() {
        return Ok(Detection {
            backend: Backend::Adapter,
            manifest: Some(manifest),
        });
    }
    if native_disabled {
        return Err(ambiguous(
            "Built-in MCP is disabled and no active adapter is registered",
        ));
    }
    Ok(Detection {
        backend: Backend::Native,
        manifest: None,
    })
}
fn ambiguous(message: &str) -> Error {
    Error::Catalogue(message.into())
}
