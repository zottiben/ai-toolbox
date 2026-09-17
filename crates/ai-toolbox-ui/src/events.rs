//! Telling the browser that somebody else wrote.
//!
//! ai-planner can ask SQLite whether another connection has committed. Here the source of
//! truth is the filesystem, and there is no equivalent question - so the server polls a
//! stat-only fingerprint of the repos a browser actually has open, and says so when one
//! moves.
//!
//! Only the open ones. Watching every registered repo would mean stat-walking seventeen
//! trees on a timer to keep one panel fresh, and the interest is declared by the SSE
//! connection itself: it arrives when a project is opened and goes when the tab closes.

use std::collections::HashMap;
use std::convert::Infallible;
use std::path::PathBuf;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use axum::Router;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};

use ai_toolbox_core::worktree;

use crate::state::{AppState, Change};

/// How often the watcher looks. Short enough that a board feels live, long enough that an
/// idle laptop is doing nothing measurable - a stat walk of one repo is microseconds.
const TICK: Duration = Duration::from_millis(1200);

pub fn routes() -> Router<AppState> {
    Router::new().route("/events", get(events))
}

/// Poll the open repos and announce every change. One task per server, not one per
/// client: ten open tabs must not mean ten pollers.
pub fn watch(state: AppState, tx: broadcast::Sender<Change>) {
    tokio::spawn(async move {
        let mut seen: HashMap<PathBuf, u64> = HashMap::new();
        loop {
            tokio::time::sleep(TICK).await;
            let open = state.watched();
            // A repo nobody is looking at any more should not keep a stale reading that
            // fires the moment somebody opens it again.
            seen.retain(|path, _| open.contains(path));

            for repo in open {
                let current = worktree::fingerprint(&repo);
                match seen.get(&repo) {
                    Some(previous) if *previous == current => {}
                    Some(_) => {
                        seen.insert(repo.clone(), current);
                        let _ = tx.send(Change { repo });
                    }
                    // First reading: record it, and say nothing. Announcing here would
                    // make every newly opened project look like it had just changed.
                    None => {
                        seen.insert(repo, current);
                    }
                }
            }
        }
    });
}

#[derive(Debug, serde::Deserialize)]
pub struct Watch {
    /// The project this client is looking at. Without it the stream still delivers what
    /// other tabs announce, but nothing is polled on its behalf.
    project: Option<i64>,
}

async fn events(
    State(state): State<AppState>,
    Query(watch): Query<Watch>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Held by the stream, so interest lasts exactly as long as the connection.
    let guard = match watch
        .project
        .and_then(|id| crate::read::find(&state, id).ok())
    {
        Some(repo) => Some(state.watch(repo.path)),
        None => None,
    };

    let stream = BroadcastStream::new(state.subscribe()).map(move |message| {
        // Keeps the guard alive for the life of the stream without doing anything with
        // it - dropping it here is what releases the watch.
        let _ = &guard;
        Ok(match message {
            Ok(change) => Event::default()
                .event("changed")
                .data(change.repo.to_string_lossy()),
            // Lagged: this client fell behind the buffer. The event says only "something
            // changed", so a missed one and a delivered one mean the same thing.
            Err(_) => Event::default().event("changed").data(""),
        })
    });

    Sse::new(stream).keep_alive(
        // Without this a proxy, or a laptop that slept, drops a silent connection and the
        // board looks live while being dead. EventSource reconnects on its own once the
        // socket actually closes.
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}
