//! The SQLite file behind the registry.
//!
//! Same shape as ai-planner's store, for the same reasons: WAL so a reader is not blocked
//! by a writer, `busy_timeout` so a collision is a short wait rather than an error, and
//! `IMMEDIATE` transactions so a concurrent writer waits up front instead of failing
//! halfway through.
//!
//! That matters more than the size of the data suggests. This file is written by the
//! CLI, by the board's server, and by however many agents are running in however many
//! worktrees - which is precisely the case a JSON file cannot survive (D4).

use std::path::{Path, PathBuf};

use rusqlite::{Connection, Transaction, TransactionBehavior};

use crate::error::{Error, Result};

const MIGRATIONS: &[(i64, &str, &str)] = &[(1, "repos", include_str!("migrations/001_repos.sql"))];

/// Override for tests, or for anyone who wants the registry somewhere else.
pub const DB_ENV: &str = "AI_TOOLBOX_DB";

pub fn default_path() -> PathBuf {
    if let Some(explicit) = std::env::var_os(DB_ENV) {
        return PathBuf::from(explicit);
    }
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(xdg).join("ai-toolbox").join("toolbox.db");
    }
    crate::machine::home()
        .join(".ai-toolbox")
        .join("toolbox.db")
}

pub struct Db {
    conn: Connection,
    path: PathBuf,
}

impl Db {
    /// Open the registry, creating it if it is not there.
    ///
    /// Creating on demand is right here in a way it is not for ai-planner: this file is
    /// an index that can be rebuilt by scanning, so losing it costs nothing and a first
    /// run should not need a setup step.
    pub fn open(path: &Path) -> Result<Db> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
        }
        let conn = Connection::open(path).map_err(|e| Error::db(path, e))?;
        configure(&conn).map_err(|e| Error::db(path, e))?;
        let mut db = Db {
            conn,
            path: path.to_path_buf(),
        };
        db.migrate()?;
        Ok(db)
    }

    pub fn open_default() -> Result<Db> {
        Db::open(&default_path())
    }

    /// Whether a registry exists yet, without creating one. Read-only commands ask, so
    /// that looking at a repo does not quietly make a file in the home directory.
    pub fn exists() -> bool {
        default_path().is_file()
    }

    #[cfg(any(test, feature = "testing"))]
    pub fn memory() -> Result<Db> {
        let conn = Connection::open_in_memory().map_err(|e| Error::db(":memory:", e))?;
        configure(&conn).map_err(|e| Error::db(":memory:", e))?;
        let mut db = Db {
            conn,
            path: PathBuf::from(":memory:"),
        };
        db.migrate()?;
        Ok(db)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn write<T>(&mut self, f: impl FnOnce(&Transaction<'_>) -> Result<T>) -> Result<T> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| Error::db(&self.path, e))?;
        let out = f(&tx)?;
        tx.commit().map_err(|e| Error::db(&self.path, e))?;
        Ok(out)
    }

    fn migrate(&mut self) -> Result<()> {
        let path = self.path.clone();
        self.conn
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS schema_migrations (
                     version    INTEGER PRIMARY KEY,
                     name       TEXT NOT NULL,
                     applied_at TEXT NOT NULL
                 )",
            )
            .map_err(|e| Error::db(&path, e))?;
        let applied: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                [],
                |row| row.get(0),
            )
            .map_err(|e| Error::db(&path, e))?;

        for (version, name, sql) in MIGRATIONS {
            if *version <= applied {
                continue;
            }
            let tx = self
                .conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(|e| Error::db(&path, e))?;
            tx.execute_batch(sql).map_err(|e| Error::db(&path, e))?;
            tx.execute(
                "INSERT INTO schema_migrations (version, name, applied_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![version, name, crate::registry::now()],
            )
            .map_err(|e| Error::db(&path, e))?;
            tx.commit().map_err(|e| Error::db(&path, e))?;
        }
        Ok(())
    }

    pub fn schema_version(&self) -> Result<i64> {
        self.conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                [],
                |row| row.get(0),
            )
            .map_err(|e| Error::db(&self.path, e))
    }
}

fn configure(conn: &Connection) -> rusqlite::Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrating_twice_is_harmless() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("toolbox.db");

        let db = Db::open(&path).unwrap();
        assert_eq!(db.schema_version().unwrap(), 1);
        drop(db);

        let again = Db::open(&path).unwrap();
        assert_eq!(again.schema_version().unwrap(), 1);
    }

    #[test]
    fn the_parent_directory_is_created() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("nested/deeper/toolbox.db");
        Db::open(&path).unwrap();
        assert!(path.is_file());
    }
}
