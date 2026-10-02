//! Network and installer processes run off the async executor. No caller can supply a
//! URL or a destination: only the release tag it has already previewed.

use ai_toolbox_core::update::{self, Status, Updated};
use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};

use crate::{AppState, Error, Result};

pub fn routes() -> Router<AppState> {
    Router::new().route("/update", get(check).post(apply))
}

#[derive(serde::Serialize)]
struct Checked {
    #[serde(flatten)]
    status: Status,
    installed: Option<Updated>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    tag: String,
}

async fn check(State(state): State<AppState>) -> Result<Json<Checked>> {
    tokio::task::spawn_blocking(move || {
        let installed = state
            .update
            .try_lock()
            .map_err(|_| {
                Error::bad_request("An update is already running. Wait for it to finish.")
            })?
            .clone();
        Ok(Json(Checked {
            status: update::check().map_err(|e| Error::bad_request(format!("{e:#}")))?,
            installed,
        }))
    })
    .await
    .map_err(|e| Error::bad_request(format!("update task failed: {e}")))?
}

async fn apply(
    State(state): State<AppState>,
    Json(request): Json<Request>,
) -> Result<Json<Updated>> {
    tokio::task::spawn_blocking(move || {
        let mut installed = state.update.try_lock().map_err(|_| {
            Error::bad_request("An update is already running. Wait for it to finish.")
        })?;
        if let Some(result) = installed.as_ref() {
            return Ok(Json(result.clone()));
        }
        let result =
            update::apply(&request.tag).map_err(|e| Error::bad_request(format!("{e:#}")))?;
        *installed = Some(result.clone());
        Ok(Json(result))
    })
    .await
    .map_err(|e| Error::bad_request(format!("update task failed: {e}")))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use ai_toolbox_core::{registry::Registry, testing};

    fn state() -> AppState {
        AppState::new(
            testing::catalogue_root(),
            Registry::memory().unwrap(),
            "token",
        )
    }

    #[tokio::test]
    async fn another_tab_gets_the_completed_result_without_a_second_install() {
        let state = state();
        *state.update.lock().unwrap() = Some(Updated {
            version: "0.2.0".into(),
            restart_required: true,
            output: "done".into(),
        });
        let result = apply(
            State(state),
            Json(Request {
                tag: "v0.2.0".into(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(result.0.version, "0.2.0");
        assert!(result.0.restart_required);
    }

    #[tokio::test]
    async fn simultaneous_requests_are_refused_not_queued() {
        let state = state();
        let other = state.clone();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _guard = other.update.lock().unwrap();
            ready_tx.send(()).unwrap();
            release_rx.recv().unwrap();
        });
        ready_rx.recv().unwrap();
        let result = apply(
            State(state),
            Json(Request {
                tag: "v0.2.0".into(),
            }),
        )
        .await;
        release_tx.send(()).unwrap();
        worker.join().unwrap();
        assert!(result.unwrap_err().to_string().contains("already running"));
    }
}
