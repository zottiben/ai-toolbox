//! Self-update for both shells. Checks are read-only; installation is explicit, pinned
//! to the release that was shown, and uses the same installer as a first-time install.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use semver::Version;
use serde::{Deserialize, Serialize};

const REPOSITORY: &str = "https://github.com/zottiben/ai-toolbox";
const LATEST: &str = "https://api.github.com/repos/zottiben/ai-toolbox/releases/latest";
const INSTALLER: &str = include_str!("../../../install/install.sh");
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub current: String,
    pub latest: String,
    pub tag: String,
    pub release_url: String,
    pub available: bool,
    pub installation: Installation,
}

#[derive(Debug, Clone, Serialize)]
pub struct Installation {
    pub binary: Option<PathBuf>,
    pub app: Option<PathBuf>,
    pub catalogue: Option<PathBuf>,
    /// Some platforms (e.g. a Linux .deb) must go through their package manager.
    pub blocked: Option<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Updated {
    pub version: String,
    pub restart_required: bool,
    pub output: String,
}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// Deliberately no catalogue load, repo discovery or database creation here. An update
/// must still work when the catalogue has gone missing or the working directory is not
/// a repo.
pub fn check() -> Result<Status> {
    let output = Command::new("curl")
        .args([
            "-fsSL",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--connect-timeout",
            "10",
            "--max-time",
            "30",
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "User-Agent: ai-toolbox",
            LATEST,
        ])
        .stdin(Stdio::null())
        .output()
        .context("checking releases requires curl")?;
    if !output.status.success() {
        bail!(
            "could not check GitHub releases: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let exe = std::env::current_exe().context("locating the running executable")?;
    let home = crate::machine::home();
    let root = crate::root::find().ok();
    status(
        &output.stdout,
        VERSION,
        installation(&exe, &home, root.as_deref(), std::env::consts::OS),
    )
}

fn status(json: &[u8], current: &str, installation: Installation) -> Result<Status> {
    let release: Release =
        serde_json::from_slice(json).context("invalid GitHub release response")?;
    let latest = Version::parse(release.tag_name.trim_start_matches('v'))
        .context("the release tag is not a semantic version")?;
    // Tags also become URL path segments and installer arguments. Accept only the
    // release workflow's canonical spelling, never a path or an arbitrary ref.
    if release.tag_name != format!("v{latest}")
        || release.draft
        || release.prerelease
        || !latest.pre.is_empty()
    {
        bail!("the latest release is not a stable v<version> release");
    }
    let running = Version::parse(current).context("invalid running version")?;
    Ok(Status {
        current: current.to_string(),
        latest: latest.to_string(),
        release_url: format!("{REPOSITORY}/releases/tag/{}", release.tag_name),
        tag: release.tag_name,
        // Build metadata is not precedence. A newer dev build must never downgrade.
        available: latest.cmp_precedence(&running).is_gt(),
        installation,
    })
}

fn installation(exe: &Path, home: &Path, root: Option<&Path>, os: &str) -> Installation {
    let clone = home.join(".ai-toolbox/clone");
    let mut result = Installation {
        binary: None,
        app: None,
        catalogue: None,
        blocked: None,
        notes: vec![],
    };
    if os != "macos" && os != "linux" {
        result.blocked = Some("No prebuilt release for this operating system.".into());
        return result;
    }
    if exe.file_name().is_some_and(|n| n == "ai-toolbox") {
        // current_exe is the resolved binary, not the PATH symlink or checkout shim.
        result.binary = Some(exe.to_path_buf());
        let app = PathBuf::from("/Applications/ai-toolbox.app");
        if os == "macos" && app.is_dir() {
            result.app = Some(app);
        }
    } else if os == "macos" {
        result.app = exe
            .ancestors()
            .find(|p| p.file_name().is_some_and(|n| n == "ai-toolbox.app"))
            .map(Path::to_path_buf);
        if result.app.is_some() {
            // A Finder-launched app may have a minimal PATH. Prefer an actual CLI on
            // PATH, then the installer's usual user-writable destination. Never replace
            // a source shim: it is somebody's tracked file.
            result.binary = cli_on_path()
                .or_else(|| {
                    root.and_then(|root| {
                        [
                            root.join("target/release/ai-toolbox"),
                            root.join("target/debug/ai-toolbox"),
                        ]
                        .into_iter()
                        .find(|p| is_binary(p))
                    })
                })
                .or_else(|| {
                    [
                        home.join(".local/bin/ai-toolbox"),
                        home.join(".cargo/bin/ai-toolbox"),
                        PathBuf::from("/usr/local/bin/ai-toolbox"),
                    ]
                    .into_iter()
                    .find(|p| is_binary(p))
                })
                .or_else(|| Some(home.join(".local/bin/ai-toolbox")));
            if result
                .binary
                .as_ref()
                .is_some_and(|p| p.exists() && !is_binary(p))
            {
                result.blocked = Some("The CLI destination is a script. Run ai-toolbox update from a terminal to preserve your checkout shim.".into());
            }
        }
    }
    if result.binary.is_none() {
        result.blocked = Some("This desktop build cannot replace itself. Update an AppImage/.deb through its download or package manager; for a source build, rebuild with cargo.".into());
    }
    if root.is_none_or(|p| same_path(p, &clone)) {
        result.catalogue = Some(clone);
    } else {
        result.notes.push("Your working catalogue checkout is left untouched; only the compiled application is replaced. Pull catalogue changes yourself when ready.".into());
    }
    result
        .notes
        .push("Installed hooks, skills and project configuration are not changed.".into());
    result
}

fn same_path(a: &Path, b: &Path) -> bool {
    a == b
        || a.canonicalize()
            .ok()
            .zip(b.canonicalize().ok())
            .is_some_and(|(a, b)| a == b)
}

fn cli_on_path() -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|p| p.join("ai-toolbox"))
        .find(|p| is_binary(p))
        .and_then(|p| p.canonicalize().ok())
}

fn is_binary(path: &Path) -> bool {
    use std::io::Read;
    let mut prefix = [0; 2];
    path.is_file()
        && File::open(path)
            .and_then(|mut f| f.read_exact(&mut prefix))
            .is_ok()
        && prefix != *b"#!"
}

/// The caller supplies the tag it showed the user, not a URL, script or destination.
/// Rechecking under a cross-process lock prevents a stale preview installing a different
/// release. The UI additionally remembers completion until the process is restarted.
pub fn apply(expected_tag: &str) -> Result<Updated> {
    let dir = crate::machine::home().join(".ai-toolbox");
    fs::create_dir_all(&dir).context("creating the update lock directory")?;
    let lock = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join("update.lock"))
        .context("opening the update lock")?;
    lock.try_lock()
        .context("another ai-toolbox update is running; try again when it finishes")?;
    let status = check()?;
    if status.tag != expected_tag {
        bail!("the latest release changed; check for updates again before installing");
    }
    if !status.available {
        bail!("there is no newer release to install");
    }
    if let Some(reason) = &status.installation.blocked {
        bail!("{reason}");
    }
    let binary = status
        .installation
        .binary
        .as_ref()
        .context("no CLI destination")?;
    // A browser or second process may still be running the old version after another
    // updater has replaced its executable. Do not reinstall or downgrade that file.
    if binary.is_file() && is_binary(binary) {
        let output = Command::new(binary)
            .arg("--version")
            .output()
            .context("reading the installed version")?;
        if output.status.success() {
            if let Some(version) = String::from_utf8_lossy(&output.stdout)
                .trim()
                .strip_prefix("ai-toolbox ")
            {
                let order =
                    Version::parse(version)?.cmp_precedence(&Version::parse(&status.latest)?);
                if order.is_gt() {
                    bail!("the installed CLI is newer ({version}); refusing to downgrade it");
                }
                if order.is_eq() && status.installation.app.is_none() {
                    return Ok(Updated {
                        version: version.into(),
                        restart_required: true,
                        output:
                            "This release is already installed. Restart the running application."
                                .into(),
                    });
                }
                // A newer CLI does not prove that the separately installed app was
                // updated. When both are targets, let the installer replace both.
            }
        }
    }
    let mut script = tempfile::NamedTempFile::new().context("staging the installer")?;
    script.write_all(INSTALLER.as_bytes())?;
    let mut command = Command::new("sh");
    command
        .arg(script.path())
        .args(["--version", &status.tag, "--non-interactive", "--bin-dir"])
        .arg(binary.parent().context("no binary directory")?);
    if let Some(app) = &status.installation.app {
        command
            .arg("--app-dir")
            .arg(app.parent().context("no app directory")?);
    } else {
        command.arg("--no-app");
    }
    if status.installation.catalogue.is_none() {
        command.arg("--no-catalogue");
    }
    let output = command
        .stdin(Stdio::null())
        .output()
        .context("running the release installer")?;
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !output.status.success() {
        bail!("update failed ({}):\n{log}", output.status);
    }
    Ok(Updated {
        version: status.latest,
        restart_required: true,
        output: log,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, current: &str) -> Result<Status> {
        status(
            serde_json::json!({"tag_name": tag}).to_string().as_bytes(),
            current,
            installation(
                Path::new("/tmp/bin/ai-toolbox"),
                Path::new("/tmp/home"),
                None,
                "linux",
            ),
        )
    }

    #[test]
    fn versions_are_compared_by_precedence_not_text_or_build_metadata() {
        assert!(release("v0.10.0", "0.9.0").unwrap().available);
        assert!(!release("v0.1.0", "0.1.0").unwrap().available);
        assert!(!release("v0.1.0", "0.2.0").unwrap().available);
        assert!(release("v0.2.0", "0.2.0-rc.1").unwrap().available);
        assert!(!release("v0.1.0+release", "0.1.0+local").unwrap().available);
    }

    #[test]
    fn invalid_and_nonstable_releases_are_not_installable() {
        for tag in ["main", "v../oops", "1.0.0", "vv1.0.0", "v1.0.0-rc.1"] {
            assert!(release(tag, "0.1.0").is_err(), "{tag}");
        }
        let install = release("v0.2.0", "0.1.0").unwrap().installation;
        assert!(status(b"not json", "0.1.0", install.clone()).is_err());
        for flag in ["draft", "prerelease"] {
            let json = serde_json::json!({"tag_name": "v1.0.0", flag: true});
            assert!(status(json.to_string().as_bytes(), "0.1.0", install.clone()).is_err());
        }
    }

    #[test]
    fn source_binary_is_replaced_in_place_without_pulling_the_working_tree() {
        let exe = Path::new("/src/ai-toolbox/target/release/ai-toolbox");
        let install = installation(
            exe,
            Path::new("/home/me"),
            Some(Path::new("/src/ai-toolbox")),
            "linux",
        );
        assert_eq!(install.binary.as_deref(), Some(exe));
        assert!(install.catalogue.is_none());
        assert!(install.blocked.is_none());
    }

    #[test]
    fn release_install_refreshes_only_the_managed_clone() {
        let install = installation(
            Path::new("/home/me/.local/bin/ai-toolbox"),
            Path::new("/home/me"),
            Some(Path::new("/home/me/.ai-toolbox/clone")),
            "linux",
        );
        assert_eq!(
            install.catalogue.unwrap(),
            Path::new("/home/me/.ai-toolbox/clone")
        );
    }

    #[test]
    fn linux_desktop_and_unbundled_desktop_builds_are_not_mistaken_for_the_cli() {
        for os in ["linux", "macos", "windows"] {
            assert!(installation(
                Path::new("/tmp/ai-toolbox-desktop"),
                Path::new("/home/me"),
                None,
                os
            )
            .blocked
            .is_some());
        }
    }
}
