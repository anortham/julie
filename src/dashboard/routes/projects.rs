//! Projects page route handlers.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Html;
use tera::Context;

use crate::dashboard::render_template;
use crate::dashboard::AppState;

pub async fn index(
    State(state): State<AppState>,
) -> Result<Html<String>, StatusCode> {
    let mut context = Context::new();
    context.insert("active_page", "projects");
    render_template(&state, "projects.html", context).await
}

pub async fn detail(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
) -> Result<Html<String>, StatusCode> {
    let mut context = Context::new();
    context.insert("active_page", "projects");
    context.insert("workspace_id", &workspace_id);
    render_template(&state, "projects_detail.html", context).await
}
