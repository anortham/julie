//! Unified cross-content search handler for the REST API.
//!
//! Handles `POST /api/search` when `content_type` is "memory" or "all",
//! routing through `unified_search()` for cross-content RRF merge.

use std::path::PathBuf;
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
fn try_open_memory_index(julie_dir: &PathBuf) -> Option<MemoryIndex> {
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

/// Check if the request needs the memory index (content_type includes memories).
pub fn needs_memory_search(content_type_filter: Option<ContentType>) -> bool {
    match content_type_filter {
        None => true,                                  // "all"
        Some(ContentType::Memory) => true,             // "memory"
        Some(ContentType::Code) | Some(ContentType::Doc) => false,
    }
}

/// Unified search path for "memory" or "all" content types.
///
/// Opens the memory Tantivy index, runs `unified_search()`, and splits
/// results into separate `symbols` and `memories` arrays in the response.
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
    let search_target = body.search_target.clone();

    let result = tokio::task::spawn_blocking(move || {
        let search_index = search_index_arc.lock().map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Search index lock poisoned: {}", e),
            )
        })?;

        // Open memory index (graceful: None if unavailable)
        let memory_index_opt = if needs_memory_search(content_type_filter) {
            try_open_memory_index(&julie_dir)
        } else {
            None
        };

        // If memory-only search but no index exists, return empty results
        let memory_index;
        let memory_index_ref = match &memory_index_opt {
            Some(idx) => idx,
            None if content_type_filter == Some(ContentType::Memory) => {
                return Ok(SearchResponse {
                    search_target: search_target.clone(),
                    relaxed: false,
                    count: 0,
                    symbols: None,
                    content: None,
                    memories: None,
                });
            }
            None => {
                // "all" mode but no memory index — create a temp empty one
                // so unified_search can run (it'll just return code results)
                let tmp_dir = tempfile::tempdir().map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Failed to create temp dir: {}", e),
                    )
                })?;
                memory_index = MemoryIndex::create(tmp_dir.path()).map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Failed to create fallback memory index: {}", e),
                    )
                })?;
                &memory_index
            }
        };

        // Get or create a SymbolDatabase reference for unified_search
        let db_guard;
        let db_ref: Option<std::sync::MutexGuard<'_, crate::database::SymbolDatabase>> =
            if let Some(ref db) = db_arc {
                db_guard = db.lock().map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("DB lock poisoned: {}", e),
                    )
                })?;
                Some(db_guard)
            } else {
                None
            };

        let temp_db;
        let temp_dir_db;
        let db_for_search: &crate::database::SymbolDatabase = match &db_ref {
            Some(guard) => &*guard,
            None => {
                temp_dir_db = tempfile::tempdir().map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Failed to create temp dir: {}", e),
                    )
                })?;
                temp_db = crate::database::SymbolDatabase::new(
                    temp_dir_db.path().join("temp.db"),
                )
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Failed to create temp DB: {}", e),
                    )
                })?;
                &temp_db
            }
        };

        let options = UnifiedSearchOptions {
            content_type: content_type_filter,
            limit,
        };

        let emb_ref = embedding_provider
            .as_ref()
            .map(|p| p.as_ref() as &dyn crate::embeddings::EmbeddingProvider);

        let results = unified_search(
            &query,
            &options,
            &search_index,
            memory_index_ref,
            db_for_search,
            emb_ref,
        )
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Unified search failed: {}", e),
            )
        })?;

        // Split results into symbols and memories
        let mut symbol_results = Vec::new();
        let mut memory_results = Vec::new();

        for tagged in &results {
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

        Ok(SearchResponse {
            search_target: search_target.clone(),
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
