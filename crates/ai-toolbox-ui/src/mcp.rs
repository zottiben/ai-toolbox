//! MCP inspection and retained previews share the exact CLI engine. Apply takes an
//! opaque preview id, never a recomputed request body or a client-supplied file path.
use crate::{AppState, Result};
use ai_toolbox_core::mcp::{self, Applied, Options, Preview, Report};
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/projects/{id}/mcp", get(report))
        .route("/projects/{id}/mcp/preview", post(preview))
        .route("/mcp/apply/{id}", post(apply))
        .route("/mcp/rollback/{id}", post(rollback))
}
#[derive(serde::Deserialize)]
struct Request {
    #[serde(flatten)]
    options: Options,
}
async fn report(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<Report>> {
    let repo = crate::read::find(&state, id)?;
    let context = state.mcp_for_project(&repo.path)?;
    Ok(Json(mcp::inspect(&context, &repo.path)?))
}
async fn preview(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(request): Json<Request>,
) -> Result<Json<Preview>> {
    let repo = crate::read::find(&state, id)?;
    let registered = state.registry(|r| {
        Ok(r.all()?
            .into_iter()
            .filter(|r| r.exists())
            .map(|r| r.path)
            .collect::<Vec<_>>())
    })?;
    let context = state.mcp_for_project(&repo.path)?;
    Ok(Json(mcp::preview(
        &context,
        &repo.path,
        &registered,
        request.options,
    )?))
}
async fn apply(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Applied>> {
    Ok(Json(mcp::apply(&state.mcp_context, &id)?))
}
async fn rollback(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Applied>> {
    Ok(Json(mcp::rollback(&state.mcp_context, &id)?))
}
