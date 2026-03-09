//! Reference workspace database utilities
//!
//! This module handles database operations for reference workspaces
//! (workspaces other than the primary one).

use anyhow::Result;
use tracing::debug;

use crate::extractors::{Relationship, Symbol};
use crate::handler::JulieServerHandler;
use crate::utils::cross_language_intelligence::generate_naming_variants;

/// Find references in a reference workspace using handler helpers for DB access
///
/// In daemon mode, uses the shared loaded workspace DB (no re-opening).
/// In stdio mode, opens the reference workspace's SQLite database from disk.
pub async fn find_references_in_reference_workspace(
    handler: &JulieServerHandler,
    ref_workspace_id: String,
    symbol: &str,
) -> Result<(Vec<Symbol>, Vec<Relationship>)> {
    // Use handler helper for DB access (shared in daemon mode, opens fresh in stdio mode)
    let db_arc = handler
        .get_database_for_workspace(&ref_workspace_id)
        .await?;

    debug!(
        "Querying reference workspace DB via handler helper: {}",
        ref_workspace_id
    );

    let symbol_owned = symbol.to_string();

    // All DB work in spawn_blocking (SQLite is synchronous)
    let (definitions, mut references) = tokio::task::spawn_blocking(move || -> Result<_> {
        let ref_db = db_arc
            .lock()
            .map_err(|e| anyhow::anyhow!("Database lock error: {}", e))?;

        // Strategy 1: Find exact matches by name
        let mut defs = ref_db.get_symbols_by_name(&symbol_owned)?;

        debug!(
            "Reference workspace search found {} exact matches",
            defs.len()
        );

        // Strategy 2: Cross-language Intelligence Layer - naming convention variants
        let variants = generate_naming_variants(&symbol_owned);
        debug!("Cross-language search variants: {:?}", variants);

        for variant in variants {
            if variant != symbol_owned {
                if let Ok(variant_symbols) = ref_db.get_symbols_by_name(&variant) {
                    for sym in variant_symbols {
                        if sym.name == variant {
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

    debug!(
        "Reference workspace search: {} definitions, {} references",
        definitions.len(),
        references.len()
    );

    Ok((definitions, references))
}

// find_definitions_in_reference_workspace removed — fast_goto cut in toolset redesign
