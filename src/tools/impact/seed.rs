use std::collections::HashSet;

use anyhow::{Result, anyhow};

use super::BlastRadiusTool;
use crate::database::RevisionChangeKind;
use crate::database::SymbolDatabase;
use crate::extractors::Symbol;

#[derive(Debug, Clone)]
pub struct SeedContext {
    pub seed_symbols: Vec<Symbol>,
    pub changed_files: Vec<String>,
    pub deleted_files: Vec<String>,
}

pub fn resolve_seed_context(
    tool: &BlastRadiusTool,
    db: &SymbolDatabase,
    workspace_id: &str,
) -> Result<SeedContext> {
    validate_request(tool)?;

    let mut seed_symbols = Vec::new();
    let mut changed_files = Vec::new();
    let mut deleted_files = Vec::new();

    if !tool.symbol_ids.is_empty() {
        let requested_ids: HashSet<&str> = tool.symbol_ids.iter().map(|id| id.as_str()).collect();
        let resolved = db.get_symbols_by_ids(&tool.symbol_ids)?;
        let resolved_ids: HashSet<&str> =
            resolved.iter().map(|symbol| symbol.id.as_str()).collect();

        let mut missing_ids: Vec<String> = requested_ids
            .difference(&resolved_ids)
            .map(|id| (*id).to_string())
            .collect();
        missing_ids.sort();
        if !missing_ids.is_empty() {
            return Err(anyhow!(
                "Unknown symbol ids for blast_radius: {}",
                missing_ids.join(", ")
            ));
        }

        seed_symbols.extend(resolved);
    }

    for file_path in &tool.file_paths {
        changed_files.push(file_path.clone());
        seed_symbols.extend(db.get_symbols_for_file(file_path)?);
    }

    if let (Some(from_revision), Some(to_revision)) = (tool.from_revision, tool.to_revision) {
        let changes =
            db.get_revision_file_changes_between(workspace_id, from_revision, to_revision)?;
        for change in changes {
            match change.change_kind {
                RevisionChangeKind::Deleted => deleted_files.push(change.file_path),
                RevisionChangeKind::Added | RevisionChangeKind::Modified => {
                    changed_files.push(change.file_path.clone());
                    seed_symbols.extend(db.get_symbols_for_file(&change.file_path)?);
                }
            }
        }
    }

    let mut seen_symbol_ids = HashSet::new();
    seed_symbols.retain(|symbol| seen_symbol_ids.insert(symbol.id.clone()));

    changed_files.sort();
    changed_files.dedup();
    deleted_files.sort();
    deleted_files.dedup();

    if seed_symbols.is_empty() && deleted_files.is_empty() {
        return Err(anyhow!(
            "No indexed symbols found for the requested blast_radius seeds."
        ));
    }

    Ok(SeedContext {
        seed_symbols,
        changed_files,
        deleted_files,
    })
}

fn validate_request(tool: &BlastRadiusTool) -> Result<()> {
    let has_symbol_seeds = !tool.symbol_ids.is_empty();
    let has_file_seeds = !tool.file_paths.is_empty();
    let has_revision_seeds = tool.from_revision.is_some() || tool.to_revision.is_some();

    if !has_symbol_seeds && !has_file_seeds && !has_revision_seeds {
        return Err(anyhow!(
            "blast_radius requires symbol_ids, file_paths, or a revision range."
        ));
    }

    if tool.from_revision.is_some() ^ tool.to_revision.is_some() {
        return Err(anyhow!(
            "blast_radius requires from_revision and to_revision together."
        ));
    }

    if let (Some(from_revision), Some(to_revision)) = (tool.from_revision, tool.to_revision) {
        if from_revision >= to_revision {
            return Err(anyhow!(
                "blast_radius requires from_revision < to_revision."
            ));
        }
    }

    Ok(())
}
