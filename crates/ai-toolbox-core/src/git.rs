//! The git facts this tool needs, which are only ever about worktrees.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, serde::Serialize)]
pub struct Worktree {
    pub path: PathBuf,
    pub head: Option<String>,
    /// Short branch name. `None` for a detached HEAD, which is a normal state for a
    /// worktree parked on a tag or a review.
    pub branch: Option<String>,
    /// The one the repository was cloned into. It is the reference every other worktree
    /// is compared against (D6), and git always lists it first.
    pub is_main: bool,
    pub bare: bool,
    pub locked: Option<String>,
    /// Git's own words for why this entry is dead - usually that the directory is gone.
    pub prunable: Option<String>,
}

impl Worktree {
    /// A name short enough for a table: the directory, or the branch when that says more.
    pub fn label(&self) -> String {
        match &self.branch {
            Some(branch) => branch.clone(),
            None => self
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| self.path.display().to_string()),
        }
    }

    /// Whether there is anything on disk to inspect. A prunable entry usually has not.
    pub fn is_usable(&self) -> bool {
        self.prunable.is_none() && !self.bare && self.path.is_dir()
    }
}

/// Every worktree of the repository `path` belongs to, main first.
///
/// An empty list means "not a git repository", not an error: `ai-toolbox` is useful in a
/// directory that was never `git init`ed, and refusing to report on one would be worse
/// than having nothing to say about its worktrees.
pub fn worktrees(path: &Path) -> Vec<Worktree> {
    let Some(output) = git(path, &["worktree", "list", "--porcelain"]) else {
        return Vec::new();
    };
    parse(&output)
}

/// Whether this path is inside a git repository at all.
pub fn is_repository(path: &Path) -> bool {
    git(path, &["rev-parse", "--git-dir"]).is_some()
}

/// The worktree containing `path`, by asking git rather than by comparing strings - a
/// path can reach the same directory through a symlink or a relative hop.
pub fn worktree_root(path: &Path) -> Option<PathBuf> {
    let output = git(path, &["rev-parse", "--show-toplevel"])?;
    let text = output.trim();
    (!text.is_empty()).then(|| PathBuf::from(text))
}

/// Records are separated by a blank line, and each opens with `worktree <path>`.
fn parse(text: &str) -> Vec<Worktree> {
    let mut found: Vec<Worktree> = Vec::new();
    for block in text.split("\n\n") {
        let mut current: Option<Worktree> = None;
        for line in block.lines() {
            let (key, value) = match line.split_once(' ') {
                Some((key, value)) => (key, value),
                // `detached` and `bare` are bare words with no value.
                None => (line, ""),
            };
            match key {
                "worktree" => {
                    current = Some(Worktree {
                        path: PathBuf::from(value),
                        head: None,
                        branch: None,
                        // Git lists the main worktree first, always.
                        is_main: found.is_empty(),
                        bare: false,
                        locked: None,
                        prunable: None,
                    });
                }
                "HEAD" => {
                    if let Some(worktree) = current.as_mut() {
                        worktree.head = Some(value.to_string());
                    }
                }
                "branch" => {
                    if let Some(worktree) = current.as_mut() {
                        worktree.branch = Some(value.trim_start_matches("refs/heads/").to_string());
                    }
                }
                "bare" => {
                    if let Some(worktree) = current.as_mut() {
                        worktree.bare = true;
                    }
                }
                "locked" => {
                    if let Some(worktree) = current.as_mut() {
                        // The reason is optional; an empty one still means locked.
                        worktree.locked = Some(value.to_string());
                    }
                }
                "prunable" => {
                    if let Some(worktree) = current.as_mut() {
                        worktree.prunable = Some(value.to_string());
                    }
                }
                _ => {}
            }
        }
        if let Some(worktree) = current {
            found.push(worktree);
        }
    }
    found
}

fn git(path: &Path, args: &[&str]) -> Option<String> {
    if !path.is_dir() {
        return None;
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_branch_a_detached_head_and_a_dead_entry() {
        let text = "worktree /src/main\nHEAD abc123\nbranch refs/heads/main\n\n\
                    worktree /src/gone\nHEAD abc123\ndetached\nprunable gitdir file points to non-existent location\n\n\
                    worktree /src/feature\nHEAD def456\nbranch refs/heads/toolbox-gui/pr4\n";
        let found = parse(text);
        assert_eq!(found.len(), 3);

        assert!(found[0].is_main, "git lists the main worktree first");
        assert_eq!(found[0].branch.as_deref(), Some("main"));

        assert!(!found[1].is_main);
        assert_eq!(found[1].branch, None, "a detached HEAD has no branch");
        assert!(found[1].prunable.is_some());
        assert!(!found[1].is_usable());

        // A branch name with a slash keeps all of it.
        assert_eq!(found[2].branch.as_deref(), Some("toolbox-gui/pr4"));
        assert_eq!(found[2].label(), "toolbox-gui/pr4");
    }

    #[test]
    fn a_detached_worktree_is_labelled_by_its_directory() {
        let found = parse("worktree /src/main\nHEAD a\nbranch refs/heads/main\n\nworktree /src/review\nHEAD b\ndetached\n");
        assert_eq!(found[1].label(), "review");
    }

    #[test]
    fn a_bare_main_repository_is_marked_and_not_inspected() {
        let found = parse(
            "worktree /src/bare\nbare\n\nworktree /src/work\nHEAD a\nbranch refs/heads/main\n",
        );
        assert!(found[0].bare);
        assert!(
            !found[0].is_usable(),
            "there is nothing to configure in a bare repo"
        );
    }

    #[test]
    fn a_locked_worktree_keeps_its_reason_and_is_still_usable() {
        let found =
            parse("worktree /src/main\nHEAD a\nbranch refs/heads/main\nlocked on a usb stick\n");
        assert_eq!(found[0].locked.as_deref(), Some("on a usb stick"));
    }

    #[test]
    fn a_directory_that_is_not_a_repository_has_no_worktrees() {
        let temp = tempfile::tempdir().unwrap();
        assert!(worktrees(temp.path()).is_empty());
        assert!(!is_repository(temp.path()));
    }
}
