//! Everything that changes a repo.
//!
//! Plan and apply are separate endpoints (D3), and the board always plans first. That is
//! what lets a button say "Repair (4 changes)" and list them before it touches anything -
//! a different product from one that says "Repair" and hopes.
//!
//! Both take the same request body, so previewing and doing are the same call with a
//! different verb rather than two code paths that could disagree.

use std::path::PathBuf;

use axum::extract::{Path, State};
use axum::routing::post;
use axum::{Json, Router};

use ai_toolbox_core::registry::{self, Repo, ScanOptions, ScanResult};
use ai_toolbox_core::{action, doctor, install, survey, worktree, Harness, Plan};

use crate::error::{Error, Result};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/projects/scan", post(scan))
        .route("/projects/add", post(add))
        .route("/projects/{id}/forget", post(forget))
        .route("/projects/{id}/plan", post(plan))
        .route("/projects/{id}/apply", post(apply))
}

/// What the board is asking for. One shape for preview and for commit.
#[derive(Debug, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Request {
    /// Install catalogue items, for a chosen set of harnesses.
    Install {
        #[serde(default)]
        hooks: Vec<String>,
        #[serde(default)]
        presets: Vec<String>,
        #[serde(default)]
        skills: Vec<String>,
        #[serde(default)]
        harnesses: Vec<Harness>,
        /// Scaffold AGENTS.md / CLAUDE.md as well.
        #[serde(default)]
        scaffold: bool,
    },
    /// Everything the stack detection recommends.
    Recommended {
        #[serde(default)]
        harnesses: Vec<Harness>,
        #[serde(default)]
        scaffold: bool,
    },
    /// Fix what doctor found and is willing to fix.
    Repair,
    /// Bring worktrees into step with the main one. All of them when `worktree` is null.
    Converge {
        #[serde(default)]
        worktree: Option<PathBuf>,
    },
}

/// A plan as the board reads it.
#[derive(serde::Serialize)]
pub struct PlanView {
    pub actions: Vec<ActionView>,
    pub changes: usize,
    pub warnings: Vec<String>,
    pub secrets: Vec<ai_toolbox_core::secrets::Secret>,
}

#[derive(serde::Serialize)]
pub struct ActionView {
    pub summary: String,
    pub path: PathBuf,
    pub noop: bool,
}

#[derive(serde::Serialize)]
pub struct Applied {
    pub applied: usize,
    pub skipped: usize,
    /// Paths that moved between planning and applying. Nothing was written for these.
    pub stale: Vec<PathBuf>,
}

async fn plan(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(request): Json<Request>,
) -> Result<Json<PlanView>> {
    let (plan, _) = build(&state, id, &request)?;
    Ok(Json(view(&plan)))
}

async fn apply(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(request): Json<Request>,
) -> Result<Json<Applied>> {
    let (plan, repo) = build(&state, id, &request)?;
    let outcome = action::apply(&plan.actions)?;

    // Anything that wrote counts as configuring, which is what separates a repo you set
    // up from one that was merely found.
    if outcome.applied > 0 {
        state.registry(|registry| {
            registry.record(&repo.path, true)?;
            Ok(())
        })?;
    }
    // Tell the other open tabs rather than making them wait for the poller.
    state.announce(repo.path.clone());

    Ok(Json(Applied {
        applied: outcome.applied,
        skipped: outcome.skipped,
        stale: outcome.stale,
    }))
}

/// One place that turns a request into a plan, so preview and commit cannot diverge.
fn build(state: &AppState, id: i64, request: &Request) -> Result<(Plan, Repo)> {
    let catalogue = state.catalogue()?;
    let repo = crate::read::find(state, id)?;
    if !repo.exists() {
        return Err(Error::bad_request(format!(
            "{} is no longer on disk",
            repo.path.display()
        )));
    }
    let mut plan = Plan::default();

    match request {
        Request::Install {
            hooks,
            presets,
            skills,
            harnesses,
            scaffold,
        } => {
            let harnesses = resolve(harnesses, &repo);
            if *scaffold {
                install::knowledge_files(&mut plan, &repo.path, &catalogue, &harnesses)?;
            }
            install::hooks(&mut plan, &repo.path, &catalogue, hooks, &harnesses)?;
            install::mcp(&mut plan, &repo.path, &catalogue, presets, &harnesses)?;
            install::skills(&mut plan, &repo.path, &catalogue, skills, &harnesses, true)?;
        }
        Request::Recommended {
            harnesses,
            scaffold,
        } => {
            let harnesses = resolve(harnesses, &repo);
            let recommendation = ai_toolbox_core::detect::recommend(&repo.path);
            plan = install::everything(
                &repo.path,
                &catalogue,
                &harnesses,
                &recommendation.hooks,
                &recommendation.mcp,
                &recommendation.skills,
                *scaffold,
            )?;
        }
        Request::Repair => {
            let survey = survey(&repo.path, &catalogue)?;
            doctor::repair(
                &mut plan,
                &repo.path,
                &survey.inventory,
                &survey.report,
                &catalogue,
                &survey.findings,
            )?;
        }
        Request::Converge { worktree: target } => {
            let comparison = worktree::compare(&repo.path)?;
            let targets: Vec<PathBuf> = match target {
                Some(one) => vec![one.clone()],
                None => comparison
                    .out_of_step()
                    .iter()
                    .map(|state| state.worktree.path.clone())
                    .collect(),
            };
            for target in targets {
                // Never the reference itself: converging a worktree onto itself is a
                // no-op at best and a request that makes no sense at worst.
                if target == comparison.reference {
                    continue;
                }
                worktree::converge(&mut plan, &comparison.reference, &target)?;
            }
        }
    }
    Ok((plan, repo))
}

/// The harnesses to write for: what was asked, or the ones this repo already uses.
fn resolve(asked: &[Harness], repo: &Repo) -> Vec<Harness> {
    if !asked.is_empty() {
        return asked.to_vec();
    }
    let mut found = ai_toolbox_core::harness::configured(&repo.path);
    if found.is_empty() {
        found = ai_toolbox_core::harness::detect(&repo.path, &ai_toolbox_core::machine::home());
    }
    found
}

fn view(plan: &Plan) -> PlanView {
    PlanView {
        actions: plan
            .actions
            .iter()
            .map(|action| ActionView {
                summary: action.summary.clone(),
                path: action.path.clone(),
                noop: action.noop,
            })
            .collect(),
        changes: plan.changes(),
        warnings: plan.warnings.clone(),
        secrets: plan.secrets.clone(),
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct ScanRequest {
    #[serde(default)]
    pub roots: Vec<PathBuf>,
}

async fn scan(
    State(state): State<AppState>,
    Json(request): Json<ScanRequest>,
) -> Result<Json<ScanResult>> {
    state.registry(|registry| {
        let roots = if request.roots.is_empty() {
            registry.scan_roots()?
        } else {
            request.roots.clone()
        };
        let result = registry::scan(registry, &roots, &ScanOptions::default())?;
        for root in &roots {
            if root.is_dir() {
                registry.add_scan_root(root)?;
            }
        }
        Ok(Json(result))
    })
}

#[derive(Debug, serde::Deserialize)]
pub struct AddRequest {
    pub path: PathBuf,
}

async fn add(State(state): State<AppState>, Json(request): Json<AddRequest>) -> Result<Json<Repo>> {
    if !request.path.is_dir() {
        return Err(Error::bad_request(format!(
            "{} is not a directory",
            request.path.display()
        )));
    }
    // `false`: adding a repo to the list is not configuring it (D9).
    state.registry(|registry| Ok(Json(registry.record(&request.path, false)?)))
}

async fn forget(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<bool>> {
    let repo = crate::read::find(&state, id)?;
    state.registry(|registry| Ok(Json(registry.forget(&repo.path)?)))
}
