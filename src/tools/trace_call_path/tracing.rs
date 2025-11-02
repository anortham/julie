//! Core call path tracing algorithms

use crate::database::SymbolDatabase;
use crate::embeddings::CodeContext;
use crate::extractors::{RelationshipKind, Symbol};
use crate::handler::JulieServerHandler;
use anyhow::Result;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use tracing::debug;

use super::cross_language::{
    find_cross_language_callees, find_cross_language_callers,
    find_semantic_cross_language_callees, find_semantic_cross_language_callers,
};
use super::types::{CallPathNode, MatchType};

/// Find semantic neighbors using vector similarity search
pub async fn semantic_neighbors(
    handler: &JulieServerHandler,
    db: &Arc<Mutex<SymbolDatabase>>,
    vector_store: &Option<Arc<tokio::sync::RwLock<crate::embeddings::vector_store::VectorStore>>>,
    symbol: &Symbol,
    max_results: usize,
) -> Result<Vec<(Symbol, f32)>> {
    if max_results == 0 {
        return Ok(vec![]);
    }

    // Check if vector store is available
    let vector_store = match vector_store {
        Some(store) => store.clone(),
        None => {
            debug!("Semantic tracing disabled - no vector store for this workspace");
            return Ok(vec![]);
        }
    };

    // Ensure embedding engine is available
    if let Err(e) = handler.ensure_embedding_engine().await {
        debug!(
            "Semantic tracing disabled - embedding engine unavailable: {}",
            e
        );
        return Ok(vec![]);
    }

    let db_arc = db.clone();

    // Check if HNSW index is built
    let store_guard = vector_store.read().await;
    if !store_guard.has_hnsw_index() {
        return Ok(vec![]);
    }

    let mut embedding_guard = handler.embedding_engine.write().await;
    let embedding_engine = match embedding_guard.as_mut() {
        Some(engine) => engine,
        None => return Ok(vec![]),
    };

    let context = CodeContext {
        parent_symbol: None,
        surrounding_code: symbol.code_context.clone(),
        file_context: Some(symbol.file_path.clone()),
    };

    let embedding = embedding_engine.embed_symbol(symbol, &context)?;
    drop(embedding_guard);

    // Use new architecture with SQLite on-demand fetching
    let semantic_results = match tokio::task::block_in_place(|| {
        let db_lock = db_arc.lock().unwrap();
        let model_name = "bge-small";
        store_guard.search_similar_hnsw(
            &db_lock,
            &embedding,
            max_results,
            0.7, // Hardcoded good balance threshold
            model_name,
        )
    }) {
        Ok(results) => results,
        Err(e) => {
            debug!("Semantic neighbor search failed: {}", e);
            return Ok(vec![]);
        }
    };
    drop(store_guard);

    debug!(
        "🚀 HNSW search found {} semantic neighbors",
        semantic_results.len()
    );

    let mut matches = Vec::new();
    let db_lock = db_arc.lock().unwrap();
    for result in semantic_results {
        if let Ok(Some(candidate)) = db_lock.get_symbol_by_id(&result.symbol_id) {
            if candidate.id != symbol.id {
                matches.push((candidate, result.similarity_score));
            }
        }
    }
    drop(db_lock);

    Ok(matches)
}

/// Trace upstream (find callers) recursively
#[async_recursion::async_recursion]
pub async fn trace_upstream(
    handler: &JulieServerHandler,
    db: &Arc<Mutex<SymbolDatabase>>,
    vector_store: &Option<Arc<tokio::sync::RwLock<crate::embeddings::vector_store::VectorStore>>>,
    symbol: &Symbol,
    current_depth: u32,
    visited: &mut HashSet<String>,
    max_depth: u32,
) -> Result<Vec<CallPathNode>> {
    if current_depth >= max_depth {
        debug!(
            "Reached max depth {} for symbol {}",
            current_depth, symbol.name
        );
        return Ok(vec![]);
    }

    // Prevent infinite recursion using unique key
    let visit_key = format!("{}:{}:{}", symbol.file_path, symbol.start_line, symbol.name);
    if visited.contains(&visit_key) {
        debug!("Already visited symbol: {}", visit_key);
        return Ok(vec![]);
    }
    visited.insert(visit_key);

    let mut nodes = Vec::new();

    // Step 1: Find direct callers via relationships (upstream = relationships TO this symbol)
    debug!(
        "Finding direct callers for: {} (id: {})",
        symbol.name, symbol.id
    );

    // Build callers list - wrap in block to ensure mutex guard is dropped before .await
    let callers = {
        let db_lock = db.lock().unwrap();
        let relationships = db_lock.get_relationships_to_symbol(&symbol.id)?;

        // Filter to call relationships and collect symbol IDs
        let relevant_rels: Vec<_> = relationships
            .into_iter()
            .filter(|rel| {
                rel.to_symbol_id == symbol.id
                    && matches!(
                        rel.kind,
                        RelationshipKind::Calls | RelationshipKind::References
                    )
            })
            .collect();

        // Batch fetch all caller symbols (avoids N+1 query pattern)
        let caller_ids: Vec<String> = relevant_rels.iter().map(|r| r.from_symbol_id.clone()).collect();
        let caller_symbols = db_lock.get_symbols_by_ids(&caller_ids)?;

        // Build callers list by matching symbols with relationships
        let mut result = Vec::new();
        for rel in relevant_rels {
            if let Some(caller_symbol) =
                caller_symbols.iter().find(|s| s.id == rel.from_symbol_id)
            {
                result.push((caller_symbol.clone(), rel.kind.clone()));
            }
        }
        result
    }; // Guard dropped here automatically

    // Process callers recursively
    for (caller_symbol, rel_kind) in callers {
        let mut node = CallPathNode {
            symbol: caller_symbol.clone(),
            level: current_depth,
            match_type: MatchType::Direct,
            relationship_kind: Some(rel_kind),
            similarity: None,
            children: vec![],
        };

        // Recursively trace callers
        if current_depth + 1 < max_depth {
            node.children = trace_upstream(
                handler,
                db,
                vector_store,
                &caller_symbol,
                current_depth + 1,
                visited,
                max_depth,
            )
            .await?;
        }

        nodes.push(node);
    }

    // Step 2: Cross-language matching (always enabled - this is Julie's superpower!)
    if current_depth < max_depth {
        debug!("Finding cross-language callers for: {}", symbol.name);
        let cross_lang_callers = find_cross_language_callers(db, symbol).await?;

        for caller_symbol in cross_lang_callers {
            // Skip if already found as direct caller
            if nodes.iter().any(|n| n.symbol.id == caller_symbol.id) {
                continue;
            }

            let mut node = CallPathNode {
                symbol: caller_symbol.clone(),
                level: current_depth,
                match_type: MatchType::NamingVariant,
                relationship_kind: None,
                similarity: None,
                children: vec![],
            };

            // Recursively trace (but limit depth for cross-language to avoid explosion)
            let cross_lang_limit = get_cross_language_depth_limit(max_depth);
            if current_depth + 1 < cross_lang_limit {
                node.children = trace_upstream(
                    handler,
                    db,
                    vector_store,
                    &caller_symbol,
                    current_depth + 1,
                    visited,
                    max_depth,
                )
                .await?;
            }

            nodes.push(node);
        }

        let semantic_callers =
            find_semantic_cross_language_callers(handler, db, vector_store, symbol).await?;

        for semantic in semantic_callers {
            if nodes.iter().any(|n| n.symbol.id == semantic.symbol.id) {
                continue;
            }

            let mut node = CallPathNode {
                symbol: semantic.symbol.clone(),
                level: current_depth,
                match_type: MatchType::Semantic,
                relationship_kind: Some(semantic.relationship_kind.clone()),
                similarity: Some(semantic.similarity),
                children: vec![],
            };

            let cross_lang_limit = get_cross_language_depth_limit(max_depth);
            if current_depth + 1 < cross_lang_limit {
                node.children = trace_upstream(
                    handler,
                    db,
                    vector_store,
                    &semantic.symbol,
                    current_depth + 1,
                    visited,
                    max_depth,
                )
                .await?;
            }

            nodes.push(node);
        }
    }

    Ok(nodes)
}

/// Trace downstream (find callees) recursively
#[async_recursion::async_recursion]
pub async fn trace_downstream(
    handler: &JulieServerHandler,
    db: &Arc<Mutex<SymbolDatabase>>,
    vector_store: &Option<Arc<tokio::sync::RwLock<crate::embeddings::vector_store::VectorStore>>>,
    symbol: &Symbol,
    current_depth: u32,
    visited: &mut HashSet<String>,
    max_depth: u32,
) -> Result<Vec<CallPathNode>> {
    if current_depth >= max_depth {
        debug!(
            "Reached max depth {} for symbol {}",
            current_depth, symbol.name
        );
        return Ok(vec![]);
    }

    // Prevent infinite recursion
    let visit_key = format!("{}:{}:{}", symbol.file_path, symbol.start_line, symbol.name);
    if visited.contains(&visit_key) {
        debug!("Already visited symbol: {}", visit_key);
        return Ok(vec![]);
    }
    visited.insert(visit_key);

    let mut nodes = Vec::new();

    // Step 1: Find direct callees via relationships
    debug!(
        "Finding direct callees for: {} (id: {})",
        symbol.name, symbol.id
    );

    // Build callees list - wrap in block to ensure mutex guard is dropped before .await
    let callees = {
        let db_lock = db.lock().unwrap();
        let relationships = db_lock.get_relationships_for_symbol(&symbol.id)?;

        // Filter to call relationships and collect symbol IDs
        let relevant_rels: Vec<_> = relationships
            .into_iter()
            .filter(|rel| {
                rel.from_symbol_id == symbol.id
                    && matches!(
                        rel.kind,
                        RelationshipKind::Calls | RelationshipKind::References
                    )
            })
            .collect();

        // Batch fetch all callee symbols (avoids N+1 query pattern)
        let callee_ids: Vec<String> = relevant_rels.iter().map(|r| r.to_symbol_id.clone()).collect();
        let callee_symbols = db_lock.get_symbols_by_ids(&callee_ids)?;

        // Build callees list by matching symbols with relationships
        let mut result = Vec::new();
        for rel in relevant_rels {
            if let Some(callee_symbol) =
                callee_symbols.iter().find(|s| s.id == rel.to_symbol_id)
            {
                result.push((callee_symbol.clone(), rel.kind.clone()));
            }
        }
        result
    }; // Guard dropped here automatically

    // Process callees recursively
    for (callee_symbol, rel_kind) in callees {
        let mut node = CallPathNode {
            symbol: callee_symbol.clone(),
            level: current_depth,
            match_type: MatchType::Direct,
            relationship_kind: Some(rel_kind),
            similarity: None,
            children: vec![],
        };

        // Recursively trace callees
        if current_depth + 1 < max_depth {
            node.children = trace_downstream(
                handler,
                db,
                vector_store,
                &callee_symbol,
                current_depth + 1,
                visited,
                max_depth,
            )
            .await?;
        }

        nodes.push(node);
    }

    // Step 2: Cross-language matching (always enabled - this is Julie's superpower!)
    if current_depth < max_depth {
        debug!("Finding cross-language callees for: {}", symbol.name);
        let cross_lang_callees = find_cross_language_callees(db, symbol).await?;

        for callee_symbol in cross_lang_callees {
            // Skip if already found as direct callee
            if nodes.iter().any(|n| n.symbol.id == callee_symbol.id) {
                continue;
            }

            let mut node = CallPathNode {
                symbol: callee_symbol.clone(),
                level: current_depth,
                match_type: MatchType::NamingVariant,
                relationship_kind: None,
                similarity: None,
                children: vec![],
            };

            // Recursively trace (but limit depth for cross-language to avoid explosion)
            let cross_lang_limit = get_cross_language_depth_limit(max_depth);
            if current_depth + 1 < cross_lang_limit {
                node.children = trace_downstream(
                    handler,
                    db,
                    vector_store,
                    &callee_symbol,
                    current_depth + 1,
                    visited,
                    max_depth,
                )
                .await?;
            }

            nodes.push(node);
        }

        let semantic_callees =
            find_semantic_cross_language_callees(handler, db, vector_store, symbol).await?;

        for semantic in semantic_callees {
            if nodes.iter().any(|n| n.symbol.id == semantic.symbol.id) {
                continue;
            }

            let mut node = CallPathNode {
                symbol: semantic.symbol.clone(),
                level: current_depth,
                match_type: MatchType::Semantic,
                relationship_kind: Some(semantic.relationship_kind.clone()),
                similarity: Some(semantic.similarity),
                children: vec![],
            };

            let cross_lang_limit = get_cross_language_depth_limit(max_depth);
            if current_depth + 1 < cross_lang_limit {
                node.children = trace_downstream(
                    handler,
                    db,
                    vector_store,
                    &semantic.symbol,
                    current_depth + 1,
                    visited,
                    max_depth,
                )
                .await?;
            }

            nodes.push(node);
        }
    }

    Ok(nodes)
}

/// Get cross-language recursion depth limit
/// Uses max_depth - 1 to prevent excessive expansion
fn get_cross_language_depth_limit(max_depth: u32) -> u32 {
    max_depth.saturating_sub(1)
}
