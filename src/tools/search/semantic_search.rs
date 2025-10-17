//! Semantic search using HNSW embeddings
//!
//! Provides semantic similarity search powered by ONNX embeddings and HNSW indexing.
//! Falls back to text search if embeddings are unavailable.

use anyhow::Result;
use tracing::{debug, warn};

use crate::extractors::Symbol;
use crate::handler::JulieServerHandler;

use super::query::matches_glob_pattern;

/// Perform semantic search using HNSW embeddings
///
/// Returns semantically similar symbols based on the query embedding.
/// Falls back gracefully to text search if vector store is unavailable.
pub async fn semantic_search_impl(
    query: &str,
    language: &Option<String>,
    file_pattern: &Option<String>,
    limit: u32,
    handler: &JulieServerHandler,
) -> Result<Vec<Symbol>> {
    debug!("🧠 Semantic search mode (using HNSW index)");

    // Ensure vector store is initialized first (lazy-loads from disk or rebuilds)
    handler.ensure_vector_store().await?;

    // PERF FIX: Get workspace once after initialization instead of twice
    // Previous code called get_workspace() before and after ensure_vector_store()
    let workspace = handler
        .get_workspace()
        .await?
        .ok_or_else(|| anyhow::anyhow!("No workspace initialized for semantic search"))?;

    let db = workspace
        .db
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No database available for semantic search"))?;

    // Check if vector_store is ready
    let vector_store = match workspace.vector_store.as_ref() {
        Some(vs) => vs,
        None => {
            warn!("Vector store initialization failed - falling back to text search");
            return crate::tools::search::text_search::text_search_impl(
                query, language, file_pattern, limit, None, handler,
            )
            .await;
        }
    };

    // Ensure embedding engine is initialized for query embedding
    handler.ensure_embedding_engine().await?;

    // Generate embedding for the query
    let query_embedding = {
        let mut embedding_guard = handler.embedding_engine.write().await;
        let embedding_engine = match embedding_guard.as_mut() {
            Some(engine) => engine,
            None => {
                warn!("Embedding engine not available - falling back to text search");
                return crate::tools::search::text_search::text_search_impl(
                    query, language, file_pattern, limit, None, handler,
                )
                .await;
            }
        };

        // Create a temporary symbol from the query for embedding
        let query_symbol = Symbol {
            id: "query".to_string(),
            name: query.to_string(),
            kind: crate::extractors::base::SymbolKind::Function,
            language: "query".to_string(),
            file_path: "query".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column: query.len() as u32,
            start_byte: 0,
            end_byte: query.len() as u32,
            signature: None,
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        };

        let context = crate::embeddings::CodeContext {
            parent_symbol: None,
            surrounding_code: None,
            file_context: Some("".to_string()),
        };

        embedding_engine.embed_symbol(&query_symbol, &context)?
    };

    // Use HNSW index for fast similarity search
    // Search for more results than needed to allow filtering
    let search_limit = (limit * 5).min(200) as usize;
    let similarity_threshold = 0.3; // Minimum similarity score

    let store_guard = vector_store.read().await;
    if store_guard.is_empty() {
        debug!("⚠️ Semantic vector store empty - falling back to text search");
        return crate::tools::search::text_search::text_search_impl(
            query, language, file_pattern, limit, None, handler,
        )
        .await;
    }

    let (semantic_results, used_hnsw) = match store_guard
        .search_with_fallback(&query_embedding, search_limit, similarity_threshold)
    {
        Ok(results) => results,
        Err(e) => {
            warn!("Semantic similarity search failed: {} - falling back to text search", e);
            return crate::tools::search::text_search::text_search_impl(
                query, language, file_pattern, limit, None, handler,
            )
            .await;
        }
    };
    drop(store_guard);

    if used_hnsw {
        debug!(
            "🔍 HNSW search returned {} candidates (threshold: {})",
            semantic_results.len(),
            similarity_threshold
        );
    } else {
        debug!(
            "⚠️ Using brute-force semantic search ({} candidates, threshold: {})",
            semantic_results.len(),
            similarity_threshold
        );
    }

    // Extract symbol IDs from similarity results
    let symbol_ids: Vec<String> = semantic_results
        .iter()
        .map(|result| result.symbol_id.clone())
        .collect();

    if symbol_ids.is_empty() {
        debug!("No similar symbols found by HNSW search");
        return Ok(Vec::new());
    }

    // Fetch actual symbols from database (batched query for efficiency)
    // CRITICAL FIX: Wrap blocking rusqlite call in block_in_place
    // rusqlite operations are synchronous blocking I/O that can block Tokio runtime
    let symbols = tokio::task::block_in_place(|| {
        let db_lock = db.lock().unwrap();
        db_lock.get_symbols_by_ids(&symbol_ids)
    })?;

    // Apply filters (language, file_pattern)
    let filtered_symbols: Vec<Symbol> = symbols
        .into_iter()
        .filter(|symbol| {
            // Apply language filter if specified
            let language_match = language
                .as_ref()
                .map(|lang| symbol.language.eq_ignore_ascii_case(lang))
                .unwrap_or(true);

            // CRITICAL FIX: Use proper glob matching instead of contains()
            // This now correctly handles patterns like "src/**/*.rs", "!**/target/*", etc.
            let file_match = file_pattern
                .as_ref()
                .map(|pattern| matches_glob_pattern(&symbol.file_path, pattern))
                .unwrap_or(true);

            language_match && file_match
        })
        .collect();

    // Limit to requested number of results
    let results: Vec<Symbol> = filtered_symbols
        .into_iter()
        .take(limit as usize)
        .collect();

    debug!(
        "✅ Semantic search returned {} results (HNSW-powered)",
        results.len()
    );
    Ok(results)
}
