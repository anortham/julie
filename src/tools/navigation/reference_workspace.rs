//! Reference workspace database utilities
//!
//! This module handles database operations for reference workspaces
//! (workspaces other than the primary one).

use anyhow::Result;
use tracing::debug;

use crate::extractors::{Relationship, Symbol};
use crate::handler::JulieServerHandler;
use crate::utils::cross_language_intelligence::generate_naming_variants;

/// Find references in a reference workspace by opening its separate database
///
/// Reference workspaces have separate DB files at indexes/{workspace_id}/db/symbols.db
/// The old code incorrectly queried primary workspace DB with workspace_id filtering
pub async fn find_references_in_reference_workspace(
    handler: &JulieServerHandler,
    ref_workspace_id: String,
    symbol: &str,
) -> Result<(Vec<Symbol>, Vec<Relationship>)> {
    // Get primary workspace to access workspace_db_path() helper
    let primary_workspace = handler
        .get_workspace()
        .await?
        .ok_or_else(|| anyhow::anyhow!("No workspace initialized"))?;

    // Get path to reference workspace's separate database file
    let ref_db_path = primary_workspace.workspace_db_path(&ref_workspace_id);

    debug!(
        "🗄️ Opening reference workspace DB: {}",
        ref_db_path.display()
    );

    // 🚨 CRITICAL FIX: Wrap blocking file I/O in spawn_blocking
    // Opening SQLite database involves blocking filesystem operations
    let ref_db =
        tokio::task::spawn_blocking(move || crate::database::SymbolDatabase::new(ref_db_path))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to spawn database open task: {}", e))??;

    // Query the reference workspace database (not primary!)
    // ✅ NO MUTEX: ref_db is owned (not Arc<Mutex<>>), so we can call directly
    let (definitions, mut references) = {
        // Strategy 1: Find exact matches by name
        let mut defs = ref_db.get_symbols_by_name(symbol)?;

        debug!(
            "⚡ Reference workspace search found {} exact matches",
            defs.len()
        );

        // Strategy 2: Cross-language Intelligence Layer - naming convention variants
        let variants = generate_naming_variants(symbol);
        debug!("🔍 Cross-language search variants: {:?}", variants);

        for variant in variants {
            if variant != symbol {
                if let Ok(variant_symbols) = ref_db.get_symbols_by_name(&variant) {
                    for sym in variant_symbols {
                        if sym.name == variant {
                            debug!(
                                "✨ Found cross-language match: {} (variant: {})",
                                sym.name, variant
                            );
                            defs.push(sym);
                        }
                    }
                }
            }
        }

        // Remove duplicates
        defs.sort_by(|a, b| a.id.cmp(&b.id));
        defs.dedup_by(|a, b| a.id == b.id);

        // Strategy 3: Find direct relationships - REFERENCES TO these symbols
        let mut refs: Vec<Relationship> = Vec::new();

        // Collect all definition IDs for single batch query
        let definition_ids: Vec<String> = defs.iter().map(|d| d.id.clone()).collect();

        // Single batch query instead of N individual queries
        if let Ok(symbol_references) = ref_db.get_relationships_to_symbols(&definition_ids) {
            refs.extend(symbol_references);
        }

        Ok::<(Vec<Symbol>, Vec<Relationship>), anyhow::Error>((defs, refs))
    }?;

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

    debug!(
        "✅ Reference workspace search: {} definitions, {} references",
        definitions.len(),
        references.len()
    );

    Ok((definitions, references))
}

/// Find definitions in a reference workspace by opening its separate database
pub async fn find_definitions_in_reference_workspace(
    handler: &JulieServerHandler,
    ref_workspace_id: String,
    symbol: &str,
    context_file: Option<&str>,
) -> Result<Vec<Symbol>> {
    // Get primary workspace to access workspace_db_path() helper
    let primary_workspace = handler
        .get_workspace()
        .await?
        .ok_or_else(|| anyhow::anyhow!("No workspace initialized"))?;

    // Get path to reference workspace's separate database file
    let ref_db_path = primary_workspace.workspace_db_path(&ref_workspace_id);

    debug!(
        "🗄️ Opening reference workspace DB: {}",
        ref_db_path.display()
    );

    // 🚨 CRITICAL FIX: Wrap blocking file I/O in spawn_blocking
    // Opening SQLite database involves blocking filesystem operations
    let ref_db =
        tokio::task::spawn_blocking(move || crate::database::SymbolDatabase::new(ref_db_path))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to spawn database open task: {}", e))??;

    // Query the reference workspace database (not primary!)
    // ✅ NO MUTEX: ref_db is owned (not Arc<Mutex<>>), so we can call directly
    let mut exact_matches = {
        // Find exact matches by name
        let mut matches = ref_db.get_symbols_by_name(symbol)?;

        // Strategy 2: Cross-language Intelligence Layer - naming convention variants
        if matches.is_empty() {
            debug!(
                "🌍 Attempting cross-language resolution for '{}' in reference workspace",
                symbol
            );

            // Generate all naming convention variants
            let variants = generate_naming_variants(symbol);

            for variant in variants {
                if variant != symbol {
                    if let Ok(variant_symbols) = ref_db.get_symbols_by_name(&variant) {
                        if !variant_symbols.is_empty() {
                            debug!(
                                "🎯 Found cross-language match: {} -> {} ({} results)",
                                symbol,
                                variant,
                                variant_symbols.len()
                            );
                            matches.extend(variant_symbols);
                        }
                    }
                }
            }
        }
        Ok::<Vec<Symbol>, anyhow::Error>(matches)
    }?;

    // Remove duplicates based on symbol id
    exact_matches.sort_by(|a, b| a.id.cmp(&b.id));
    exact_matches.dedup_by(|a, b| a.id == b.id);

    // Prioritize results using shared logic
    use super::resolution::compare_symbols_by_priority_and_context;

    exact_matches.sort_by(|a, b| {
        // Use shared prioritization logic (definition priority + context file preference)
        let shared_cmp = compare_symbols_by_priority_and_context(a, b, context_file);
        if shared_cmp != std::cmp::Ordering::Equal {
            return shared_cmp;
        }

        // Finally by file path alphabetically
        a.file_path.cmp(&b.file_path)
    });

    debug!(
        "✅ Reference workspace search returned {} results",
        exact_matches.len()
    );
    Ok(exact_matches)
}
