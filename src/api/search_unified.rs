//! Unified cross-content search handler for the REST API.
//!
//! Handles `POST /api/search` when `content_type` is "memory" or "all",
//! routing through `unified_search()` for cross-content RRF merge.

use std::path::Path;
use std::sync::Arc;

use axum::http::StatusCode;
use axum::Json;
use tracing::warn;

use crate::api::common::{MAX_RESULT_LIMIT, resolve_workspace};
use crate::api::search::{
    MemoryResultResponse, SearchRequest, SearchResponse, SymbolResultResponse,
};
use crate::memory::index::MemoryIndex;
use crate::search::content_type::ContentType;
use crate::search::unified::{SearchResultItem, UnifiedSearchOptions, unified_search};
use crate::server::AppState;

/// Relative path from `julie_dir` to the memory Tantivy index.
const MEMORY_INDEX_REL: &str = "indexes/memories/tantivy";

/// Try to open the memory index for a workspace. Returns `None` if the index
/// directory doesn't exist or can't be opened (graceful degradation).
fn try_open_memory_index(julie_dir: &Path) -> Option<MemoryIndex> {
    let index_path = julie_dir.join(MEMORY_INDEX_REL);
    if !index_path.exists() {
        return None;
    }
    match MemoryIndex::open_or_create(&index_path) {
        Ok(idx) => Some(idx),
        Err(e) => {
            warn!("Failed to open memory index at {}: {e}", index_path.display());
            None
        }
    }
}

/// Convert memory search results to API response format.
fn memory_results_to_response(
    results: Vec<crate::memory::MemorySearchResult>,
) -> Vec<MemoryResultResponse> {
    results
        .into_iter()
        .map(|r| MemoryResultResponse {
            content_type: "memory".to_string(),
            id: r.id,
            body: r.body,
            tags: r.tags,
            symbols: r.symbols,
            decision: r.decision,
            impact: r.impact,
            branch: r.branch,
            timestamp: r.timestamp,
            file_path: r.file_path,
            score: r.score,
        })
        .collect()
}

/// Unified search path for "memory" or "all" content types.
///
/// Opens the memory Tantivy index, runs `unified_search()`, and splits
/// results into separate `symbols` and `memories` arrays in the response.
///
/// Note: The unified path always searches symbol definitions (not content),
/// so `search_target` in the response is forced to `"definitions"` regardless
/// of the request value.
pub async fn search_unified(
    state: Arc<AppState>,
    body: SearchRequest,
    content_type_filter: Option<ContentType>,
) -> Result<Json<SearchResponse>, (StatusCode, String)> {
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

    let db_arc = loaded_ws.workspace.db.clone();
    let julie_dir = loaded_ws.workspace.julie_dir.clone();
    let embedding_provider = loaded_ws.workspace.embedding_provider.clone();

    drop(daemon_state);

    let query = body.query.clone();
    let limit = body.limit.min(MAX_RESULT_LIMIT);

    let result = tokio::task::spawn_blocking(move || {
        let search_index = search_index_arc.lock().map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Search index lock poisoned: {}", e),
            )
        })?;

        // Open memory index (graceful: None if unavailable)
        let memory_index_opt = try_open_memory_index(&julie_dir);

        // If memory-only search but no index on disk: return empty
        if memory_index_opt.is_none() && content_type_filter == Some(ContentType::Memory) {
            return Ok(SearchResponse {
                search_target: "definitions".to_string(),
                relaxed: false,
                count: 0,
                symbols: None,
                content: None,
                memories: None,
            });
        }

        // If "all" mode but no memory index: fall back to code-only search
        if memory_index_opt.is_none() && content_type_filter.is_none() {
            let results = search_index
                .search_symbols(&query, &Default::default(), limit)
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Search failed: {}", e),
                    )
                })?;
            let count = results.results.len();
            return Ok(SearchResponse {
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
            });
        }

        // We have a memory index. Try to use unified_search if we have a DB.
        let mem_idx = memory_index_opt
            .as_ref()
            .expect("memory index should be Some here");

        let db_guard = match &db_arc {
            Some(db) => Some(db.lock().map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("DB lock poisoned: {}", e),
                )
            })?),
            None => None,
        };

        // If we have a DB, use unified_search for full RRF merge
        if let Some(ref guard) = db_guard {
            let options = UnifiedSearchOptions {
                content_type: content_type_filter,
                limit,
            };

            let emb_ref = embedding_provider
                .as_ref()
                .map(|p| p.as_ref() as &dyn crate::embeddings::EmbeddingProvider);

            let results = unified_search(
                &query, &options, &search_index, mem_idx, guard, emb_ref,
            )
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Unified search failed: {}", e),
                )
            })?;

            return Ok(build_split_response(&results));
        }

        // No DB available: fall back to direct BM25 search on each index.
        // This skips embedding-based hybrid search but still returns results.
        warn!("No database available for unified search; falling back to BM25-only");

        let search_code = content_type_filter.map_or(true, |ct| ct == ContentType::Code);
        let search_memory =
            content_type_filter.map_or(true, |ct| ct == ContentType::Memory);

        let symbol_results = if search_code {
            let results = search_index
                .search_symbols(&query, &Default::default(), limit)
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Search failed: {}", e),
                    )
                })?;
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
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        let memory_results = if search_memory {
            let results = mem_idx.search(&query, limit).map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Memory search failed: {}", e),
                )
            })?;
            memory_results_to_response(results)
        } else {
            Vec::new()
        };

        let count = symbol_results.len() + memory_results.len();

        Ok(SearchResponse {
            search_target: "definitions".to_string(),
            relaxed: false,
            count,
            symbols: if symbol_results.is_empty() {
                None
            } else {
                Some(symbol_results)
            },
            content: None,
            memories: if memory_results.is_empty() {
                None
            } else {
                Some(memory_results)
            },
        })
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

/// Split unified search results into separate symbols and memories arrays.
fn build_split_response(
    results: &[crate::search::unified::UnifiedSearchResult],
) -> SearchResponse {
    let mut symbol_results = Vec::new();
    let mut memory_results = Vec::new();

    for tagged in results {
        match &tagged.result {
            SearchResultItem::Code(r) => {
                symbol_results.push(SymbolResultResponse {
                    content_type: "code".to_string(),
                    id: r.id.clone(),
                    name: r.name.clone(),
                    signature: r.signature.clone(),
                    doc_comment: r.doc_comment.clone(),
                    file_path: r.file_path.clone(),
                    kind: r.kind.clone(),
                    language: r.language.clone(),
                    start_line: r.start_line,
                    score: tagged.score,
                });
            }
            SearchResultItem::Memory(r) => {
                memory_results.push(MemoryResultResponse {
                    content_type: "memory".to_string(),
                    id: r.id.clone(),
                    body: r.body.clone(),
                    tags: r.tags.clone(),
                    symbols: r.symbols.clone(),
                    decision: r.decision.clone(),
                    impact: r.impact.clone(),
                    branch: r.branch.clone(),
                    timestamp: r.timestamp.clone(),
                    file_path: r.file_path.clone(),
                    score: tagged.score,
                });
            }
        }
    }

    let count = symbol_results.len() + memory_results.len();

    SearchResponse {
        search_target: "definitions".to_string(),
        relaxed: false,
        count,
        symbols: if symbol_results.is_empty() {
            None
        } else {
            Some(symbol_results)
        },
        content: None,
        memories: if memory_results.is_empty() {
            None
        } else {
            Some(memory_results)
        },
    }
}
