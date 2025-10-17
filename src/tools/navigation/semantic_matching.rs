//! Semantic similarity matching for symbol references and definitions
//!
//! This module handles HNSW-based semantic search when exact name matching
//! and naming convention variants don't find results.

use anyhow::Result;
use std::collections::{HashMap, HashSet};
use tracing::debug;

use crate::extractors::{Relationship, RelationshipKind, Symbol};
use crate::handler::JulieServerHandler;

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
                let has_vectors = {
                    let store_guard = vector_store.read().await;
                    !store_guard.is_empty()
                };

                if !has_vectors {
                    debug!("⚠️ Semantic store empty - skipping embedding similarity search");
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
                        let similarity_threshold = 0.75;
                        let max_semantic_matches = 5;

                        let store_guard = vector_store.read().await;
                        let (semantic_results, used_hnsw) =
                            match store_guard.search_with_fallback(
                                &embedding,
                                max_semantic_matches,
                                similarity_threshold,
                            ) {
                                Ok(results) => results,
                                Err(e) => {
                                    debug!("Semantic reference search failed: {}", e);
                                    (Vec::new(), false)
                                }
                            };
                        drop(store_guard);

                        if used_hnsw {
                            debug!(
                                "🚀 Using HNSW index to find semantic references ({} results)",
                                semantic_results.len()
                            );
                        } else {
                            debug!(
                                "⚠️ Using brute-force semantic search for references ({} results)",
                                semantic_results.len()
                            );
                        }

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
                                                if !used_hnsw {
                                                    metadata.insert(
                                                        "search_strategy".to_string(),
                                                        serde_json::json!("brute-force"),
                                                    );
                                                }

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
            // Capture current store state
            let has_vectors = {
                let store_guard = vector_store_arc.read().await;
                !store_guard.is_empty()
            };

            if !has_vectors {
                debug!("⚠️ Semantic store empty - skipping embedding search fallback");
                return Ok(Vec::new());
            }

            if let Ok(()) = handler.ensure_embedding_engine().await {
                // Get embedding for query
                let mut embedding_guard = handler.embedding_engine.write().await;
                if let Some(embedding_engine) = embedding_guard.as_mut() {
                    if let Ok(query_embedding) = embedding_engine.embed_text(symbol_name) {
                        let store_guard = vector_store_arc.read().await;
                        let (similar_symbols, used_hnsw) =
                            match store_guard.search_with_fallback(&query_embedding, 10, 0.7) {
                                Ok(results) => results,
                                Err(e) => {
                                    debug!("Semantic search failed: {}", e);
                                    (Vec::new(), false)
                                }
                            };
                        drop(store_guard);

                        if used_hnsw {
                            debug!(
                                "🚀 Using HNSW index for fast semantic search ({} results)",
                                similar_symbols.len()
                            );
                        } else {
                            debug!(
                                "⚠️ Using brute-force semantic search ({} results)",
                                similar_symbols.len()
                            );
                        }

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
