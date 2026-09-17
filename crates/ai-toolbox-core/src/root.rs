//! Finding the ai-toolbox clone.
//!
//! The clone is not a detail of the install - it *is* the catalogue (D2). `install.sh`
//! symlinks the command out of it so that `git pull` updates the hooks and skills with
//! no reinstall, which means resolving the root has to follow symlinks the same way
//! `bin/ai-toolbox` does, or the binary would read a catalogue from wherever the symlink
//! happened to live.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// The directories that make a path a toolbox clone rather than any other folder.
const MARKERS: [&str; 3] = ["hooks", "skills", "mcp/presets"];

/// Where `install.sh` puts the catalogue for a downloaded binary (D11).
pub const INSTALLED_CLONE: &str = ".ai-toolbox/clone";

/// The catalogue this binary should read, in the order the candidates should win.
///
/// 1. `$AI_TOOLBOX`, so a developer can point a released binary at a working clone.
/// 2. The clone containing the binary, so running out of a checkout behaves exactly as
///    it always has.
/// 3. `~/.ai-toolbox/clone`, which is what makes a `curl | sh` install work: a binary
///    downloaded from a release has no repo beside it, and the catalogue is read from
///    disk on purpose (D2).
pub fn find() -> Result<PathBuf> {
    if let Some(from_env) = std::env::var_os("AI_TOOLBOX") {
        let path = PathBuf::from(from_env);
        return verify(&path).ok_or_else(|| {
            Error::Catalogue(format!(
                "AI_TOOLBOX points at {}, which is not an ai-toolbox clone (no hooks/, skills/, mcp/presets/)",
                path.display()
            ))
        });
    }

    let exe = std::env::current_exe().map_err(|e| Error::io("the running binary", e))?;
    if let Some(root) = from_binary(&exe) {
        return Ok(root);
    }
    let installed = crate::machine::home().join(INSTALLED_CLONE);
    if let Some(root) = verify(&installed) {
        return Ok(root);
    }

    Err(Error::Catalogue(format!(
        "no ai-toolbox catalogue found - looked beside {} and in {}. \
         Reinstall with `curl -fsSL https://zottiben.github.io/ai-toolbox/install.sh | sh`, \
         or set AI_TOOLBOX to a clone.",
        exe.display(),
        installed.display()
    )))
}

/// Walk up from a binary looking for the clone.
///
/// Three layouts have to work, and they are all real: the installed symlink
/// (`~/.local/bin/ai-toolbox` -> `<clone>/bin/ai-toolbox`), a cargo build
/// (`<clone>/target/debug/ai-toolbox`), and running the binary in place. Ascending
/// until a marker matches covers all three without special-casing any of them.
pub fn from_binary(exe: &Path) -> Option<PathBuf> {
    let resolved = std::fs::canonicalize(exe).unwrap_or_else(|_| exe.to_path_buf());
    let mut dir = resolved.parent();
    while let Some(candidate) = dir {
        if let Some(root) = verify(candidate) {
            return Some(root);
        }
        dir = candidate.parent();
    }
    None
}

/// A path is a clone when it holds all three markers. Canonicalised on the way out so
/// that two paths to the same clone never look like two catalogues.
fn verify(path: &Path) -> Option<PathBuf> {
    MARKERS
        .iter()
        .all(|marker| path.join(marker).is_dir())
        .then(|| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clone_at(root: &Path) {
        for marker in MARKERS {
            std::fs::create_dir_all(root.join(marker)).unwrap();
        }
    }

    #[test]
    fn finds_the_clone_from_a_binary_nested_inside_it() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("ai-toolbox");
        clone_at(&root);
        std::fs::create_dir_all(root.join("target/debug")).unwrap();
        let exe = root.join("target/debug/ai-toolbox");
        std::fs::write(&exe, "").unwrap();

        assert_eq!(
            from_binary(&exe).unwrap(),
            std::fs::canonicalize(&root).unwrap()
        );
    }

    #[test]
    fn follows_the_path_symlink_install_sh_creates() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("ai-toolbox");
        clone_at(&root);
        std::fs::create_dir_all(root.join("bin")).unwrap();
        let real = root.join("bin/ai-toolbox");
        std::fs::write(&real, "").unwrap();

        let bin_dir = temp.path().join("local-bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let link = bin_dir.join("ai-toolbox");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        // Without following the link this would search ~/.local/bin and find nothing.
        assert_eq!(
            from_binary(&link).unwrap(),
            std::fs::canonicalize(&root).unwrap()
        );
    }

    #[test]
    fn a_binary_with_no_clone_beside_it_falls_back_to_the_installed_one() {
        // The curl-install case: ~/.local/bin/ai-toolbox, catalogue at ~/.ai-toolbox/clone.
        let home = tempfile::tempdir().unwrap();
        let clone = home.path().join(INSTALLED_CLONE);
        clone_at(&clone);
        let bin = home.path().join(".local/bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("ai-toolbox"), "").unwrap();

        let found = crate::testing::with_home(home.path(), || {
            verify(&crate::machine::home().join(INSTALLED_CLONE))
        });
        assert_eq!(found.unwrap(), std::fs::canonicalize(&clone).unwrap());
    }

    #[test]
    fn a_directory_missing_a_marker_is_not_a_clone() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("hooks")).unwrap();
        std::fs::create_dir_all(temp.path().join("skills")).unwrap();
        let exe = temp.path().join("ai-toolbox");
        std::fs::write(&exe, "").unwrap();

        assert!(from_binary(&exe).is_none());
    }
}
