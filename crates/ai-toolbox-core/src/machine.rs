//! The once-per-machine facts, which are not properties of any repo.
//!
//! Two things live at this level: whether each harness has the always-on charter in its
//! global config, and whether Pi can speak MCP at all. Both are reported per repo in the
//! CLI's status output, which makes them easy to mistake for repo state - they are not,
//! and fixing them in one repo fixes them everywhere.

use std::path::{Path, PathBuf};

use crate::harness::{self, Harness};
use crate::paths;

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
            pi_mcp: pi_mcp(&home, &pi_agent_dir),
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
fn pi_mcp(home: &Path, agent_dir: &Path) -> PiMcp {
    if !home.join(".pi").is_dir() && !agent_dir.is_dir() {
        return PiMcp::NoPi;
    }
    let installed = agent_dir
        .join("npm/node_modules")
        .join(paths::PI_MCP_PACKAGE)
        .is_dir();
    let registered = std::fs::read_to_string(agent_dir.join("settings.json"))
        .is_ok_and(|text| text.contains(paths::PI_MCP_PACKAGE));
    if installed && registered {
        PiMcp::Ready
    } else {
        PiMcp::Missing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        assert_eq!(pi_mcp(home.path(), &agent), PiMcp::Missing);

        std::fs::create_dir_all(agent.join("npm/node_modules").join(paths::PI_MCP_PACKAGE))
            .unwrap();
        assert_eq!(
            pi_mcp(home.path(), &agent),
            PiMcp::Missing,
            "on disk but unregistered is still a half-install"
        );

        std::fs::write(
            agent.join("settings.json"),
            format!(r#"{{"extensions":["{}"]}}"#, paths::PI_MCP_PACKAGE),
        )
        .unwrap();
        assert_eq!(pi_mcp(home.path(), &agent), PiMcp::Ready);
    }

    #[test]
    fn no_pi_on_the_machine_is_not_a_missing_install() {
        let home = tempfile::tempdir().unwrap();
        assert_eq!(
            pi_mcp(home.path(), &home.path().join(".pi/agent")),
            PiMcp::NoPi
        );
    }
}
