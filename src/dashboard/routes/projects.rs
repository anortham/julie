//! Projects page route handlers.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Html;
use tera::Context;

use crate::dashboard::render_template;
use crate::dashboard::AppState;

pub async fn index(State(state): State<AppState>) -> Result<Html<String>, StatusCode> {
    let workspaces = state
        .dashboard
        .daemon_db()
        .and_then(|db| db.list_workspaces().ok())
        .unwrap_or_default();

    let ready_count = workspaces.iter().filter(|w| w.status == "ready").count();
    let indexing_count = workspaces.iter().filter(|w| w.status == "indexing").count();
    let error_count = workspaces.iter().filter(|w| w.status == "error").count();

    let mut context = Context::new();
    context.insert("active_page", "projects");
    context.insert("workspaces", &workspaces);
    context.insert("total_count", &workspaces.len());
    context.insert("ready_count", &ready_count);
    context.insert("indexing_count", &indexing_count);
    context.insert("error_count", &error_count);

    render_template(&state, "projects.html", context).await
}

pub async fn detail(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
) -> Result<Html<String>, StatusCode> {
    let db = state.dashboard.daemon_db().ok_or(StatusCode::NOT_FOUND)?;

    let workspace = db
        .get_workspace(&workspace_id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let references = db.list_references(&workspace_id).unwrap_or_default();
    let health = db.get_latest_snapshot(&workspace_id).ok().flatten();

    let mut context = Context::new();
    context.insert("workspace", &workspace);
    context.insert("references", &references);
    context.insert("health", &health);

    render_template(&state, "partials/project_detail.html", context).await
}
