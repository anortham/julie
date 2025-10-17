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
    workspace_ids: Option<Vec<String>>,
    handler: &JulieServerHandler,
) -> Result<Vec<Symbol>> {
    debug!("🧠 Semantic search mode (using HNSW index)");

    // Ensure vector store is initialized first (lazy-loads from disk or rebuilds)
    handler.ensure_vector_store().await?;

    // Get the primary workspace (needed for paths even when searching reference workspaces)
    let primary_workspace = handler
        .get_workspace()
        .await?
        .ok_or_else(|| anyhow::anyhow!("No workspace initialized for semantic search"))?;

    // WORKSPACE FILTERING: Load database and vectors for the target workspace
    // For reference workspaces, their indexes are stored in the primary workspace's
    // .julie/indexes/{workspace_id}/ directory
    let (db, vector_store, target_workspace_id) = if let Some(ref ids) = workspace_ids {
        if ids.len() == 1 {
            let workspace_id = &ids[0];
            debug!("🔍 Semantic search targeting workspace: {}", workspace_id);

            // Verify workspace exists in registry
            use crate::workspace::registry_service::WorkspaceRegistryService;
            let registry_service = WorkspaceRegistryService::new(primary_workspace.root.clone());

            if registry_service.get_workspace(workspace_id).await?.is_none() {
                return Err(anyhow::anyhow!("Workspace '{}' not found in registry", workspace_id));
            }

            // Load the specific workspace's database from primary workspace's indexes
            let db_path = primary_workspace.workspace_db_path(workspace_id);
            debug!("📂 Loading database from: {}", db_path.display());
            let db = std::sync::Arc::new(std::sync::Mutex::new(
                crate::database::SymbolDatabase::new(&db_path)?
            ));

            // Load the specific workspace's vector store from primary workspace's indexes
            // This replicates the logic from JulieWorkspace::initialize_vector_store()
            let vectors_dir = primary_workspace.workspace_vectors_path(workspace_id);
            debug!("🧠 Loading vector store from: {}", vectors_dir.display());

            let vector_store = if vectors_dir.exists() {
                // Create a new vector store (384 dimensions for BGE-Small model)
                let mut store = crate::embeddings::vector_store::VectorStore::new(384)?;

                // Load embeddings from database
                let embeddings_result = {
                    let db_lock = db.lock().map_err(|_| {
                        anyhow::anyhow!("Could not acquire database lock for reference workspace")
                    })?;
                    let model_name = "bge-small"; // Match the embedding model
                    db_lock.load_all_embeddings(model_name)
                };

                match embeddings_result {
                    Ok(embeddings) => {
                        let count = embeddings.len();
                        if count > 0 {
                            debug!("📥 Loading {} embeddings into vector store", count);

                            // Store each embedding in the vector store
                            for (symbol_id, vector) in embeddings {
                                if let Err(e) = store.store_vector(symbol_id.clone(), vector) {
                                    warn!("Failed to store embedding for symbol {}: {}", symbol_id, e);
                                }
                            }

                            debug!("✅ Loaded {} embeddings into vector store", count);

                            // Load HNSW index from disk
                            match store.load_hnsw_index(&vectors_dir) {
                                Ok(_) => {
                                    debug!("✅ HNSW index loaded from disk for workspace '{}'", workspace_id);
                                }
                                Err(e) => {
                                    warn!("⚠️ Failed to load HNSW from disk: {}. Will use brute-force search.", e);
                                }
                            }
                        }

                        Some(std::sync::Arc::new(tokio::sync::RwLock::new(store)))
                    }
                    Err(e) => {
                        warn!("Could not load embeddings from database: {}. Semantic search not available.", e);
                        None
                    }
                }
            } else {
                debug!("Vector store directory does not exist for workspace '{}'", workspace_id);
                None
            };

            (db, vector_store, workspace_id.clone())
        } else {
            // Multiple workspace IDs not supported for semantic search
            return Err(anyhow::anyhow!(
                "Semantic search only supports single workspace filtering"
            ));
        }
    } else {
        // No workspace filter - use primary workspace's database and vectors
        let db = primary_workspace
            .db
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No database available for semantic search"))?
            .clone();

        let vector_store = primary_workspace.vector_store.clone();

        // Get primary workspace ID for logging
        use crate::workspace::registry_service::WorkspaceRegistryService;
        let registry_service = WorkspaceRegistryService::new(primary_workspace.root.clone());
        let primary_id = registry_service.get_primary_workspace_id().await?.unwrap_or_else(|| "primary".to_string());

        (db, vector_store, primary_id)
    };

    // Check if vector_store is ready
    let vector_store = match vector_store {
        Some(vs) => vs,
        None => {
            warn!("Vector store not available for workspace '{}' - falling back to text search", target_workspace_id);
            return crate::tools::search::text_search::text_search_impl(
                query,
                language,
                file_pattern,
                limit,
                workspace_ids,
                handler,
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
                    query,
                    language,
                    file_pattern,
                    limit,
                    workspace_ids,
                    handler,
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
            query,
            language,
            file_pattern,
            limit,
            workspace_ids,
            handler,
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
                query,
                language,
                file_pattern,
                limit,
                workspace_ids,
                handler,
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
