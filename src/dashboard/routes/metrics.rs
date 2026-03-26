//! Metrics page route handlers.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Html;
use tera::Context;

use crate::dashboard::render_template;
use crate::dashboard::AppState;

pub async fn index(
    State(state): State<AppState>,
) -> Result<Html<String>, StatusCode> {
    let mut context = Context::new();
    context.insert("active_page", "metrics");
    render_template(&state, "metrics.html", context).await
}

pub async fn table(
    State(state): State<AppState>,
) -> Result<Html<String>, StatusCode> {
    let mut context = Context::new();
    context.insert("active_page", "metrics");
    render_template(&state, "metrics_table.html", context).await
}
