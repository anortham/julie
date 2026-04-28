//! Target workspace database utilities.
//!
//! This module handles database operations for explicit workspace targets
//! (workspaces other than the primary one).

use anyhow::Result;
use std::collections::HashSet;
use tracing::debug;

use super::resolution::parse_qualified_name;
use crate::extractors::{Relationship, RelationshipKind, Symbol, SymbolKind};
use crate::handler::JulieServerHandler;
use crate::utils::cross_language_intelligence::generate_naming_variants;

/// Find references in a target workspace using handler helpers for DB access.
///
/// Supports the same strategies as the primary workspace path:
/// 1. Exact name lookup
/// 2. Cross-language naming variants
/// 3. Relationship-based refs (optionally filtered by `reference_kind`)
/// 4. Identifier-based refs (optionally filtered by `reference_kind`)
///
/// Results are sorted by confidence (descending) then truncated to `limit`.
pub async fn find_references_in_target_workspace(
    handler: &JulieServerHandler,
    target_workspace_id: String,
    symbol: &str,
    limit: u32,
    reference_kind: Option<&str>,
) -> Result<(Vec<Symbol>, Vec<Relationship>)> {
    // Use handler helper for DB access
    let db_arc = handler
        .get_database_for_workspace(&target_workspace_id)
        .await?;

    debug!(
        "Querying target workspace DB via handler helper: {}",
        target_workspace_id
    );

    let symbol_owned = symbol.to_string();
    let (effective_symbol, parent_filter) = match parse_qualified_name(&symbol_owned) {
        Some((parent, child)) => (child.to_string(), Some(parent.to_string())),
        None => (symbol_owned.clone(), None),
    };
    let reference_kind_owned = reference_kind.map(|s| s.to_string());

    // All DB work in spawn_blocking (SQLite is synchronous)
    let (definitions, mut references) = tokio::task::spawn_blocking(move || -> Result<_> {
        let ref_db = db_arc
            .lock()
            .map_err(|e| anyhow::anyhow!("Database lock error: {}", e))?;

        // Strategy 1: Find exact matches by name
        let mut defs = ref_db.get_symbols_by_name(&effective_symbol)?;

        debug!("Target workspace search found {} exact matches", defs.len());

        // Strategy 2: Cross-language Intelligence Layer - naming convention variants
        let variants = generate_naming_variants(&effective_symbol);
        debug!("Cross-language search variants: {:?}", variants);

        for variant in &variants {
            if *variant != effective_symbol {
                if let Ok(variant_symbols) = ref_db.get_symbols_by_name(variant) {
                    for sym in variant_symbols {
                        if sym.name == *variant {
                            debug!(
                                "Found cross-language match: {} (variant: {})",
                                sym.name, variant
                            );
                            defs.push(sym);
                        }
                    }
                }
            }
        }

        if let Some(ref parent_name) = parent_filter {
            let parent_ids: Vec<String> = defs
                .iter()
                .filter_map(|s| s.parent_id.clone())
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();

            if !parent_ids.is_empty() {
                let parents = ref_db.get_symbols_by_ids(&parent_ids)?;
                let matching_parent_ids: HashSet<String> = parents
                    .into_iter()
                    .filter(|p| p.name == *parent_name)
                    .map(|p| p.id)
                    .collect();

                defs.retain(|s| {
                    s.parent_id
                        .as_deref()
                        .map(|pid| matching_parent_ids.contains(pid))
                        .unwrap_or(false)
                });
            } else {
                defs.clear();
            }
        }

        // Remove duplicates
        defs.sort_by(|a, b| a.id.cmp(&b.id));
        defs.dedup_by(|a, b| a.id == b.id);

        let mut import_refs: Vec<Relationship> = Vec::new();
        defs.retain(|sym| {
            if sym.kind == SymbolKind::Import {
                import_refs.push(Relationship {
                    id: format!("import_{}_{}", sym.file_path, sym.start_line),
                    from_symbol_id: sym.id.clone(),
                    to_symbol_id: String::new(),
                    kind: RelationshipKind::Imports,
                    file_path: sym.file_path.clone(),
                    line_number: sym.start_line,
                    confidence: 1.0,
                    metadata: None,
                });
                false
            } else {
                true
            }
        });

        // Strategy 3: Find direct relationships - REFERENCES TO these symbols
        // Collect all definition IDs for single batch query
        let definition_ids: Vec<String> = defs.iter().map(|d| d.id.clone()).collect();

        // Single batch query, optionally filtered by identifier kind
        let mut refs: Vec<Relationship> = match reference_kind_owned.as_deref() {
            Some(kind) if kind != "import" => Vec::new(),
            _ => import_refs,
        };

        if let Some(ref kind) = reference_kind_owned {
            ref_db
                .get_relationships_to_symbols_filtered_by_kind(&definition_ids, kind)
                .unwrap_or_default()
        } else {
            ref_db
                .get_relationships_to_symbols(&definition_ids)
                .unwrap_or_default()
        }
        .into_iter()
        .for_each(|rel| refs.push(rel));

        // Strategy 4: Identifier-based reference discovery
        // The identifiers table stores every usage site extracted by all 31 language extractors.
        // This catches references that relationships miss (struct type usages, function calls
        // without extracted relationships, member accesses, etc.)
        let mut all_names = vec![effective_symbol.clone()];
        for v in &variants {
            if *v != effective_symbol {
                all_names.push(v.clone());
            }
        }

        let first_def_id = defs.first().map(|d| d.id.clone()).unwrap_or_default();
        let resolved_definition_ids: HashSet<String> = defs.iter().map(|d| d.id.clone()).collect();
        let qualified_lookup = parent_filter.is_some();

        let identifier_refs = if let Some(ref kind) = reference_kind_owned {
            ref_db
                .get_identifiers_by_names_and_kind(&all_names, kind)
                .unwrap_or_default()
        } else {
            ref_db
                .get_identifiers_by_names(&all_names)
                .unwrap_or_default()
        };

        // Build dedup set from existing relationships AND definitions
        // so identifier entries at definition sites don't create duplicates
        let mut existing_refs: HashSet<(String, u32)> = refs
            .iter()
            .map(|r| (r.file_path.clone(), r.line_number))
            .collect();
        for def in &defs {
            existing_refs.insert((def.file_path.clone(), def.start_line));
        }

        let mut added = 0;
        for ident in identifier_refs {
            let key = (ident.file_path.clone(), ident.start_line);
            if existing_refs.contains(&key) {
                continue; // Prefer existing relationship (richer data)
            }

            if qualified_lookup
                && !ident
                    .target_symbol_id
                    .as_deref()
                    .map(|target_id| resolved_definition_ids.contains(target_id))
                    .unwrap_or(false)
            {
                continue;
            }

            // Convert IdentifierKind string to RelationshipKind
            let rel_kind = match ident.kind.as_str() {
                "call" => RelationshipKind::Calls,
                "import" => RelationshipKind::Imports,
                "type_usage" => RelationshipKind::Uses,
                "member_access" => RelationshipKind::References,
                _ => RelationshipKind::References,
            };

            refs.push(Relationship {
                id: format!("ident_{}_{}", ident.file_path, ident.start_line),
                from_symbol_id: ident.containing_symbol_id.unwrap_or_default(),
                to_symbol_id: first_def_id.clone(),
                kind: rel_kind,
                file_path: ident.file_path,
                line_number: ident.start_line,
                confidence: ident.confidence,
                metadata: None,
            });
            existing_refs.insert(key);
            added += 1;
        }

        debug!(
            "Identifiers added {} new references (deduped from existing relationships)",
            added
        );

        Ok((defs, refs))
    })
    .await
    .map_err(|e| anyhow::anyhow!("spawn_blocking error: {}", e))??;

    // Sort references by confidence and location
    references.sort_by(|a, b| {
        let conf_cmp = b
            .confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal);
        if conf_cmp != std::cmp::Ordering::Equal {
            return conf_cmp;
        }
        let file_cmp = a.file_path.cmp(&b.file_path);
        if file_cmp != std::cmp::Ordering::Equal {
            return file_cmp;
        }
        a.line_number.cmp(&b.line_number)
    });

    // Apply user-specified limit to prevent massive responses
    // Truncate AFTER sorting to return the top N most relevant references
    references.truncate(limit as usize);

    debug!(
        "Target workspace search: {} definitions, {} references (limit: {})",
        definitions.len(),
        references.len(),
        limit
    );

    Ok((definitions, references))
}

// find_definitions_in_target_workspace removed; fast_goto left the toolset earlier.
