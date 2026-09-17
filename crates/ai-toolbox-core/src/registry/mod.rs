//! Which repos to look at (D4).
//!
//! The filesystem is the source of truth for everything else. This answers one question
//! only - *where are the repos* - because walking the disk on every page load is not a
//! way to render a list, and because a repo configured six months ago should still be
//! findable without remembering where it is.
//!
//! What it must never become is a cache of installed state. That would go stale the
//! moment an agent edited `.mcp.json`, and going stale is exactly the failure this tool
//! exists to catch. Rows here carry paths and dates; every question about content is
//! answered by reading the files.

pub mod db;
mod scan;

use std::path::{Path, PathBuf};

pub use db::Db;
pub use scan::{scan, ScanOptions};

use crate::error::{Error, Result};
use crate::{classify, detect, git, harness, worktree, Catalogue, Harness, Inventory, State};

#[derive(Debug, Clone, serde::Serialize)]
pub struct Repo {
    pub id: i64,
    pub path: PathBuf,
    pub first_seen: String,
    pub last_seen: String,
    /// `None` for a repo that was discovered rather than set up. That difference is what
    /// separates "I found this" from "you configured this".
    pub last_configured: Option<String>,
}

impl Repo {
    /// The directory name, which is what a list shows.
    pub fn name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string())
    }

    /// Whether the path is still there. A repo that has been deleted or moved stays in
    /// the registry until someone says otherwise - silently dropping rows would make
    /// an unmounted volume look like a decision.
    pub fn exists(&self) -> bool {
        self.path.is_dir()
    }
}

pub struct Registry {
    db: Db,
}

impl Registry {
    pub fn open() -> Result<Registry> {
        Ok(Registry {
            db: Db::open_default()?,
        })
    }

    pub fn at(path: &Path) -> Result<Registry> {
        Ok(Registry {
            db: Db::open(path)?,
        })
    }

    #[cfg(any(test, feature = "testing"))]
    pub fn memory() -> Result<Registry> {
        Ok(Registry { db: Db::memory()? })
    }

    pub fn db(&self) -> &Db {
        &self.db
    }

    /// Note that a repo exists, and optionally that it was just configured.
    ///
    /// Takes any path inside the repo and records the *main worktree*, so four worktrees
    /// of one project are one row rather than four projects (D6). A path outside any git
    /// repository is recorded as itself - the toolkit works in a directory that was never
    /// `git init`ed, and refusing to remember one would be worse than remembering it
    /// under its own name.
    pub fn record(&mut self, path: &Path, configured: bool) -> Result<Repo> {
        let canonical = main_worktree(path)?;
        let text = canonical.to_string_lossy().into_owned();
        let stamp = now();

        self.db.write(|tx| {
            tx.execute(
                "INSERT INTO repos (path, first_seen, last_seen, last_configured)
                 VALUES (?1, ?2, ?2, CASE WHEN ?3 THEN ?2 ELSE NULL END)
                 ON CONFLICT(path) DO UPDATE SET
                     last_seen = ?2,
                     -- A read must not clear the mark that says this was configured.
                     last_configured = CASE WHEN ?3 THEN ?2 ELSE last_configured END",
                rusqlite::params![&text, &stamp, configured],
            )
            .map_err(|e| Error::db(&text, e))?;
            Ok(())
        })?;

        self.get(&canonical)?
            .ok_or_else(|| Error::Catalogue(format!("{} was not recorded", canonical.display())))
    }

    pub fn get(&self, path: &Path) -> Result<Option<Repo>> {
        let text = path.to_string_lossy().into_owned();
        let mut statement = self
            .db
            .conn()
            .prepare("SELECT id, path, first_seen, last_seen, last_configured FROM repos WHERE path = ?1")
            .map_err(|e| Error::db(self.db.path(), e))?;
        let mut rows = statement
            .query_map([&text], row_to_repo)
            .map_err(|e| Error::db(self.db.path(), e))?;
        match rows.next() {
            Some(row) => Ok(Some(row.map_err(|e| Error::db(self.db.path(), e))?)),
            None => Ok(None),
        }
    }

    pub fn all(&self) -> Result<Vec<Repo>> {
        let mut statement = self
            .db
            .conn()
            .prepare(
                "SELECT id, path, first_seen, last_seen, last_configured FROM repos
                 ORDER BY last_configured IS NULL, last_seen DESC, path",
            )
            .map_err(|e| Error::db(self.db.path(), e))?;
        let rows = statement
            .query_map([], row_to_repo)
            .map_err(|e| Error::db(self.db.path(), e))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| Error::db(self.db.path(), e))
    }

    /// Remove a repo from the list. The files are not touched - this forgets, it does
    /// not uninstall.
    pub fn forget(&mut self, path: &Path) -> Result<bool> {
        let text = path.to_string_lossy().into_owned();
        self.db.write(|tx| {
            let removed = tx
                .execute("DELETE FROM repos WHERE path = ?1", [&text])
                .map_err(|e| Error::db(&text, e))?;
            Ok(removed > 0)
        })
    }

    /// Drop every row whose directory is gone. Only on request, never automatically.
    pub fn prune(&mut self) -> Result<Vec<PathBuf>> {
        let gone: Vec<PathBuf> = self
            .all()?
            .into_iter()
            .filter(|repo| !repo.exists())
            .map(|repo| repo.path)
            .collect();
        for path in &gone {
            self.forget(path)?;
        }
        Ok(gone)
    }

    pub fn scan_roots(&self) -> Result<Vec<PathBuf>> {
        let mut statement = self
            .db
            .conn()
            .prepare("SELECT path FROM scan_roots ORDER BY path")
            .map_err(|e| Error::db(self.db.path(), e))?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0).map(PathBuf::from))
            .map_err(|e| Error::db(self.db.path(), e))?;
        let roots = rows
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| Error::db(self.db.path(), e))?;
        // `~/src` is where this machine keeps its repos, and a default that finds
        // something beats a first run that finds nothing and looks broken.
        if roots.is_empty() {
            let default = crate::machine::home().join("src");
            if default.is_dir() {
                return Ok(vec![default]);
            }
        }
        Ok(roots)
    }

    pub fn add_scan_root(&mut self, path: &Path) -> Result<()> {
        let text = path.to_string_lossy().into_owned();
        let stamp = now();
        self.db.write(|tx| {
            tx.execute(
                "INSERT INTO scan_roots (path, added_at) VALUES (?1, ?2)
                 ON CONFLICT(path) DO NOTHING",
                rusqlite::params![&text, &stamp],
            )
            .map_err(|e| Error::db(&text, e))?;
            Ok(())
        })
    }
}

fn row_to_repo(row: &rusqlite::Row<'_>) -> rusqlite::Result<Repo> {
    Ok(Repo {
        id: row.get(0)?,
        path: PathBuf::from(row.get::<_, String>(1)?),
        first_seen: row.get(2)?,
        last_seen: row.get(3)?,
        last_configured: row.get(4)?,
    })
}

/// The main worktree containing `path`, canonicalised.
pub fn main_worktree(path: &Path) -> Result<PathBuf> {
    let canonical = std::fs::canonicalize(path).map_err(|e| Error::io(path, e))?;
    let worktrees = git::worktrees(&canonical);
    let main = worktrees
        .iter()
        .find(|w| w.is_main)
        .map(|w| w.path.clone())
        .unwrap_or(canonical);
    Ok(std::fs::canonicalize(&main).unwrap_or(main))
}

/// One repo's line in the list.
///
/// Everything here is read from disk on demand. It is a full survey rather than a cached
/// digest, which costs a few milliseconds per repo and is the reason the list can never
/// be wrong about a repo an agent has just edited.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Summary {
    pub repo: Repo,
    pub exists: bool,
    pub state: State,
    pub harnesses: Vec<Harness>,
    pub counts: classify::Counts,
    pub stack: Vec<detect::Stack>,
    /// How many worktrees, and whether they all match. A repo whose worktrees disagree
    /// is a repo where half your branches have no hooks.
    pub worktrees: usize,
    pub worktrees_in_step: bool,
}

pub fn summarise(repo: &Repo, catalogue: &Catalogue) -> Result<Summary> {
    if !repo.exists() {
        return Ok(Summary {
            repo: repo.clone(),
            exists: false,
            state: State::Unconfigured,
            harnesses: Vec::new(),
            counts: classify::Counts::default(),
            stack: Vec::new(),
            worktrees: 0,
            worktrees_in_step: true,
        });
    }
    let inventory = Inventory::read(&repo.path)?;
    let report = classify::classify(&inventory, catalogue);
    let comparison = worktree::compare(&repo.path)?;
    Ok(Summary {
        state: classify::state(&inventory, &report),
        harnesses: harness::configured(&repo.path),
        counts: report.counts(),
        stack: detect::recommend(&repo.path).detected,
        worktrees: comparison.worktrees.len(),
        worktrees_in_step: comparison.is_uniform(),
        exists: true,
        repo: repo.clone(),
    })
}

pub(crate) fn now() -> String {
    use time::format_description::well_known::Rfc3339;
    time::OffsetDateTime::now_utc()
        .replace_nanosecond(0)
        .unwrap_or_else(|_| time::OffsetDateTime::now_utc())
        .format(&Rfc3339)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_repo_is_recorded_once_and_updated_on_the_next_sighting() {
        let temp = tempfile::tempdir().unwrap();
        let mut registry = Registry::memory().unwrap();

        let first = registry.record(temp.path(), false).unwrap();
        assert!(first.last_configured.is_none(), "seeing is not configuring");

        let again = registry.record(temp.path(), false).unwrap();
        assert_eq!(again.id, first.id, "one row, not two");
        assert_eq!(registry.all().unwrap().len(), 1);
    }

    #[test]
    fn configuring_marks_the_repo_and_a_later_read_does_not_clear_the_mark() {
        let temp = tempfile::tempdir().unwrap();
        let mut registry = Registry::memory().unwrap();

        registry.record(temp.path(), true).unwrap();
        let after_read = registry.record(temp.path(), false).unwrap();
        assert!(
            after_read.last_configured.is_some(),
            "a plain sighting must not unset last_configured"
        );
    }

    #[test]
    fn every_worktree_of_a_repo_is_the_same_project() {
        let lab = crate::testing::Lab::new();
        let feature = lab.add_worktree("feature");
        let mut registry = Registry::memory().unwrap();

        let from_main = registry.record(lab.main(), false).unwrap();
        let from_worktree = registry.record(&feature, false).unwrap();

        assert_eq!(from_main.id, from_worktree.id);
        assert_eq!(
            registry.all().unwrap().len(),
            1,
            "four branches, one project"
        );
    }

    #[test]
    fn forgetting_removes_the_row_and_leaves_the_files_alone() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("AGENTS.md"), "# keep me").unwrap();
        let mut registry = Registry::memory().unwrap();
        let repo = registry.record(temp.path(), true).unwrap();

        assert!(registry.forget(&repo.path).unwrap());
        assert!(registry.all().unwrap().is_empty());
        assert!(
            temp.path().join("AGENTS.md").is_file(),
            "forget is not uninstall"
        );
        assert!(
            !registry.forget(&repo.path).unwrap(),
            "forgetting twice is not an error"
        );
    }

    #[test]
    fn a_deleted_repo_stays_listed_until_it_is_pruned() {
        let temp = tempfile::tempdir().unwrap();
        let gone = temp.path().join("gone");
        std::fs::create_dir_all(&gone).unwrap();
        let mut registry = Registry::memory().unwrap();
        registry.record(&gone, true).unwrap();

        std::fs::remove_dir_all(&gone).unwrap();
        let listed = registry.all().unwrap();
        assert_eq!(
            listed.len(),
            1,
            "an unmounted volume is not a decision to forget"
        );
        assert!(!listed[0].exists());

        let pruned = registry.prune().unwrap();
        assert_eq!(pruned.len(), 1);
        assert!(registry.all().unwrap().is_empty());
    }

    #[test]
    fn configured_repos_are_listed_before_ones_that_were_only_found() {
        let temp = tempfile::tempdir().unwrap();
        let found = temp.path().join("found");
        let set_up = temp.path().join("set-up");
        std::fs::create_dir_all(&found).unwrap();
        std::fs::create_dir_all(&set_up).unwrap();

        let mut registry = Registry::memory().unwrap();
        registry.record(&found, false).unwrap();
        registry.record(&set_up, true).unwrap();

        let listed = registry.all().unwrap();
        assert!(listed[0].last_configured.is_some());
        assert_eq!(listed[0].name(), "set-up");
    }

    #[test]
    fn a_scan_root_is_added_once() {
        let temp = tempfile::tempdir().unwrap();
        let mut registry = Registry::memory().unwrap();
        registry.add_scan_root(temp.path()).unwrap();
        registry.add_scan_root(temp.path()).unwrap();
        assert_eq!(registry.scan_roots().unwrap().len(), 1);
    }

    #[test]
    fn summarising_reads_the_disk_rather_than_anything_remembered() {
        let fixture = crate::testing::Fixture::configured();
        let mut registry = Registry::memory().unwrap();
        let repo = registry.record(fixture.path(), true).unwrap();

        let catalogue = crate::testing::catalogue();
        let before = summarise(&repo, &catalogue).unwrap();
        assert_eq!(before.state, State::Healthy);

        // An agent breaks the repo. The registry knows nothing about it, and that is the
        // point: the summary is a fresh read, so it notices.
        fixture.remove(".agents/skills");
        let after = summarise(&repo, &catalogue).unwrap();
        assert_eq!(after.state, State::Broken);
    }

    #[test]
    fn a_summary_of_a_deleted_repo_does_not_fail() {
        let temp = tempfile::tempdir().unwrap();
        let gone = temp.path().join("gone");
        std::fs::create_dir_all(&gone).unwrap();
        let mut registry = Registry::memory().unwrap();
        let repo = registry.record(&gone, true).unwrap();
        std::fs::remove_dir_all(&gone).unwrap();

        let summary = summarise(&repo, &crate::testing::catalogue()).unwrap();
        assert!(!summary.exists);
    }
}
