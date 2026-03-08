//! Search endpoints for the Julie daemon HTTP server.
//!
//! - `POST /api/search` — standard search (definitions or content), with optional
//!   cross-content filtering via `content_type` (code/memory/all)
//! - `POST /api/search/debug` — search with scoring breakdown

use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::api::common::{MAX_RESULT_LIMIT, resolve_workspace};
use crate::api::search_unified;
use crate::search::content_type::ContentType;
use crate::search::debug::{ContentDebugResults, SymbolDebugResults, search_content_debug};
use crate::search::index::SearchFilter;
use crate::server::AppState;

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

/// Request body for `POST /api/search` and `POST /api/search/debug`.
#[derive(Debug, Deserialize)]
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
    /// Content type filter: "code" (default), "memory", or "all".
    /// When set to "memory" or "all", searches the memory index alongside code.
    #[serde(default)]
    pub content_type: Option<String>,
    /// When true, forces hybrid (BM25 + semantic) search for memories,
    /// even for queries that look like code identifiers.
    #[serde(default)]
    pub hybrid: Option<bool>,
}

fn default_limit() -> usize {
    20
}

fn default_search_target() -> String {
    "definitions".to_string()
}

/// Parse the `content_type` request field into a `ContentType` filter.
/// Returns `None` for "all" (meaning: search all content types).
/// Returns `Some(ContentType::Code)` when no content_type is specified (default).
pub fn parse_content_type(content_type: &Option<String>) -> Option<ContentType> {
    match content_type.as_deref() {
        None | Some("code") => Some(ContentType::Code),
        Some("all") => None,
        Some(s) => ContentType::from_str_loose(s),
    }
}

// -- Standard search response -----------------------------------------------

/// A single symbol result in the standard search response.
#[derive(Debug, Serialize)]
pub struct SymbolResultResponse {
    pub content_type: String,
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
#[derive(Debug, Serialize)]
pub struct ContentResultResponse {
    pub file_path: String,
    pub language: String,
    pub score: f32,
}

/// A single memory result in the standard search response.
#[derive(Debug, Serialize)]
pub struct MemoryResultResponse {
    pub content_type: String,
    pub id: String,
    pub body: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub tags: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub symbols: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub decision: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub impact: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub branch: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub timestamp: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub file_path: String,
    pub score: f32,
}

/// Response body for `POST /api/search`.
#[derive(Debug, Serialize)]
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
    /// Memory results (present when content_type includes memories)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memories: Option<Vec<MemoryResultResponse>>,
}

/// Response body for `POST /api/search/debug`.
#[derive(Debug, Serialize)]
pub struct DebugSearchResponse {
    pub search_target: String,
    pub relaxed: bool,
    pub count: usize,
    pub query_tokens: Vec<String>,
    /// Whether hybrid (BM25 + semantic) memory search was requested.
    pub hybrid_mode: bool,
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
///
/// Supports cross-content search via the `content_type` parameter:
/// - `"code"` (default): search code symbols only (backward compatible)
/// - `"memory"`: search memories only
/// - `"all"`: search both code and memories, merged via RRF
pub async fn search(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SearchRequest>,
) -> Result<Json<SearchResponse>, (StatusCode, String)> {
    let content_type_filter = parse_content_type(&body.content_type);

    // Code-only: use the fast path (no unified search overhead)
    if content_type_filter == Some(ContentType::Code) {
        return search_code_only(state, body).await;
    }

    // Unified search path: content_type is "memory" or "all"
    search_unified::search_unified(state, body, content_type_filter).await
}

/// Fast path for code-only search (no unified search overhead).
/// This preserves the original behavior for backward compatibility.
async fn search_code_only(
    state: Arc<AppState>,
    body: SearchRequest,
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
                memories: None,
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
                            content_type: "code".to_string(),
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
                memories: None,
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
pub async fn search_debug(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SearchRequest>,
) -> Result<Json<DebugSearchResponse>, (StatusCode, String)> {
    let filter = SearchFilter {
        language: body.language.clone(),
        file_pattern: body.file_pattern.clone(),
        ..Default::default()
    };

    let hybrid_mode = body.hybrid.unwrap_or(false);

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
                hybrid_mode,
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
                hybrid_mode,
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
