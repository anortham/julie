//! Semantic search using HNSW embeddings
//!
//! Provides semantic similarity search powered by ONNX embeddings and HNSW indexing.
//! Falls back to text search if embeddings are unavailable.

use anyhow::Result;
use tracing::{debug, warn};

use crate::extractors::Symbol;
use crate::handler::JulieServerHandler;

use super::query::matches_glob_pattern;

/// Check if a symbol is from a vendor/third-party library
fn is_vendor_symbol(file_path: &str) -> bool {
    file_path.contains("/node_modules/")
        || file_path.contains("/wwwroot/lib/")
        || file_path.contains("/vendor/")
        || file_path.contains("/bower_components/")
        || file_path.contains("font-awesome")
        || file_path.contains("popper.js")
        || file_path.contains("/packages/")
        || file_path.contains("/.npm/")
        || file_path.contains("/third-party/")
        || file_path.contains("/external/")
}

/// Get boost factor based on symbol kind (prioritize meaningful symbols)
fn get_symbol_kind_boost(symbol: &Symbol) -> f32 {
    use crate::extractors::base::SymbolKind;
    match symbol.kind {
        SymbolKind::Class | SymbolKind::Interface | SymbolKind::Struct => 2.5,
        SymbolKind::Function | SymbolKind::Method => 2.0,
        SymbolKind::Enum | SymbolKind::Module | SymbolKind::Namespace => 1.8,
        SymbolKind::Property | SymbolKind::Field => 1.2,
        SymbolKind::Variable | SymbolKind::Constant => 0.8,
        _ => 0.3, // Anonymous functions, etc. get heavy penalty
    }
}

/// Boost symbols with documentation (rich docs = higher quality)
pub(crate) fn get_doc_comment_boost(symbol: &Symbol) -> f32 {
    match &symbol.doc_comment {
        None => 1.0,
        Some(doc) if doc.is_empty() => 1.0,
        Some(doc) => {
            let doc_len = doc.len();
            if doc_len > 200 {
                2.0 // Rich documentation
            } else if doc_len > 100 {
                1.5 // Good documentation
            } else {
                1.3 // Some documentation
            }
        }
    }
}

/// Check if symbol is an HTML element (not real code)
fn is_html_element(symbol: &Symbol) -> bool {
    use crate::extractors::base::SymbolKind;
    if symbol.kind != SymbolKind::Class {
        return false;
    }

    symbol.metadata
        .as_ref()
        .and_then(|m| m.get("type"))
        .and_then(|v| v.as_str())
        .map(|s| s == "html-element")
        .unwrap_or(false)
}

/// Boost real code over markup/templates
pub(crate) fn get_language_quality_boost(symbol: &Symbol) -> f32 {
    match symbol.language.as_str() {
        // Real code languages - high signal
        "csharp" | "rust" | "typescript" | "java" | "kotlin" => 1.2,

        // Scripting languages - good signal
        "javascript" | "python" | "ruby" | "php" => 1.1,

        // Markup/templating - context dependent
        "razor" | "vue" | "html" => {
            if is_html_element(symbol) {
                0.7 // HTML tag penalty
            } else {
                1.0 // Razor C# code is normal
            }
        }

        _ => 1.0,
    }
}

/// Check if symbol is generic framework boilerplate
pub(crate) fn is_generic_symbol(symbol: &Symbol) -> bool {
    const GENERIC_NAMES: &[&str] = &[
        "Template",
        "Container",
        "Wrapper",
        "Item",
        "Data",
        "Value",
        "Component",
        "Element",
    ];

    // Only penalize if BOTH generic name AND no documentation
    symbol.doc_comment.is_none() && GENERIC_NAMES.contains(&symbol.name.as_str())
}

/// Penalize generic undocumented symbols
pub(crate) fn get_generic_penalty(symbol: &Symbol) -> f32 {
    if is_generic_symbol(symbol) {
        0.5 // 50% penalty
    } else {
        1.0
    }
}

/// Apply all quality boost factors to a base score
pub(crate) fn apply_all_boosts(symbol: &Symbol, base_score: f32) -> f32 {
    let mut score = base_score;

    // Factor 1: Symbol kind boosting (existing)
    score *= get_symbol_kind_boost(symbol);

    // Factor 2: Doc comment boost (NEW)
    score *= get_doc_comment_boost(symbol);

    // Factor 3: Language quality boost (NEW)
    score *= get_language_quality_boost(symbol);

    // Factor 4: Generic symbol penalty (NEW)
    score *= get_generic_penalty(symbol);

    // Factor 5: Vendor penalty (existing)
    if is_vendor_symbol(&symbol.file_path) {
        score *= 0.05;
    }

    score
}

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
    let primary_workspace = handler // After scope parameter
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

            if registry_service
                .get_workspace(workspace_id)
                .await?
                .is_none()
            {
                return Err(anyhow::anyhow!(
                    "Workspace '{}' not found in registry",
                    workspace_id
                ));
            }

            // Load the specific workspace's database from primary workspace's indexes
            let db_path = primary_workspace.workspace_db_path(workspace_id);
            debug!("📂 Loading database from: {}", db_path.display());
            let db = std::sync::Arc::new(std::sync::Mutex::new(
                crate::database::SymbolDatabase::new(&db_path)?,
            ));

            // Load the specific workspace's vector store from primary workspace's indexes
            // This replicates the logic from JulieWorkspace::initialize_vector_store()
            let vectors_dir = primary_workspace.workspace_vectors_path(workspace_id);
            debug!("🧠 Loading vector store from: {}", vectors_dir.display());

            let vector_store = if vectors_dir.exists() {
                // CASCADE Architecture (2-tier): SQLite FTS5 → HNSW Semantic
                // SQLite is the single source of truth. We only load the HNSW graph structure
                // and fetch vectors from SQLite on-demand during search for re-ranking.
                let mut store = crate::embeddings::vector_store::VectorStore::new(384)?;

                // Load HNSW index from disk (just the graph structure + id_mapping)
                match store.load_hnsw_index(&vectors_dir) {
                    Ok(_) => {
                        debug!(
                            "✅ HNSW index loaded from disk for workspace '{}'",
                            workspace_id
                        );
                        Some(std::sync::Arc::new(tokio::sync::RwLock::new(store)))
                    }
                    Err(e) => {
                        warn!(
                            "⚠️ Failed to load HNSW from disk: {}. Semantic search not available.",
                            e
                        );
                        None
                    }
                }
            } else {
                debug!(
                    "Vector store directory does not exist for workspace '{}'",
                    workspace_id
                );
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
        let primary_id = registry_service
            .get_primary_workspace_id()
            .await?
            .unwrap_or_else(|| "primary".to_string());

        (db, vector_store, primary_id)
    };

    // Check if vector_store is ready
    let vector_store = match vector_store {
        Some(vs) => vs,
        None => {
            warn!(
                "Vector store not available for workspace '{}' - falling back to text search",
                target_workspace_id
            );
            return crate::tools::search::text_search::text_search_impl(
                query,
                language,
                file_pattern,
                limit,
                workspace_ids,
                "symbols", // Semantic fallback searches symbols
                None, // context_lines: use default
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
                    "symbols", // Semantic fallback searches symbols
                None, // context_lines: use default
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

    // Check if HNSW index is available (graceful degradation to text search)
    if !store_guard.has_hnsw_index() {
        debug!("⚠️ HNSW index not available - falling back to text search");
        return crate::tools::search::text_search::text_search_impl(
            query,
            language,
            file_pattern,
            limit,
            workspace_ids,
            "symbols", // Semantic fallback searches symbols
                None, // context_lines: use default
            handler,
        )
        .await;
    }

    // CASCADE Architecture: Fetch vectors from SQLite on-demand during HNSW search
    // We need to access the database within the async context, so use tokio::task::block_in_place
    let semantic_results = match tokio::task::block_in_place(|| {
        let db_lock = db.lock().unwrap();
        let model_name = "bge-small"; // Match the embedding model
        store_guard.search_similar_hnsw(
            &*db_lock,
            &query_embedding,
            search_limit,
            similarity_threshold,
            model_name,
        )
    }) {
        Ok(results) => results,
        Err(e) => {
            warn!(
                "Semantic similarity search failed: {} - falling back to text search",
                e
            );
            return crate::tools::search::text_search::text_search_impl(
                query,
                language,
                file_pattern,
                limit,
                workspace_ids,
                "symbols", // Semantic fallback searches symbols
                None, // context_lines: use default
                handler,
            )
            .await;
        }
    };
    drop(store_guard);

    let used_hnsw = true; // Always using HNSW in the new architecture

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

    // Apply multi-factor quality scoring to rerank results
    // Factors: symbol kind, doc comments, language quality, generic names, vendor status
    // Documented symbols rank higher than generic boilerplate (e.g., C# class > HTML tag)
    let mut scored_symbols: Vec<(Symbol, f32)> = symbols
        .into_iter()
        .zip(semantic_results.iter())
        .map(|(symbol, result)| {
            let score = result.similarity_score;

            // Apply all boost factors (symbol kind, docs, language, generic penalty, vendor)
            let final_score = apply_all_boosts(&symbol, score);

            (symbol, final_score)
        })
        .collect();

    // Re-sort by adjusted scores (higher is better)
    scored_symbols.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    // Extract symbols after re-ranking
    let symbols: Vec<Symbol> = scored_symbols.into_iter().map(|(symbol, _)| symbol).collect();

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
    let results: Vec<Symbol> = filtered_symbols.into_iter().take(limit as usize).collect();

    debug!(
        "✅ Semantic search returned {} results (HNSW-powered)",
        results.len()
    );
    Ok(results)
}
