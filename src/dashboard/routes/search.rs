//! Search page route handlers.

use axum::extract::{Form, State};
use axum::http::StatusCode;
use axum::response::Html;
use serde::Deserialize;
use tera::Context;

use crate::dashboard::render_template;
use crate::dashboard::AppState;

#[derive(Deserialize)]
pub struct SearchParams {
    pub query: Option<String>,
    pub workspace: Option<String>,
    pub search_target: Option<String>,
    pub language: Option<String>,
    pub file_pattern: Option<String>,
    pub debug: Option<String>, // checkbox sends "true" or absent
    pub limit: Option<u32>,
}

pub async fn index(State(state): State<AppState>) -> Result<Html<String>, StatusCode> {
    let workspaces = state
        .dashboard
        .daemon_db()
        .and_then(|db| db.list_workspaces().ok())
        .unwrap_or_default();

    let mut context = Context::new();
    context.insert("active_page", "search");
    context.insert("workspaces", &workspaces);
    context.insert("searched", &false);
    context.insert("results", &Vec::<serde_json::Value>::new());
    context.insert("query", &"");
    context.insert("selected_workspace", &"");
    context.insert("search_target", &"definitions");
    context.insert("language", &"");
    context.insert("file_pattern", &"");
    context.insert("debug", &false);

    render_template(&state, "search.html", context).await
}

pub async fn search(
    State(state): State<AppState>,
    Form(params): Form<SearchParams>,
) -> Result<Html<String>, StatusCode> {
    let query = params.query.unwrap_or_default();
    if query.is_empty() {
        return index(State(state)).await;
    }

    let workspaces = state
        .dashboard
        .daemon_db()
        .and_then(|db| db.list_workspaces().ok())
        .unwrap_or_default();

    let workspace_id = params.workspace.unwrap_or_default();
    let search_target = params
        .search_target
        .unwrap_or_else(|| "definitions".to_string());
    let language = params.language.unwrap_or_default();
    let file_pattern = params.file_pattern.unwrap_or_default();
    let debug = params.debug.as_deref() == Some("true");

    let mut context = Context::new();
    context.insert("active_page", "search");
    context.insert("workspaces", &workspaces);
    context.insert("searched", &true);
    context.insert("query", &query);
    context.insert("selected_workspace", &workspace_id);
    context.insert("search_target", &search_target);
    context.insert("language", &language);
    context.insert("file_pattern", &file_pattern);
    context.insert("debug", &debug);

    // Search results placeholder — actual search pipeline integration is a follow-up.
    // For now, render with empty results. The template handles the "no results" state.
    context.insert("results", &Vec::<serde_json::Value>::new());

    render_template(&state, "partials/search_results.html", context).await
}
