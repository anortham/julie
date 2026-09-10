use std::collections::HashSet;

use anyhow::{Result, anyhow};
use julie_extractors::SymbolKind;
use julie_index::graph::{Graph, SymbolId};

use super::BlastRadiusTool;
use crate::navigation::resolution::find_symbols;

#[derive(Debug, Clone)]
pub struct SeedContext {
    pub seed_symbols: Vec<SymbolId>,
    pub changed_files: Vec<String>,
}

/// Seeds are graph row ids or symbol names (`symbol_ids`) and file paths
/// (`file_paths`); every matching definition of a name seeds the walk.
pub fn resolve_seed_context(tool: &BlastRadiusTool, graph: &Graph) -> Result<SeedContext> {
    if tool.symbol_ids.is_empty() && tool.file_paths.is_empty() {
        return Err(anyhow!("blast_radius requires symbol_ids or file_paths."));
    }

    let mut seed_symbols = Vec::new();
    let mut missing_ids = Vec::new();
    for requested in &tool.symbol_ids {
        let matches = match graph.symbol_by_row_id(requested) {
            Some(id) => vec![id],
            None => find_symbols(graph, requested, None),
        };
        if matches.is_empty() {
            missing_ids.push(requested.clone());
        }
        seed_symbols.extend(matches);
    }
    missing_ids.sort();
    if !missing_ids.is_empty() {
        return Err(anyhow!(
            "Unknown symbol ids for blast_radius: {}",
            missing_ids.join(", ")
        ));
    }

    let mut changed_files = tool.file_paths.clone();
    for file_path in &tool.file_paths {
        seed_symbols.extend(
            graph
                .symbols_in_path(file_path)
                .iter()
                .copied()
                .filter(|id| is_file_path_seed_kind(&graph.symbol(*id).kind)),
        );
    }

    let mut seen = HashSet::new();
    seed_symbols.retain(|id| seen.insert(*id));
    changed_files.sort();
    changed_files.dedup();

    if seed_symbols.is_empty() {
        return Err(anyhow!(
            "No indexed symbols found for the requested blast_radius seeds."
        ));
    }

    Ok(SeedContext {
        seed_symbols,
        changed_files,
    })
}

fn is_file_path_seed_kind(kind: &SymbolKind) -> bool {
    !matches!(
        kind,
        SymbolKind::Import
            | SymbolKind::Export
            | SymbolKind::Field
            | SymbolKind::EnumMember
            | SymbolKind::Property
            | SymbolKind::Variable
            | SymbolKind::Constant
    )
}
