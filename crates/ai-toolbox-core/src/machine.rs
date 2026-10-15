//! The once-per-machine facts, which are not properties of any repo.
//!
//! Two things live at this level: whether each harness has the always-on charter in its
//! global config, and whether Pi can speak MCP at all. Both are reported per repo in the
//! CLI's status output, which makes them easy to mistake for repo state - they are not,
//! and fixing them in one repo fixes them everywhere.

use std::path::{Path, PathBuf};

use crate::harness::{self, Harness};

/// The marker `ai-toolbox base-charter` wraps its block in, so appending twice is a
/// no-op and removing it by hand is detectable.
pub const CHARTER_MARKER: &str = "<!-- ai-toolbox:base-charter -->";

#[derive(Debug, Clone, serde::Serialize)]
pub struct Machine {
    pub home: PathBuf,
    pub pi_agent_dir: PathBuf,
    pub charters: Vec<Charter>,
    pub pi_mcp: PiMcp,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Charter {
    pub harness: Harness,
    pub path: PathBuf,
    pub installed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PiMcp {
    /// The package is installed and registered - Pi can load MCP servers.
    Ready,
    /// Pi 0.99+ can use MCP without the adapter.
    Native,
    /// Pi is on the machine but the adapter is not set up. `ai-toolbox pi-init` fixes it.
    Missing,
    /// No Pi here, so the question does not arise.
    NoPi,
}

impl Machine {
    pub fn read() -> Machine {
        let home = home();
        let pi_agent_dir = pi_agent_dir(&home);
        Machine {
            charters: harness::ALL
                .iter()
                .map(|&h| {
                    let path = h.charter_path(&home, &pi_agent_dir);
                    Charter {
                        harness: h,
                        installed: has_charter(&path),
                        path,
                    }
                })
                .collect(),
            pi_mcp: pi_mcp(&home, &pi_agent_dir, native_pi_version()),
            home,
            pi_agent_dir,
        }
    }

    pub fn charter(&self, harness: Harness) -> Option<&Charter> {
        self.charters.iter().find(|c| c.harness == harness)
    }
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Pi keeps its agent config here, overridable for anyone running more than one.
pub fn pi_agent_dir(home: &Path) -> PathBuf {
    std::env::var_os("PI_CODING_AGENT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".pi/agent"))
}

fn has_charter(path: &Path) -> bool {
    std::fs::read_to_string(path).is_ok_and(|text| text.contains(CHARTER_MARKER))
}

/// Both halves have to hold: the package present on disk *and* registered in Pi's
/// settings. Either alone is a half-install that fails at connect time with a message
/// that does not mention either.
fn pi_mcp(home: &Path, agent_dir: &Path, native: Option<semver::Version>) -> PiMcp {
    if native.is_none() && !home.join(".pi").is_dir() && !agent_dir.is_dir() {
        return PiMcp::NoPi;
    }
    let context = crate::mcp::Context {
        home: home.into(),
        agent_dir: agent_dir.into(),
        backend: crate::mcp::Backend::Native,
        adapter_manifest: None,
    };
    match context.detected(None) {
        Ok(context) if context.backend == crate::mcp::Backend::Adapter => PiMcp::Ready,
        Ok(_) if native.is_some_and(|v| v >= semver::Version::new(0, 99, 0)) => PiMcp::Native,
        _ => PiMcp::Missing,
    }
}

/// Inspect an installed npm package through PATH, without executing Pi or loading extensions.
pub(crate) fn native_pi_version() -> Option<semver::Version> {
    let path = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&path) {
        let Ok(binary) = directory.join("pi").canonicalize() else {
            continue;
        };
        for ancestor in binary.ancestors().skip(1).take(3) {
            let Ok(text) = std::fs::read_to_string(ancestor.join("package.json")) else {
                continue;
            };
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
                continue;
            };
            if value["name"] == "@earendil-works/pi-coding-agent"
                || value["name"] == "@mariozechner/pi-coding-agent"
            {
                return value["version"]
                    .as_str()
                    .and_then(|v| semver::Version::parse(v).ok());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths;

    #[test]
    fn a_charter_is_found_by_its_marker_not_by_the_file_existing() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join(".claude/CLAUDE.md");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();

        std::fs::write(&path, "# my own rules\n").unwrap();
        assert!(
            !has_charter(&path),
            "a file without the marker is not a charter"
        );

        std::fs::write(&path, format!("# my own rules\n{CHARTER_MARKER}\nbody\n")).unwrap();
        assert!(has_charter(&path));
    }

    #[test]
    fn pi_mcp_needs_the_package_and_the_registration() {
        let home = tempfile::tempdir().unwrap();
        let agent = home.path().join(".pi/agent");
        std::fs::create_dir_all(&agent).unwrap();

        assert_eq!(pi_mcp(home.path(), &agent, None), PiMcp::Missing);

        std::fs::create_dir_all(agent.join("npm/node_modules").join(paths::PI_MCP_PACKAGE))
            .unwrap();
        assert_eq!(
            pi_mcp(home.path(), &agent, None),
            PiMcp::Missing,
            "on disk but unregistered is still a half-install"
        );

        std::fs::write(
            agent.join("settings.json"),
            format!(r#"{{"packages":["npm:{}"]}}"#, paths::PI_MCP_PACKAGE),
        )
        .unwrap();
        std::fs::write(
            agent.join("npm/node_modules/pi-mcp-adapter/package.json"),
            r#"{"name":"pi-mcp-adapter","version":"5.3.0"}"#,
        )
        .unwrap();
        assert_eq!(pi_mcp(home.path(), &agent, None), PiMcp::Ready);
    }

    #[test]
    fn no_pi_on_the_machine_is_not_a_missing_install() {
        let home = tempfile::tempdir().unwrap();
        assert_eq!(
            pi_mcp(home.path(), &home.path().join(".pi/agent"), None),
            PiMcp::NoPi
        );
    }
}
