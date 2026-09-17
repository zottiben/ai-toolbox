//! Everything the board asks for.
//!
//! Every answer is computed from the filesystem at the moment it is asked (D4). Nothing
//! here consults a cache, because a cached view of what is installed would go stale the
//! moment an agent edited `.mcp.json` - and noticing that is the whole job.

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};

use ai_toolbox_core::registry::{self, Repo, Summary};
use ai_toolbox_core::{harness, machine, survey, worktree, Survey};

use crate::error::{Error, Result};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/machine", get(machine_info))
        .route("/catalogue", get(catalogue))
        .route("/projects", get(projects))
        .route("/projects/{id}", get(project))
}

/// The once-per-machine facts, which are easy to mistake for repo state and are not.
async fn machine_info(State(state): State<AppState>) -> Result<Json<serde_json::Value>> {
    let catalogue = state.catalogue()?;
    Ok(Json(serde_json::json!({
        "machine": machine::Machine::read(),
        "catalogue_root": catalogue.root,
        "version": env!("CARGO_PKG_VERSION"),
        "harnesses": harness::ALL,
        "bundle": crate::bundle(),
    })))
}

async fn catalogue(State(state): State<AppState>) -> Result<Json<ai_toolbox_core::Catalogue>> {
    Ok(Json(state.catalogue()?))
}

/// The project list. A full survey per repo, which costs a few milliseconds each and is
/// the reason the list can never be wrong about a repo an agent has just edited.
async fn projects(State(state): State<AppState>) -> Result<Json<Vec<Summary>>> {
    let catalogue = state.catalogue()?;
    let repos = state.registry(|registry| Ok(registry.all()?))?;
    let summaries = repos
        .iter()
        .map(|repo| registry::summarise(repo, &catalogue))
        .collect::<ai_toolbox_core::Result<Vec<_>>>()?;
    Ok(Json(summaries))
}

/// One project in full: what is installed, what is wrong with it, and how its worktrees
/// compare.
///
/// The findings arrive inside the survey, because the survey's one-line `state` is
/// derived from them - computing the two separately is what let the list call a repo
/// healthy while its hooks were broken.
#[derive(serde::Serialize)]
pub struct Detail {
    pub repo: Repo,
    pub exists: bool,
    #[serde(flatten)]
    pub survey: Option<Survey>,
    pub worktrees: Option<worktree::Comparison>,
    /// Secrets the servers in this repo need, and where each has to go. Read off the
    /// catalogue entries the repo's servers came from.
    pub secrets: Vec<ai_toolbox_core::secrets::Secret>,
}

async fn project(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<Detail>> {
    let catalogue = state.catalogue()?;
    let repo = find(&state, id)?;

    // A repo whose directory has gone is still a row, and the board has to be able to
    // show it and offer to forget it rather than failing to render the page.
    if !repo.exists() {
        return Ok(Json(Detail {
            repo,
            exists: false,
            survey: None,
            worktrees: None,
            secrets: Vec::new(),
        }));
    }

    let survey = survey(&repo.path, &catalogue)?;
    let comparison = worktree::compare(&repo.path)?;

    let mut secrets = Vec::new();
    for server in &survey.inventory.servers {
        let Some(preset) = catalogue.preset_for_server(&server.name) else {
            continue;
        };
        for secret in &preset.secrets {
            if !secrets.contains(secret) {
                secrets.push(secret.clone());
            }
        }
    }

    Ok(Json(Detail {
        repo,
        exists: true,
        survey: Some(survey),
        worktrees: Some(comparison),
        secrets,
    }))
}

pub fn find(state: &AppState, id: i64) -> Result<Repo> {
    state.registry(|registry| {
        registry
            .all()?
            .into_iter()
            .find(|repo| repo.id == id)
            .ok_or_else(|| Error::not_found(format!("no project {id}")))
    })
}
