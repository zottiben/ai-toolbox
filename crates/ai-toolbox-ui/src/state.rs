//! What every handler shares: the catalogue, the registry, and who is watching what.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use ai_toolbox_core::registry::Registry;
use ai_toolbox_core::{Catalogue, Error as CoreError};
use tokio::sync::broadcast;

use crate::error::{Error, Result};

#[derive(Clone)]
pub struct AppState {
    /// Re-read on demand rather than held, because the clone is a working directory: a
    /// hook edited in it should show up in the board without a restart (D2).
    catalogue_root: PathBuf,
    /// `Registry` needs `&mut` to write, so it is behind a mutex rather than cloned per
    /// request. Not a bottleneck to design around - every operation is a local SQLite
    /// query measured in microseconds, and SQLite serialises writes anyway.
    registry: Arc<Mutex<Registry>>,
    token: Arc<str>,
    changes: broadcast::Sender<Change>,
    /// Which repos have a browser looking at them, and how many. Only these are polled
    /// for changes - watching every registered repo would mean stat-walking seventeen
    /// trees on a timer to keep one panel fresh.
    watched: Arc<Mutex<HashMap<PathBuf, usize>>>,
}

/// What the board is told. Deliberately thin: the client refetches, so the message only
/// has to say *which* repo, not what about it moved.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Change {
    pub repo: PathBuf,
}

impl AppState {
    pub fn new(
        catalogue_root: PathBuf,
        registry: Registry,
        token: impl Into<Arc<str>>,
    ) -> AppState {
        // Small on purpose. A client that falls behind loses nothing by being told once
        // instead of eight times - it refetches either way.
        let (changes, _) = broadcast::channel(16);
        AppState {
            catalogue_root,
            registry: Arc::new(Mutex::new(registry)),
            token: token.into(),
            changes,
            watched: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn catalogue_root(&self) -> &std::path::Path {
        &self.catalogue_root
    }

    /// The catalogue as it is on disk right now.
    pub fn catalogue(&self) -> Result<Catalogue> {
        Ok(Catalogue::load(&self.catalogue_root)?)
    }

    pub fn changes(&self) -> broadcast::Sender<Change> {
        self.changes.clone()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Change> {
        self.changes.subscribe()
    }

    /// Borrow the registry. The closure is synchronous on purpose: holding a
    /// `std::sync::MutexGuard` across an `.await` is a deadlock waiting to happen, and a
    /// sync closure makes that impossible to write by accident.
    pub fn registry<T>(&self, f: impl FnOnce(&mut Registry) -> Result<T>) -> Result<T> {
        let mut guard = self.registry.lock().map_err(|_| poisoned())?;
        f(&mut guard)
    }

    /// Say that a repo just changed because *we* changed it, so other open tabs catch up
    /// without waiting for the poller to notice.
    pub fn announce(&self, repo: PathBuf) {
        // Err means nobody is listening, which is the normal state with no browser open.
        let _ = self.changes.send(Change { repo });
    }

    /// Register interest in a repo for as long as the returned guard lives.
    pub fn watch(&self, repo: PathBuf) -> Watching {
        if let Ok(mut watched) = self.watched.lock() {
            *watched.entry(repo.clone()).or_insert(0) += 1;
        }
        Watching {
            watched: Arc::clone(&self.watched),
            repo,
        }
    }

    pub fn watched(&self) -> Vec<PathBuf> {
        self.watched
            .lock()
            .map(|w| w.keys().cloned().collect())
            .unwrap_or_default()
    }
}

/// Drops interest when the SSE stream ends. Without this, a closed tab would leave the
/// server polling its repo for the rest of the process's life.
pub struct Watching {
    watched: Arc<Mutex<HashMap<PathBuf, usize>>>,
    repo: PathBuf,
}

impl Drop for Watching {
    fn drop(&mut self) {
        let Ok(mut watched) = self.watched.lock() else {
            return;
        };
        if let Some(count) = watched.get_mut(&self.repo) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                watched.remove(&self.repo);
            }
        }
    }
}

fn poisoned() -> Error {
    Error::Internal(CoreError::Catalogue(
        "the registry lock was poisoned by a panic in another request".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> AppState {
        AppState::new(
            ai_toolbox_core::testing::catalogue_root(),
            Registry::memory().unwrap(),
            "token",
        )
    }

    #[test]
    fn watching_is_counted_and_released_when_the_guard_drops() {
        let state = state();
        let repo = PathBuf::from("/src/thing");
        assert!(state.watched().is_empty());

        let first = state.watch(repo.clone());
        let second = state.watch(repo.clone());
        assert_eq!(state.watched(), vec![repo.clone()]);

        // Two tabs on one repo is one thing to poll, and closing one must not stop it.
        drop(first);
        assert_eq!(state.watched(), vec![repo.clone()]);

        drop(second);
        assert!(
            state.watched().is_empty(),
            "a closed tab must not leave the server polling for ever"
        );
    }
}
