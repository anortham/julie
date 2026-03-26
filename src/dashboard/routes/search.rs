//! Search page route handlers.

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
    context.insert("active_page", "search");
    render_template(&state, "search.html", context).await
}

pub async fn search(
    State(state): State<AppState>,
) -> Result<Html<String>, StatusCode> {
    let mut context = Context::new();
    context.insert("active_page", "search");
    render_template(&state, "search_results.html", context).await
}
