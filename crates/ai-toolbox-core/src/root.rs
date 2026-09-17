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

/// `$AI_TOOLBOX` if set, else the clone that contains this binary.
///
/// The env var wins so a developer can point a released binary at a working clone
/// without reinstalling - the same escape hatch the bash has.
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
    from_binary(&exe).ok_or_else(|| {
        Error::Catalogue(format!(
            "could not find the ai-toolbox clone from {} - set AI_TOOLBOX to its path",
            exe.display()
        ))
    })
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
    fn a_directory_missing_a_marker_is_not_a_clone() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("hooks")).unwrap();
        std::fs::create_dir_all(temp.path().join("skills")).unwrap();
        let exe = temp.path().join("ai-toolbox");
        std::fs::write(&exe, "").unwrap();

        assert!(from_binary(&exe).is_none());
    }
}
