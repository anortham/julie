//! Semantic similarity matching for symbol references and definitions
//!
//! This module handles HNSW-based semantic search when exact name matching
//! and naming convention variants don't find results.

use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

use crate::database::SymbolDatabase;
use crate::embeddings::vector_store::VectorStore;
use crate::extractors::{Relationship, RelationshipKind, Symbol};
use crate::handler::JulieServerHandler;

/// Find semantically similar definitions using a provided vector store and database path
///
/// This is a generalized version that works with any workspace's vector store and database.
/// Used by both primary workspace (fast_goto.rs) and reference workspaces (reference_workspace.rs).
///
/// Parameters:
/// - `vector_store`: The vector store to search (can be primary or reference workspace)
/// - `db_path`: Path to the symbol database
/// - `query_embedding`: The embedding vector for the symbol to search for
///
/// Returns: Vec of Symbol definitions that are semantically similar
pub async fn find_semantic_definitions_with_store(
    vector_store: Arc<RwLock<VectorStore>>,
    db_path: std::path::PathBuf,
    query_embedding: Vec<f32>,
) -> Result<Vec<Symbol>> {
    let mut semantic_symbols = Vec::new();

    // Check if HNSW index is built
    let has_hnsw = {
        let store_guard = vector_store.read().await;
        store_guard.has_hnsw_index()
    };

    if !has_hnsw {
        debug!("⚠️ HNSW index not available in vector store - skipping embedding search fallback");
        return Ok(Vec::new());
    }

    // Do HNSW search inside spawn_blocking (database is not Send)
    let vector_store_clone = vector_store.clone();
    let db_path_for_search = db_path.clone();
    let similar_symbols = tokio::task::spawn_blocking(move || {
        // Open database inside blocking context
        if let Ok(database) = SymbolDatabase::new(&db_path_for_search) {
            let store_guard_sync = vector_store_clone.blocking_read();
            let model_name = "bge-small";
            // INTENTIONALLY HARDCODED threshold (0.7): Conservative fallback for definition lookup.
            // This prevents AI agents from iterating through multiple thresholds and wasting context.
            store_guard_sync.search_similar_hnsw(&database, &query_embedding, 10, 0.7, model_name)
        } else {
            Err(anyhow::anyhow!("Failed to open database at {:?}", db_path_for_search))
        }
    })
    .await
    .map_err(|e| anyhow::anyhow!("spawn_blocking join error: {}", e))??;

    debug!(
        "🚀 HNSW search found {} similar definitions",
        similar_symbols.len()
    );

    // Get actual symbol data from database (also needs to be in blocking context)
    if !similar_symbols.is_empty() {
        let symbol_ids: Vec<String> =
            similar_symbols.iter().map(|r| r.symbol_id.clone()).collect();

        let db_path_for_fetch = db_path.clone();
        let symbols = tokio::task::spawn_blocking(move || {
            if let Ok(database) = SymbolDatabase::new(&db_path_for_fetch) {
                database.get_symbols_by_ids(&symbol_ids)
            } else {
                Err(anyhow::anyhow!("Failed to open database at {:?}", db_path_for_fetch))
            }
        })
        .await
        .map_err(|e| anyhow::anyhow!("spawn_blocking join error: {}", e))??;

        semantic_symbols.extend(symbols);
    }

    Ok(semantic_symbols)
}

/// Find semantically similar symbols and create reference relationships
///
/// Uses HNSW index when available, falls back to brute-force vector search.
/// Uses strict threshold (0.75) to prevent false positives.
pub async fn find_semantic_references(
    handler: &JulieServerHandler,
    symbol_name: &str,
    existing_def_ids: &HashSet<String>,
    existing_ref_ids: &HashSet<String>,
) -> Result<(Vec<Symbol>, Vec<Relationship>)> {
    let mut semantic_symbols = Vec::new();
    let mut semantic_refs = Vec::new();

    // Check store readiness FIRST before acquiring expensive resources
    if let Ok(()) = handler.ensure_vector_store().await {
        if let Ok(Some(workspace)) = handler.get_workspace().await {
            if let Some(vector_store) = workspace.vector_store.as_ref() {
                // 🔧 REFACTOR: Check if HNSW index is built
                let has_hnsw = {
                    let store_guard = vector_store.read().await;
                    store_guard.has_hnsw_index()
                };

                if !has_hnsw {
                    debug!("⚠️ HNSW index not available - skipping embedding similarity search");
                    return Ok((Vec::new(), Vec::new()));
                }

                if let Ok(()) = handler.ensure_embedding_engine().await {
                    // Get embedding for query
                    let query_embedding = {
                        let mut embedding_guard = handler.embedding_engine.write().await;
                        if let Some(embedding_engine) = embedding_guard.as_mut() {
                            embedding_engine.embed_text(symbol_name).ok()
                        } else {
                            None
                        }
                    };

                    if let Some(embedding) = query_embedding {
                        // STRICT threshold: 0.75 = only VERY similar symbols
                        // INTENTIONALLY HARDCODED to prevent false positives and context waste.
                        // AI agents would try multiple thresholds (0.9, 0.7, 0.5, ...) if exposed,
                        // wasting 3+ tool calls for a single search operation.
                        let similarity_threshold = 0.75;
                        let max_semantic_matches = 5;

                        // 🔧 REFACTOR: Use new architecture with SQLite on-demand fetching
                        let store_guard = vector_store.read().await;
                        let db = workspace.db.as_ref().cloned();

                        let semantic_results = if let Some(db_arc) = db {
                            match tokio::task::block_in_place(|| {
                                let db_lock = db_arc.lock().unwrap();
                                let model_name = "bge-small";
                                store_guard.search_similar_hnsw(
                                    &*db_lock,
                                    &embedding,
                                    max_semantic_matches,
                                    similarity_threshold,
                                    model_name
                                )
                            }) {
                                Ok(results) => results,
                                Err(e) => {
                                    debug!("Semantic reference search failed: {}", e);
                                    Vec::new()
                                }
                            }
                        } else {
                            Vec::new()
                        };
                        drop(store_guard);

                        debug!(
                            "🚀 HNSW search found {} semantic references",
                            semantic_results.len()
                        );

                        if !semantic_results.is_empty() {
                            if let Some(db) = workspace.db.as_ref() {
                                // Collect symbol IDs for batch query
                                let symbol_ids: Vec<String> =
                                    semantic_results.iter().map(|r| r.symbol_id.clone()).collect();
                                let db_arc = db.clone();

                                let symbols = tokio::task::spawn_blocking(move || {
                                    let db_lock = db_arc.lock().unwrap();
                                    db_lock.get_symbols_by_ids(&symbol_ids)
                                })
                                .await
                                .map_err(|e| anyhow::anyhow!("spawn_blocking join error: {}", e))?;

                                if let Ok(symbols) = symbols {
                                    // Create a map from symbol_id to similarity_score
                                    let score_map: HashMap<_, _> = semantic_results
                                        .iter()
                                        .map(|r| (r.symbol_id.clone(), r.similarity_score))
                                        .collect();

                                    // Process each symbol
                                    for symbol in symbols {
                                        // Skip if already in definitions or references
                                        if !existing_def_ids.contains(&symbol.id)
                                            && !existing_ref_ids.contains(&symbol.id)
                                        {
                                            // Get similarity score
                                            if let Some(&similarity_score) = score_map.get(&symbol.id)
                                            {
                                                semantic_symbols.push(symbol.clone());

                                                // Create metadata with similarity score
                                                let mut metadata = HashMap::new();
                                                metadata.insert(
                                                    "similarity".to_string(),
                                                    serde_json::json!(similarity_score),
                                                );
                                                // 🔧 REFACTOR: Always HNSW in new architecture
                                                metadata.insert(
                                                    "search_strategy".to_string(),
                                                    serde_json::json!("hnsw"),
                                                );

                                                // Create semantic reference relationship
                                                let semantic_ref = Relationship {
                                                    id: format!("semantic_{}", symbol.id),
                                                    from_symbol_id: format!(
                                                        "semantic_query:{}",
                                                        symbol_name
                                                    ),
                                                    to_symbol_id: symbol.id.clone(),
                                                    kind: RelationshipKind::References,
                                                    file_path: symbol.file_path.clone(),
                                                    line_number: symbol.start_line,
                                                    confidence: similarity_score,
                                                    metadata: Some(metadata),
                                                };

                                                debug!(
                                                    "✨ Semantic match: {} (similarity: {:.2})",
                                                    symbol.name, similarity_score
                                                );
                                                semantic_refs.push(semantic_ref);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok((semantic_symbols, semantic_refs))
}

/// Find semantically similar definitions
///
/// Used by fast_goto tool for definition lookup when exact matching fails.
pub async fn find_semantic_definitions(
    handler: &JulieServerHandler,
    symbol_name: &str,
) -> Result<Vec<Symbol>> {
    let mut semantic_symbols = Vec::new();

    // Check store readiness FIRST before acquiring expensive resources
    if let Ok(Some(workspace)) = handler.get_workspace().await {
        if let Some(vector_store_arc) = &workspace.vector_store {
            // 🔧 REFACTOR: Check if HNSW index is built
            let has_hnsw = {
                let store_guard = vector_store_arc.read().await;
                store_guard.has_hnsw_index()
            };

            if !has_hnsw {
                debug!("⚠️ HNSW index not available - skipping embedding search fallback");
                return Ok(Vec::new());
            }

            if let Ok(()) = handler.ensure_embedding_engine().await {
                // Get embedding for query
                let mut embedding_guard = handler.embedding_engine.write().await;
                if let Some(embedding_engine) = embedding_guard.as_mut() {
                    if let Ok(query_embedding) = embedding_engine.embed_text(symbol_name) {
                        // 🔧 REFACTOR: Use new architecture with SQLite on-demand fetching
                        let store_guard = vector_store_arc.read().await;
                        let db = workspace.db.as_ref().cloned();

                        let similar_symbols = if let Some(db_arc) = db {
                            match tokio::task::block_in_place(|| {
                                let db_lock = db_arc.lock().unwrap();
                                let model_name = "bge-small";
                                store_guard.search_similar_hnsw(&*db_lock, &query_embedding, 10, 0.7, model_name)
                            }) {
                                Ok(results) => results,
                                Err(e) => {
                                    debug!("Semantic search failed: {}", e);
                                    Vec::new()
                                }
                            }
                        } else {
                            Vec::new()
                        };
                        drop(store_guard);

                        debug!(
                            "🚀 HNSW search found {} similar definitions",
                            similar_symbols.len()
                        );

                        // Get actual symbol data from database
                        if !similar_symbols.is_empty() {
                            if let Some(db_arc) = &workspace.db {
                                let symbol_ids: Vec<String> =
                                    similar_symbols.iter().map(|r| r.symbol_id.clone()).collect();
                                let db_clone = db_arc.clone();

                                let symbols = tokio::task::spawn_blocking(move || {
                                    let db = db_clone.lock().unwrap();
                                    db.get_symbols_by_ids(&symbol_ids)
                                })
                                .await
                                .map_err(|e| anyhow::anyhow!("spawn_blocking join error: {}", e))?;

                                if let Ok(symbols) = symbols {
                                    semantic_symbols.extend(symbols);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(semantic_symbols)
}
