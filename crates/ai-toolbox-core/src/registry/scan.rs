//! Finding the repos on this machine.
//!
//! The registry fills up mostly by itself - anything the toolkit configures is recorded
//! as it happens. This is for the rest: repos set up before the registry existed, repos
//! configured on another machine and cloned here, and repos not configured at all that
//! you might want to.
//!
//! Bounded on purpose. An unbounded walk of a home directory reads a million files in
//! `node_modules` to find forty repos, and the forty are all within three or four levels
//! of a source root anyway.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::Registry;
use crate::error::Result;

/// Directories that never contain a repo worth registering, and do contain enormous
/// numbers of files. `.git` is here because descending into one finds its internals, not
/// a project.
const SKIP: [&str; 12] = [
    ".git",
    "node_modules",
    "target",
    "vendor",
    "dist",
    "build",
    ".venv",
    "venv",
    "__pycache__",
    ".next",
    ".cargo",
    "Library",
];

#[derive(Debug, Clone)]
pub struct ScanOptions {
    /// How far below a root to look. Four covers `~/src/org/project` and the odd
    /// `~/src/scratch/experiments/thing` without wandering.
    pub depth: usize,
}

impl Default for ScanOptions {
    fn default() -> ScanOptions {
        ScanOptions { depth: 4 }
    }
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ScanResult {
    /// Repos the registry had not seen before.
    pub added: Vec<PathBuf>,
    /// Repos it already knew about.
    pub known: Vec<PathBuf>,
    /// Roots that were asked for but are not there.
    pub missing_roots: Vec<PathBuf>,
}

impl ScanResult {
    pub fn found(&self) -> usize {
        self.added.len() + self.known.len()
    }
}

/// Walk each root and register every git repository underneath it.
pub fn scan(
    registry: &mut Registry,
    roots: &[PathBuf],
    options: &ScanOptions,
) -> Result<ScanResult> {
    let mut result = ScanResult::default();
    let mut seen = BTreeSet::new();

    for root in roots {
        if !root.is_dir() {
            result.missing_roots.push(root.clone());
            continue;
        }
        let mut found = Vec::new();
        walk(root, 0, options.depth, &mut found);

        for path in found {
            // A worktree resolves to its main repo, so scanning a directory holding a
            // repo and three of its worktrees registers one project, not four.
            let Ok(main) = super::main_worktree(&path) else {
                continue;
            };
            if !seen.insert(main.clone()) {
                continue;
            }
            let known = registry.get(&main)?.is_some();
            // `configured: false` - finding a repo is not setting one up, and the
            // difference is what the list shows.
            registry.record(&main, false)?;
            if known {
                result.known.push(main);
            } else {
                result.added.push(main);
            }
        }
    }

    result.added.sort();
    result.known.sort();
    Ok(result)
}

fn walk(dir: &Path, depth: usize, limit: usize, found: &mut Vec<PathBuf>) {
    if depth > limit {
        return;
    }
    // `.git` is a directory in a normal clone and a file in a worktree. Both mean "this
    // is a working tree", and neither is worth descending into.
    if dir.join(".git").exists() {
        found.push(dir.to_path_buf());
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        // An unreadable directory is skipped rather than fatal: a scan that dies on one
        // permission error somewhere under a home directory is a scan nobody can use.
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // Never follow a symlink: one pointing at a parent turns the walk into a loop.
        if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') && name != ".git" {
            continue;
        }
        if SKIP.contains(&name.as_ref()) {
            continue;
        }
        walk(&path, depth + 1, limit, found);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{git, Lab};

    fn repo_at(path: &Path) {
        std::fs::create_dir_all(path).unwrap();
        git(path, &["init", "-q", "-b", "main", "."]);
    }

    #[test]
    fn finds_repos_under_a_root_and_records_each_once() {
        let root = tempfile::tempdir().unwrap();
        repo_at(&root.path().join("alpha"));
        repo_at(&root.path().join("org/beta"));
        std::fs::create_dir_all(root.path().join("not-a-repo")).unwrap();

        let mut registry = Registry::memory().unwrap();
        let result = scan(
            &mut registry,
            &[root.path().to_path_buf()],
            &ScanOptions::default(),
        )
        .unwrap();

        assert_eq!(result.added.len(), 2, "{:?}", result.added);
        assert_eq!(registry.all().unwrap().len(), 2);

        // A second scan finds the same two and adds nothing.
        let again = scan(
            &mut registry,
            &[root.path().to_path_buf()],
            &ScanOptions::default(),
        )
        .unwrap();
        assert!(again.added.is_empty());
        assert_eq!(again.known.len(), 2);
    }

    #[test]
    fn a_repo_and_its_worktrees_are_one_project() {
        let lab = Lab::new();
        lab.add_worktree("feature");
        lab.add_worktree("hotfix");
        // The lab puts the worktrees beside the main repo, which is how people lay them
        // out - and exactly the case that would otherwise register three projects.
        let root = lab.main().parent().unwrap().to_path_buf();

        let mut registry = Registry::memory().unwrap();
        let result = scan(&mut registry, &[root], &ScanOptions::default()).unwrap();

        assert_eq!(result.found(), 1, "got {:?}", result.added);
        assert_eq!(registry.all().unwrap().len(), 1);
    }

    #[test]
    fn a_scan_does_not_mark_anything_as_configured() {
        let root = tempfile::tempdir().unwrap();
        repo_at(&root.path().join("alpha"));

        let mut registry = Registry::memory().unwrap();
        scan(
            &mut registry,
            &[root.path().to_path_buf()],
            &ScanOptions::default(),
        )
        .unwrap();

        let listed = registry.all().unwrap();
        assert!(
            listed[0].last_configured.is_none(),
            "finding a repo is not setting one up"
        );
    }

    #[test]
    fn the_walk_does_not_descend_into_a_repo_or_into_node_modules() {
        let root = tempfile::tempdir().unwrap();
        let outer = root.path().join("outer");
        repo_at(&outer);
        // A repo nested inside another is not reached, because the walk stops at the
        // first working tree it finds.
        repo_at(&outer.join("nested"));
        // And a repo inside node_modules is somebody's dependency, not a project.
        repo_at(&root.path().join("app/node_modules/pkg"));
        std::fs::create_dir_all(root.path().join("app")).unwrap();

        let mut found = Vec::new();
        walk(root.path(), 0, 4, &mut found);
        assert_eq!(found, vec![outer]);
    }

    #[test]
    fn the_walk_is_bounded_by_depth() {
        let root = tempfile::tempdir().unwrap();
        repo_at(&root.path().join("a/b/c/d/e/deep"));

        let mut shallow = Vec::new();
        walk(root.path(), 0, 2, &mut shallow);
        assert!(shallow.is_empty());

        let mut deeper = Vec::new();
        walk(root.path(), 0, 8, &mut deeper);
        assert_eq!(deeper.len(), 1);
    }

    #[test]
    fn a_symlink_pointing_at_a_parent_does_not_loop() {
        let root = tempfile::tempdir().unwrap();
        let inside = root.path().join("inside");
        std::fs::create_dir_all(&inside).unwrap();
        std::os::unix::fs::symlink(root.path(), inside.join("loop")).unwrap();
        repo_at(&root.path().join("alpha"));

        // Without the symlink guard this recurses until the depth limit, re-finding the
        // same repo at every level.
        let mut found = Vec::new();
        walk(root.path(), 0, 4, &mut found);
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn a_root_that_is_not_there_is_reported_rather_than_fatal() {
        let mut registry = Registry::memory().unwrap();
        let result = scan(
            &mut registry,
            &[PathBuf::from("/nowhere/at/all")],
            &ScanOptions::default(),
        )
        .unwrap();
        assert_eq!(result.missing_roots.len(), 1);
        assert_eq!(result.found(), 0);
    }
}
