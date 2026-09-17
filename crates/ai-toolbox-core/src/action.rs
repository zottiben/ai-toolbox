//! Every write, described before it happens (D3).
//!
//! Nothing in this crate writes as a side effect of being asked a question. A command
//! plans - producing a list of these - and something else applies them. That is what
//! lets a button say "Repair (4 changes)" and list them first, gives every command a
//! `--dry-run` for free, and makes idempotence a thing a test can check rather than a
//! thing a comment claims.
//!
//! The merged content of a JSON or TOML file is computed at plan time, not at apply
//! time, so what the preview shows is the bytes that will land. The cost of that is a
//! plan can go stale, which `expect` handles.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::hash::{self, Hash};

#[derive(Debug, Clone, serde::Serialize)]
pub struct Action {
    pub path: PathBuf,
    pub summary: String,
    pub kind: Kind,
    /// True when the target already holds exactly this. Kept in the list rather than
    /// filtered out, because "already installed" is an answer worth showing.
    pub noop: bool,
    /// What the target looked like when this was planned - `None` for "did not exist".
    /// Applying checks it, so a plan left open in a window while an agent edits the same
    /// file is refused instead of silently reverting the agent's work.
    pub expect: Option<Hash>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Kind {
    Write {
        #[serde(skip)]
        contents: Vec<u8>,
        /// Unix mode for a fresh file. Hook scripts need the executable bit or they do
        /// not run, and a git clone does not always carry it.
        mode: Option<u32>,
    },
    /// A whole directory, replacing whatever is there. Skills are folders, and a skill
    /// that half-updates is worse than one that does not.
    CopyTree {
        from: PathBuf,
    },
    Symlink {
        target: String,
    },
    Remove,
}

impl Action {
    pub fn write(
        path: impl Into<PathBuf>,
        contents: impl Into<Vec<u8>>,
        summary: impl Into<String>,
    ) -> Result<Action> {
        Action::write_with_mode(path, contents, summary, None)
    }

    /// A file that has to be executable, like a hook script or an MCP launcher.
    pub fn write_executable(
        path: impl Into<PathBuf>,
        contents: impl Into<Vec<u8>>,
        summary: impl Into<String>,
    ) -> Result<Action> {
        Action::write_with_mode(path, contents, summary, Some(0o755))
    }

    fn write_with_mode(
        path: impl Into<PathBuf>,
        contents: impl Into<Vec<u8>>,
        summary: impl Into<String>,
        mode: Option<u32>,
    ) -> Result<Action> {
        let path = path.into();
        let contents = contents.into();
        let current = current_hash(&path)?;
        // Content equal *and* the mode already right. A hook whose bytes match but which
        // cannot execute is not installed, whatever the bytes say.
        let noop = current.as_ref() == Some(&hash::bytes(&contents))
            && mode.is_none_or(|_| is_executable(&path).unwrap_or(false));
        Ok(Action {
            summary: summary.into(),
            kind: Kind::Write { contents, mode },
            noop,
            expect: current,
            path,
        })
    }

    pub fn copy_tree(
        from: impl Into<PathBuf>,
        to: impl Into<PathBuf>,
        summary: impl Into<String>,
    ) -> Result<Action> {
        let from = from.into();
        let to = to.into();
        let noop = to.is_dir() && hash::dir(&to)? == hash::dir(&from)?;
        Ok(Action {
            noop,
            // A directory has no single "previous contents" to compare on apply, and
            // replacing a skill folder wholesale is the intended behaviour anyway.
            expect: None,
            summary: summary.into(),
            kind: Kind::CopyTree { from },
            path: to,
        })
    }

    pub fn symlink(
        path: impl Into<PathBuf>,
        target: impl Into<String>,
        summary: impl Into<String>,
    ) -> Result<Action> {
        let path = path.into();
        let target = target.into();
        let noop =
            std::fs::read_link(&path).is_ok_and(|current| current.to_string_lossy() == target);
        Ok(Action {
            noop,
            expect: None,
            summary: summary.into(),
            kind: Kind::Symlink { target },
            path,
        })
    }

    pub fn remove(path: impl Into<PathBuf>, summary: impl Into<String>) -> Result<Action> {
        let path = path.into();
        Ok(Action {
            noop: !exists(&path),
            expect: current_hash(&path)?,
            summary: summary.into(),
            kind: Kind::Remove,
            path,
        })
    }

    /// The line a preview prints. Reads the same in a terminal and in the board.
    pub fn line(&self) -> String {
        if self.noop {
            format!("{} (already in place)", self.summary)
        } else {
            self.summary.clone()
        }
    }
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Outcome {
    pub applied: usize,
    pub skipped: usize,
    /// Paths that had changed since the plan was made. Nothing was written for these.
    pub stale: Vec<PathBuf>,
}

impl Outcome {
    pub fn changed(&self) -> bool {
        self.applied > 0
    }
}

/// Perform a plan. No-ops are skipped, and anything whose target moved since planning is
/// reported rather than overwritten.
pub fn apply(actions: &[Action]) -> Result<Outcome> {
    let mut outcome = Outcome::default();
    for action in actions {
        if action.noop {
            outcome.skipped += 1;
            continue;
        }
        if current_hash(&action.path)? != action.expect {
            outcome.stale.push(action.path.clone());
            continue;
        }
        perform(action)?;
        outcome.applied += 1;
    }
    Ok(outcome)
}

fn perform(action: &Action) -> Result<()> {
    match &action.kind {
        Kind::Write { contents, mode } => {
            parents(&action.path)?;
            std::fs::write(&action.path, contents).map_err(|e| Error::io(&action.path, e))?;
            if let Some(mode) = mode {
                set_mode(&action.path, *mode)?;
            }
        }
        Kind::CopyTree { from } => {
            // Removed first so a file deleted upstream does not survive in the copy -
            // a skill is the whole folder, not a union of two versions.
            if action.path.exists() {
                std::fs::remove_dir_all(&action.path).map_err(|e| Error::io(&action.path, e))?;
            }
            parents(&action.path)?;
            copy_tree(from, &action.path)?;
        }
        Kind::Symlink { target } => {
            parents(&action.path)?;
            // An existing link has to go before a new one can take its place; an empty
            // directory left by an older layout is fine to replace, a full one is not.
            match std::fs::symlink_metadata(&action.path) {
                Ok(meta) if meta.file_type().is_symlink() => {
                    std::fs::remove_file(&action.path).map_err(|e| Error::io(&action.path, e))?;
                }
                Ok(meta) if meta.is_dir() => {
                    std::fs::remove_dir(&action.path).map_err(|e| {
                        Error::io(
                            &action.path,
                            std::io::Error::new(
                                e.kind(),
                                format!("{e} - it has content; run 'ai-toolbox migrate' first"),
                            ),
                        )
                    })?;
                }
                _ => {}
            }
            std::os::unix::fs::symlink(target, &action.path)
                .map_err(|e| Error::io(&action.path, e))?;
        }
        Kind::Remove => {
            if action.path.is_dir() {
                std::fs::remove_dir_all(&action.path).map_err(|e| Error::io(&action.path, e))?;
            } else {
                std::fs::remove_file(&action.path).map_err(|e| Error::io(&action.path, e))?;
            }
        }
    }
    Ok(())
}

fn parents(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    Ok(())
}

pub(crate) fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to).map_err(|e| Error::io(to, e))?;
    let read = std::fs::read_dir(from).map_err(|e| Error::io(from, e))?;
    for entry in read {
        let entry = entry.map_err(|e| Error::io(from, e))?;
        let source = entry.path();
        let target = to.join(entry.file_name());
        if source.is_dir() {
            copy_tree(&source, &target)?;
            continue;
        }
        std::fs::copy(&source, &target).map_err(|e| Error::io(&source, e))?;
        // Preserve the executable bit: a skill can ship a script, and one that arrives
        // unexecutable fails at the moment someone tries to use it.
        if is_executable(&source)? {
            set_mode(&target, 0o755)?;
        }
    }
    Ok(())
}

fn current_hash(path: &Path) -> Result<Option<Hash>> {
    // symlink_metadata, so a dangling link counts as present rather than absent.
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return Ok(None);
    };
    if meta.is_dir() || meta.file_type().is_symlink() {
        // Neither is content-compared; the kind-specific no-op check covers them.
        return Ok(None);
    }
    Ok(Some(hash::file(path)?))
}

fn exists(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

fn is_executable(path: &Path) -> Result<bool> {
    use std::os::unix::fs::PermissionsExt;
    let Ok(meta) = std::fs::metadata(path) else {
        return Ok(false);
    };
    Ok(meta.permissions().mode() & 0o111 != 0)
}

fn set_mode(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .map_err(|e| Error::io(path, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_write_onto_matching_content_is_a_noop_and_applying_it_does_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.txt");
        std::fs::write(&path, "same").unwrap();

        let action = Action::write(&path, "same", "write f.txt").unwrap();
        assert!(action.noop);
        assert_eq!(action.line(), "write f.txt (already in place)");

        let outcome = apply(&[action]).unwrap();
        assert_eq!(outcome.applied, 0);
        assert_eq!(outcome.skipped, 1);
    }

    #[test]
    fn a_write_creates_missing_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a/b/c/f.txt");
        apply(&[Action::write(&path, "hello", "write").unwrap()]).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello");
    }

    #[test]
    fn an_executable_write_sets_the_bit_and_notices_when_it_is_missing() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hook.sh");

        apply(&[Action::write_executable(&path, "#!/bin/sh\n", "install hook").unwrap()]).unwrap();
        assert!(is_executable(&path).unwrap());

        // Same bytes, bit stripped: not a no-op, because the hook would not run.
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let action = Action::write_executable(&path, "#!/bin/sh\n", "install hook").unwrap();
        assert!(!action.noop);
        apply(&[action]).unwrap();
        assert!(is_executable(&path).unwrap());
    }

    #[test]
    fn a_plan_is_refused_when_the_file_moved_underneath_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.txt");
        std::fs::write(&path, "original").unwrap();

        let action = Action::write(&path, "planned", "write f.txt").unwrap();
        // An agent edits the same file between plan and apply.
        std::fs::write(&path, "an agent got here first").unwrap();

        let outcome = apply(&[action]).unwrap();
        assert_eq!(outcome.applied, 0);
        assert_eq!(outcome.stale, vec![path.clone()]);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "an agent got here first"
        );
    }

    #[test]
    fn a_copied_tree_drops_files_that_are_no_longer_in_the_source() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("source");
        let to = dir.path().join("dest");
        std::fs::create_dir_all(&from).unwrap();
        std::fs::write(from.join("SKILL.md"), "v2").unwrap();

        std::fs::create_dir_all(&to).unwrap();
        std::fs::write(to.join("SKILL.md"), "v1").unwrap();
        std::fs::write(to.join("removed-upstream.md"), "stale").unwrap();

        apply(&[Action::copy_tree(&from, &to, "install skill").unwrap()]).unwrap();
        assert_eq!(std::fs::read_to_string(to.join("SKILL.md")).unwrap(), "v2");
        assert!(!to.join("removed-upstream.md").exists());
    }

    #[test]
    fn a_copied_tree_keeps_the_executable_bit_on_a_script_inside_it() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("source");
        std::fs::create_dir_all(&from).unwrap();
        std::fs::write(from.join("run.js"), "#!/usr/bin/env node\n").unwrap();
        set_mode(&from.join("run.js"), 0o755).unwrap();

        let to = dir.path().join("dest");
        apply(&[Action::copy_tree(&from, &to, "install").unwrap()]).unwrap();
        assert!(is_executable(&to.join("run.js")).unwrap());
    }

    #[test]
    fn an_identical_tree_is_a_noop() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("source");
        let to = dir.path().join("dest");
        std::fs::create_dir_all(&from).unwrap();
        std::fs::write(from.join("SKILL.md"), "body").unwrap();
        apply(&[Action::copy_tree(&from, &to, "install").unwrap()]).unwrap();

        assert!(Action::copy_tree(&from, &to, "install").unwrap().noop);
    }

    #[test]
    fn a_symlink_replaces_one_pointing_elsewhere_but_not_a_directory_with_content() {
        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join("skills");

        apply(&[Action::symlink(&link, "../.agents/skills", "link").unwrap()]).unwrap();
        assert_eq!(
            std::fs::read_link(&link).unwrap().to_str(),
            Some("../.agents/skills")
        );
        assert!(
            Action::symlink(&link, "../.agents/skills", "link")
                .unwrap()
                .noop
        );

        // Pointing somewhere else is not a no-op, and gets replaced.
        std::fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink("somewhere/else", &link).unwrap();
        let action = Action::symlink(&link, "../.agents/skills", "link").unwrap();
        assert!(!action.noop);
        apply(&[action]).unwrap();
        assert_eq!(
            std::fs::read_link(&link).unwrap().to_str(),
            Some("../.agents/skills")
        );

        // A real directory with skills in it is someone's content, and is left alone.
        std::fs::remove_file(&link).unwrap();
        std::fs::create_dir_all(link.join("own-skill")).unwrap();
        let err =
            apply(&[Action::symlink(&link, "../.agents/skills", "link").unwrap()]).unwrap_err();
        assert!(format!("{err}").contains("migrate"), "got: {err}");
    }

    #[test]
    fn removing_something_that_is_not_there_is_a_noop() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            Action::remove(dir.path().join("gone"), "remove")
                .unwrap()
                .noop
        );
    }
}
