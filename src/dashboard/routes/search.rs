//! Search page route handlers.

use axum::extract::{Form, State};
use axum::http::StatusCode;
use axum::response::Html;
use serde::Deserialize;
use tera::Context;

use crate::dashboard::AppState;
use crate::dashboard::render_template;
use crate::dashboard::routes::projects_actions::{
    cleanup_dashboard_anchor, dashboard_handler, disconnect_dashboard_attached_workspaces,
};
use crate::tools::search::execution::{self, SearchExecutionWorkspace};
use crate::tools::search::trace::{SearchExecutionKind, SearchExecutionResult, SearchHit};

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
    let limit = params.limit.unwrap_or(20).min(100) as usize;

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

    // Run the actual search against the workspace's Tantivy index
    let execution = run_search(
        &state,
        &query,
        &workspace_id,
        &search_target,
        &language,
        &file_pattern,
        limit,
    )
    .await;

    let no_pool = state.dashboard.workspace_pool().is_none();
    let results = execution
        .as_ref()
        .map(|result| result.hits.clone())
        .unwrap_or_default();
    context.insert("results", &results);
    if let Some(result) = &execution {
        context.insert("search_trace", &result.trace);
        context.insert("strategy_id", &result.trace.strategy_id);
        context.insert("search_relaxed", &result.relaxed);
    }
    context.insert("no_pool", &no_pool);

    // Build centrality ranks (name -> rank 1..=20) for badge display.
    // Only show badges when a specific workspace is selected; cross-workspace
    // badges would be misleading since ranks come from one workspace.
    let centrality_ranks: std::collections::HashMap<String, usize> = if !workspace_id.is_empty()
        && let Some(pool) = state.dashboard.workspace_pool()
    {
        let ws_id = workspace_id.to_string();

        if let Some(ws) = pool.get(&ws_id).await {
            if let Some(db) = &ws.db {
                if let Ok(guard) = db.lock() {
                    guard
                        .get_top_symbols_by_centrality(20)
                        .ok()
                        .map(|syms| {
                            syms.into_iter()
                                .enumerate()
                                .map(|(i, s)| (s.name, i + 1))
                                .collect()
                        })
                        .unwrap_or_default()
                } else {
                    Default::default()
                }
            } else {
                Default::default()
            }
        } else {
            Default::default()
        }
    } else {
        Default::default()
    };
    context.insert("centrality_ranks", &centrality_ranks);

    render_template(&state, "partials/search_results.html", context).await
}

/// Run a search against the workspace pool's Tantivy indexes.
async fn run_search(
    state: &AppState,
    query: &str,
    workspace_id: &str,
    search_target: &str,
    language: &str,
    file_pattern: &str,
    limit: usize,
) -> Option<SearchExecutionResult> {
    if state.dashboard.workspace_pool().is_none() {
        return None;
    }

    let workspaces: Vec<String> = if workspace_id.is_empty() {
        let db = state.dashboard.daemon_db()?;
        db.list_workspaces()
            .ok()?
            .iter()
            .map(|ws| ws.workspace_id.clone())
            .collect()
    } else {
        vec![workspace_id.to_string()]
    };

    if workspaces.is_empty() {
        return None;
    }

    let (handler, _anchor_dir, anchor_id) = match dashboard_handler(state).await {
        Ok(session) => session,
        Err(_) => return None,
    };
    let execution_workspaces = workspaces
        .into_iter()
        .map(SearchExecutionWorkspace::target)
        .collect::<Vec<_>>();
    let language_filter = (!language.is_empty()).then(|| language.to_string());
    let file_pattern_filter = (!file_pattern.is_empty()).then(|| file_pattern.to_string());
    let result = execution::execute_search(
        execution::SearchExecutionParams {
            query,
            language: &language_filter,
            file_pattern: &file_pattern_filter,
            limit: limit as u32,
            search_target,
            context_lines: None,
            exclude_tests: None,
        },
        &execution_workspaces,
        &handler,
    )
    .await
    .ok();

    disconnect_dashboard_attached_workspaces(state, &handler).await;
    cleanup_dashboard_anchor(state, &anchor_id).await;

    result.map(normalize_dashboard_results)
}

fn normalize_dashboard_results(result: SearchExecutionResult) -> SearchExecutionResult {
    let hits = result
        .hits
        .into_iter()
        .map(normalize_dashboard_hit)
        .collect();
    SearchExecutionResult {
        hits,
        relaxed: result.relaxed,
        total_results: result.total_results,
        trace: result.trace,
        kind: match result.kind {
            SearchExecutionKind::Definitions => SearchExecutionKind::Definitions,
            SearchExecutionKind::Content {
                workspace_label,
                file_level,
            } => SearchExecutionKind::Content {
                workspace_label,
                file_level,
            },
        },
    }
}

fn normalize_dashboard_hit(mut hit: SearchHit) -> SearchHit {
    if hit.kind == "line" && hit.line.is_some() && hit.snippet.is_none() {
        hit.snippet = Some(String::new());
    }
    hit
}
