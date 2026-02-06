//! Core call path tracing algorithms

use crate::database::SymbolDatabase;
use crate::extractors::{RelationshipKind, Symbol};
use anyhow::Result;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use tracing::debug;

use super::cross_language::{find_cross_language_symbols, is_generic_name};
use super::types::{CallPathNode, MatchType};

/// Safety limits to prevent explosion on "hub" symbols (e.g., commonly-used functions)
/// These limits ensure trace_call_path remains responsive even for symbols with hundreds of callers
const MAX_CALLERS_PER_LEVEL: usize = 50;
const MAX_TOTAL_NODES: usize = 500;

/// Trace upstream (find callers) recursively
#[async_recursion::async_recursion]
pub async fn trace_upstream(
    db: &Arc<Mutex<SymbolDatabase>>,
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
        let db_lock = super::lock_db(db, "trace_upstream callers");
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
        let caller_ids: Vec<String> = relevant_rels
            .iter()
            .map(|r| r.from_symbol_id.clone())
            .collect();
        let caller_symbols = db_lock.get_symbols_by_ids(&caller_ids)?;

        // Build callers list by matching symbols with relationships
        let mut result = Vec::new();
        for rel in relevant_rels {
            if let Some(caller_symbol) = caller_symbols.iter().find(|s| s.id == rel.from_symbol_id)
            {
                result.push((caller_symbol.clone(), rel.kind.clone()));
            }
        }
        // Dedup by symbol ID (multiple call sites → single entry)
        let mut seen_ids = HashSet::new();
        result.retain(|(sym, _)| seen_ids.insert(sym.id.clone()));
        result
    }; // Guard dropped here automatically

    // Safety limit: truncate callers to prevent explosion on hub symbols
    let total_callers = callers.len();
    let callers: Vec<_> = if total_callers > MAX_CALLERS_PER_LEVEL {
        debug!(
            "⚠️  Hub symbol detected: {} has {} callers, truncating to {} (depth {})",
            symbol.name, total_callers, MAX_CALLERS_PER_LEVEL, current_depth
        );
        callers.into_iter().take(MAX_CALLERS_PER_LEVEL).collect()
    } else {
        callers
    };

    // Process callers recursively
    for (caller_symbol, rel_kind) in callers {
        let mut node = CallPathNode {
            symbol: caller_symbol.clone(),
            level: current_depth,
            match_type: MatchType::Direct,
            relationship_kind: Some(rel_kind),
            children: vec![],
        };

        // Recursively trace callers
        if current_depth + 1 < max_depth {
            node.children = trace_upstream(
                db,
                &caller_symbol,
                current_depth + 1,
                visited,
                max_depth,
            )
            .await?;
        }

        nodes.push(node);
    }

    // Step 1b: Identifier-based caller discovery (supplements relationships)
    // The identifiers table captures call sites from token-level extraction
    // that the relationship table may miss (e.g., dynamic dispatch, indirect calls).
    if nodes.len() < MAX_TOTAL_NODES {
        let identifier_callers = {
            let db_lock = super::lock_db(db, "trace_upstream identifiers");
            let variants =
                crate::utils::cross_language_intelligence::generate_naming_variants(&symbol.name);
            let ident_refs = db_lock.get_identifiers_by_names_and_kind(&variants, "call")?;

            // Collect unique containing_symbol_ids — these are the callers
            let mut seen = HashSet::new();
            let caller_ids: Vec<String> = ident_refs
                .iter()
                .filter_map(|ident| ident.containing_symbol_id.clone())
                .filter(|id| !id.is_empty() && seen.insert(id.clone()))
                .collect();

            if !caller_ids.is_empty() {
                debug!(
                    "Found {} identifier-based callers for {}",
                    caller_ids.len(),
                    symbol.name
                );
                db_lock.get_symbols_by_ids(&caller_ids)?
            } else {
                vec![]
            }
        };

        for caller_symbol in identifier_callers {
            if nodes.len() >= MAX_TOTAL_NODES {
                break;
            }
            // Skip if already found via relationships
            if nodes.iter().any(|n| n.symbol.id == caller_symbol.id) {
                continue;
            }

            let mut node = CallPathNode {
                symbol: caller_symbol.clone(),
                level: current_depth,
                match_type: MatchType::Direct,
                relationship_kind: Some(RelationshipKind::Calls),
                children: vec![],
            };

            if current_depth + 1 < max_depth {
                node.children = trace_upstream(
                    db,
                    &caller_symbol,
                    current_depth + 1,
                    visited,
                    max_depth,
                )
                .await?;
            }

            nodes.push(node);
        }
    }

    // Step 2: Cross-language matching (always enabled - this is Julie's superpower!)
    // Skip if we've already hit the node limit from direct callers
    if current_depth < max_depth && nodes.len() < MAX_TOTAL_NODES {
        debug!("Finding cross-language callers for: {}", symbol.name);
        let cross_lang_callers = find_cross_language_symbols(db, symbol).await?;

        for caller_symbol in cross_lang_callers {
            // Safety: stop if we've hit the total node limit
            if nodes.len() >= MAX_TOTAL_NODES {
                debug!("⚠️  Hit MAX_TOTAL_NODES limit ({}), stopping cross-language search", MAX_TOTAL_NODES);
                break;
            }
            // Skip if already found as direct caller
            if nodes.iter().any(|n| n.symbol.id == caller_symbol.id) {
                continue;
            }

            let mut node = CallPathNode {
                symbol: caller_symbol.clone(),
                level: current_depth,
                match_type: MatchType::NamingVariant,
                relationship_kind: None,
                children: vec![],
            };

            // Recursively trace (but limit depth for cross-language to avoid explosion)
            let cross_lang_limit = get_cross_language_depth_limit(max_depth);
            if current_depth + 1 < cross_lang_limit {
                node.children = trace_upstream(
                    db,
                    &caller_symbol,
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
    db: &Arc<Mutex<SymbolDatabase>>,
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
        let db_lock = super::lock_db(db, "trace_downstream callees");
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
        let callee_ids: Vec<String> = relevant_rels
            .iter()
            .map(|r| r.to_symbol_id.clone())
            .collect();
        let callee_symbols = db_lock.get_symbols_by_ids(&callee_ids)?;

        // Build callees list by matching symbols with relationships
        let mut result = Vec::new();
        for rel in relevant_rels {
            if let Some(callee_symbol) = callee_symbols.iter().find(|s| s.id == rel.to_symbol_id) {
                result.push((callee_symbol.clone(), rel.kind.clone()));
            }
        }
        // Dedup by symbol ID (multiple call sites → single entry)
        let mut seen_ids = HashSet::new();
        result.retain(|(sym, _)| seen_ids.insert(sym.id.clone()));
        // Filter out generic names that create noise (clone, to_string, len, etc.)
        result.retain(|(sym, _)| !is_generic_name(&sym.name));
        result
    }; // Guard dropped here automatically

    // Safety limit: truncate callees to prevent explosion on hub symbols
    let total_callees = callees.len();
    let callees: Vec<_> = if total_callees > MAX_CALLERS_PER_LEVEL {
        debug!(
            "⚠️  Hub symbol detected: {} has {} callees, truncating to {} (depth {})",
            symbol.name, total_callees, MAX_CALLERS_PER_LEVEL, current_depth
        );
        callees.into_iter().take(MAX_CALLERS_PER_LEVEL).collect()
    } else {
        callees
    };

    // Process callees recursively
    for (callee_symbol, rel_kind) in callees {
        let mut node = CallPathNode {
            symbol: callee_symbol.clone(),
            level: current_depth,
            match_type: MatchType::Direct,
            relationship_kind: Some(rel_kind),
            children: vec![],
        };

        // Recursively trace callees
        if current_depth + 1 < max_depth {
            node.children = trace_downstream(
                db,
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
    // Skip if we've already hit the node limit from direct callees
    if current_depth < max_depth && nodes.len() < MAX_TOTAL_NODES {
        debug!("Finding cross-language callees for: {}", symbol.name);
        let cross_lang_callees = find_cross_language_symbols(db, symbol).await?;

        for callee_symbol in cross_lang_callees {
            // Safety: stop if we've hit the total node limit
            if nodes.len() >= MAX_TOTAL_NODES {
                debug!("⚠️  Hit MAX_TOTAL_NODES limit ({}), stopping cross-language search", MAX_TOTAL_NODES);
                break;
            }
            // Skip if already found as direct callee
            if nodes.iter().any(|n| n.symbol.id == callee_symbol.id) {
                continue;
            }

            let mut node = CallPathNode {
                symbol: callee_symbol.clone(),
                level: current_depth,
                match_type: MatchType::NamingVariant,
                relationship_kind: None,
                children: vec![],
            };

            // Recursively trace (but limit depth for cross-language to avoid explosion)
            let cross_lang_limit = get_cross_language_depth_limit(max_depth);
            if current_depth + 1 < cross_lang_limit {
                node.children = trace_downstream(
                    db,
                    &callee_symbol,
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
