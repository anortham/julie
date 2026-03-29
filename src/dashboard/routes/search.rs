//! Search page route handlers.

use axum::extract::{Form, State};
use axum::http::StatusCode;
use axum::response::Html;
use serde::Deserialize;
use tera::Context;

use crate::dashboard::render_template;
use crate::dashboard::AppState;
use crate::search::index::SearchFilter;

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
    let results = run_search(
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
    context.insert("results", &results);
    context.insert("no_pool", &no_pool);

    // Build centrality ranks (name -> rank 1..=20) for badge display
    let centrality_ranks: std::collections::HashMap<String, usize> =
        if let Some(pool) = state.dashboard.workspace_pool() {
            let ws_id = if workspace_id.is_empty() {
                state
                    .dashboard
                    .daemon_db()
                    .and_then(|db| db.list_workspaces().ok())
                    .and_then(|wss| wss.first().map(|w| w.workspace_id.clone()))
                    .unwrap_or_default()
            } else {
                workspace_id.to_string()
            };

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
) -> Vec<serde_json::Value> {
    let pool = match state.dashboard.workspace_pool() {
        Some(pool) => pool,
        None => return vec![],
    };

    // Determine which workspaces to search
    let ws_ids: Vec<String> = if workspace_id.is_empty() {
        // Search all loaded workspaces
        let db = match state.dashboard.daemon_db() {
            Some(db) => db,
            None => return vec![],
        };
        db.list_workspaces()
            .unwrap_or_default()
            .iter()
            .map(|ws| ws.workspace_id.clone())
            .collect()
    } else {
        vec![workspace_id.to_string()]
    };

    let filter = SearchFilter {
        language: if language.is_empty() {
            None
        } else {
            Some(language.to_string())
        },
        file_pattern: if file_pattern.is_empty() {
            None
        } else {
            Some(file_pattern.to_string())
        },
        ..Default::default()
    };

    let mut all_results = Vec::new();

    for ws_id in &ws_ids {
        let ws = match pool.get(ws_id).await {
            Some(ws) => ws,
            None => continue,
        };

        let search_idx = match &ws.search_index {
            Some(idx) => idx.clone(),
            None => continue,
        };

        let query_str = query.to_string();
        let filter_clone = filter.clone();
        let target = search_target.to_string();
        let ws_id_clone = ws_id.clone();

        // Run search on blocking thread (Tantivy uses synchronous I/O)
        let results = tokio::task::spawn_blocking(move || {
            let idx = search_idx.lock().unwrap_or_else(|p| p.into_inner());
            if target == "content" {
                // Content search returns file-level matches
                match idx.search_content(&query_str, &filter_clone, limit) {
                    Ok(results) => results
                        .results
                        .into_iter()
                        .map(|r| {
                            let filename = r.file_path.split('/').last().unwrap_or(&r.file_path).to_string();
                            serde_json::json!({
                                "name": filename,
                                "file": r.file_path,
                                "kind": "file",
                                "language": r.language,
                                "score": r.score,
                                "workspace": ws_id_clone,
                            })
                        })
                        .collect::<Vec<_>>(),
                    Err(_) => vec![],
                }
            } else {
                // Symbol/definition search
                match idx.search_symbols(&query_str, &filter_clone, limit) {
                    Ok(results) => results
                        .results
                        .into_iter()
                        .map(|r| {
                            serde_json::json!({
                                "name": r.name,
                                "file": r.file_path,
                                "line": r.start_line,
                                "kind": r.kind,
                                "language": r.language,
                                "score": r.score,
                                "snippet": if r.signature.is_empty() { r.doc_comment } else { r.signature },
                                "workspace": ws_id_clone,
                            })
                        })
                        .collect::<Vec<_>>(),
                    Err(_) => vec![],
                }
            }
        })
        .await
        .unwrap_or_default();

        all_results.extend(results);
    }

    // Sort by score descending across all workspaces
    all_results.sort_by(|a, b| {
        let sa = a["score"].as_f64().unwrap_or(0.0);
        let sb = b["score"].as_f64().unwrap_or(0.0);
        sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
    });

    all_results.truncate(limit);
    all_results
}
