//! Search endpoints for the Julie daemon HTTP server.
//!
//! - `POST /api/search` — standard search (definitions or content)
//! - `POST /api/search/debug` — search with scoring breakdown

use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::api::common::{MAX_RESULT_LIMIT, resolve_workspace};
use crate::search::debug::{ContentDebugResults, SymbolDebugResults, search_content_debug};
use crate::search::index::SearchFilter;
use crate::server::AppState;

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

/// Request body for `POST /api/search` and `POST /api/search/debug`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SearchRequest {
    /// The search query string.
    pub query: String,
    /// Optional language filter (e.g. "rust", "typescript").
    #[serde(default)]
    pub language: Option<String>,
    /// Optional file pattern filter (glob syntax, e.g. "src/**/*.rs").
    #[serde(default)]
    pub file_pattern: Option<String>,
    /// Maximum number of results (default: 20, max: 500).
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Search target: "definitions" (default) or "content".
    #[serde(default = "default_search_target")]
    pub search_target: String,
    /// Workspace ID to search. If omitted, uses the first Ready workspace.
    #[serde(default)]
    pub project: Option<String>,
}

fn default_limit() -> usize {
    20
}

fn default_search_target() -> String {
    "definitions".to_string()
}

// -- Standard search response -----------------------------------------------

/// A single symbol result in the standard search response.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SymbolResultResponse {
    pub id: String,
    pub name: String,
    pub signature: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub doc_comment: String,
    pub file_path: String,
    pub kind: String,
    pub language: String,
    pub start_line: u32,
    pub score: f32,
}

/// A single content result in the standard search response.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ContentResultResponse {
    pub file_path: String,
    pub language: String,
    pub score: f32,
}

/// Response body for `POST /api/search`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SearchResponse {
    /// "definitions" or "content"
    pub search_target: String,
    /// True if AND->OR fallback was used
    pub relaxed: bool,
    /// Number of results
    pub count: usize,
    /// Symbol results (present when search_target = "definitions")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbols: Option<Vec<SymbolResultResponse>>,
    /// Content results (present when search_target = "content")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Vec<ContentResultResponse>>,
}

/// Response body for `POST /api/search/debug`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DebugSearchResponse {
    pub search_target: String,
    pub relaxed: bool,
    pub count: usize,
    pub query_tokens: Vec<String>,
    /// Symbol debug results (present when search_target = "definitions")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbols: Option<SymbolDebugResults>,
    /// Content debug results (present when search_target = "content")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<ContentDebugResults>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `POST /api/search` — run a standard search.
#[utoipa::path(
    post,
    path = "/api/search",
    tag = "search",
    request_body = SearchRequest,
    responses(
        (status = 200, description = "Search results", body = SearchResponse)
    )
)]
pub async fn search(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SearchRequest>,
) -> Result<Json<SearchResponse>, (StatusCode, String)> {
    let filter = SearchFilter {
        language: body.language.clone(),
        file_pattern: body.file_pattern.clone(),
        ..Default::default()
    };

    let daemon_state = state.daemon_state.read().await;
    let loaded_ws = resolve_workspace(&daemon_state, body.project.as_deref())?;
    let search_index_arc = loaded_ws
        .workspace
        .search_index
        .as_ref()
        .ok_or((
            StatusCode::SERVICE_UNAVAILABLE,
            "Workspace search index not available".to_string(),
        ))?
        .clone();

    drop(daemon_state);

    let query = body.query.clone();
    let limit = body.limit.min(MAX_RESULT_LIMIT);
    let search_target = body.search_target.clone();

    let result = tokio::task::spawn_blocking(move || {
        let index = search_index_arc.lock().map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Search index lock poisoned: {}", e),
            )
        })?;

        if search_target == "content" {
            let results = index.search_content(&query, &filter, limit).map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Search failed: {}", e),
                )
            })?;
            let count = results.results.len();
            Ok(SearchResponse {
                search_target: "content".to_string(),
                relaxed: results.relaxed,
                count,
                symbols: None,
                content: Some(
                    results
                        .results
                        .into_iter()
                        .map(|r| ContentResultResponse {
                            file_path: r.file_path,
                            language: r.language,
                            score: r.score,
                        })
                        .collect(),
                ),
            })
        } else {
            let results =
                index
                    .search_symbols(&query, &filter, limit)
                    .map_err(|e| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("Search failed: {}", e),
                        )
                    })?;
            let count = results.results.len();
            Ok(SearchResponse {
                search_target: "definitions".to_string(),
                relaxed: results.relaxed,
                count,
                symbols: Some(
                    results
                        .results
                        .into_iter()
                        .map(|r| SymbolResultResponse {
                            id: r.id,
                            name: r.name,
                            signature: r.signature,
                            doc_comment: r.doc_comment,
                            file_path: r.file_path,
                            kind: r.kind,
                            language: r.language,
                            start_line: r.start_line,
                            score: r.score,
                        })
                        .collect(),
                ),
                content: None,
            })
        }
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Search task panicked: {}", e),
        )
    })??;

    Ok(Json(result))
}

/// `POST /api/search/debug` — run a search with scoring breakdown.
#[utoipa::path(
    post,
    path = "/api/search/debug",
    tag = "search",
    request_body = SearchRequest,
    responses(
        (status = 200, description = "Search results with debug scoring breakdown", body = DebugSearchResponse)
    )
)]
pub async fn search_debug(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SearchRequest>,
) -> Result<Json<DebugSearchResponse>, (StatusCode, String)> {
    let filter = SearchFilter {
        language: body.language.clone(),
        file_pattern: body.file_pattern.clone(),
        ..Default::default()
    };

    let daemon_state = state.daemon_state.read().await;
    let loaded_ws = resolve_workspace(&daemon_state, body.project.as_deref())?;
    let search_index_arc = loaded_ws
        .workspace
        .search_index
        .as_ref()
        .ok_or((
            StatusCode::SERVICE_UNAVAILABLE,
            "Workspace search index not available".to_string(),
        ))?
        .clone();

    // Clone the DB Arc for reference score lookup inside spawn_blocking
    let db_arc = loaded_ws.workspace.db.clone();

    // Drop daemon_state lock before blocking search
    drop(daemon_state);

    let query = body.query.clone();
    let limit = body.limit.min(MAX_RESULT_LIMIT);
    let search_target = body.search_target.clone();

    let result = tokio::task::spawn_blocking(move || {
        let index = search_index_arc.lock().map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Search index lock poisoned: {}", e),
            )
        })?;

        if search_target == "content" {
            let results =
                search_content_debug(&index, &query, &filter, limit).map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Debug search failed: {}", e),
                    )
                })?;
            let count = results.results.len();
            let query_tokens = results.query_tokens.clone();
            Ok(DebugSearchResponse {
                search_target: "content".to_string(),
                relaxed: results.relaxed,
                count,
                query_tokens,
                symbols: None,
                content: Some(results),
            })
        } else {
            // First run the normal search to get result IDs
            let search_results =
                index
                    .search_symbols(&query, &filter, limit)
                    .map_err(|e| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("Search failed: {}", e),
                        )
                    })?;

            // Look up reference scores for the result IDs
            let reference_scores: HashMap<String, f64> = if let Some(ref db) = db_arc {
                let db_guard = db.lock().map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("DB lock poisoned: {}", e),
                    )
                })?;
                let ids: Vec<&str> =
                    search_results.results.iter().map(|r| r.id.as_str()).collect();
                db_guard.get_reference_scores(&ids).unwrap_or_default()
            } else {
                HashMap::new()
            };

            // Now build debug results using the enrichment functions
            let query_tokens = index.tokenize_query_public(&query);
            let relaxed = search_results.relaxed;
            let total_candidates = search_results.results.len();

            let debug_results: Vec<crate::search::debug::SymbolDebugResult> = search_results
                .results
                .into_iter()
                .map(|r| {
                    crate::search::debug::enrich_symbol_result(
                        r,
                        &query,
                        &reference_scores,
                        &query_tokens,
                        relaxed,
                    )
                })
                .collect();

            let count = debug_results.len();
            let qt = query_tokens.clone();

            Ok(DebugSearchResponse {
                search_target: "definitions".to_string(),
                relaxed,
                count,
                query_tokens: qt,
                symbols: Some(SymbolDebugResults {
                    results: debug_results,
                    relaxed,
                    query_tokens,
                    total_candidates,
                }),
                content: None,
            })
        }
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Debug search task panicked: {}", e),
        )
    })??;

    Ok(Json(result))
}
