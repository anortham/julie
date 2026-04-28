use std::collections::{HashMap, HashSet};

use anyhow::Result;

use super::graph::GraphExpansion;
use super::scoring::Pivot;
use crate::database::SymbolDatabase;
use crate::extractors::base::Symbol;
use crate::search::scoring::is_test_path;
use crate::tools::shared::NOISE_CALLEE_NAMES;

/// Pre-fetched data for building pivot entries without N+1 DB queries.
struct PivotBatchData {
    full_symbols: HashMap<String, Symbol>,
    related_symbols: HashMap<String, (String, String)>,
    incoming_by_pivot: HashMap<String, Vec<String>>,
    outgoing_by_pivot: HashMap<String, Vec<String>>,
}

/// Batch-fetch all data needed to build pivot entries.
fn fetch_pivot_batch_data(
    pivot_ids: &[String],
    expansion: &GraphExpansion,
    db: &SymbolDatabase,
) -> Result<PivotBatchData> {
    let full_symbols: HashMap<String, Symbol> = db
        .get_symbols_by_ids(pivot_ids)?
        .into_iter()
        .map(|symbol| (symbol.id.clone(), symbol))
        .collect();

    let incoming_rels = db.get_relationships_to_symbols(pivot_ids)?;
    let outgoing_rels = db.get_outgoing_relationships_for_symbols(pivot_ids)?;
    let pivot_id_set: HashSet<String> = pivot_ids.iter().cloned().collect();
    let pivot_symbols: Vec<Symbol> = pivot_ids
        .iter()
        .filter_map(|pivot_id| full_symbols.get(pivot_id).cloned())
        .collect();
    let identifier_edges = crate::database::impact_graph::identifier_incoming_edges(
        db,
        &pivot_symbols,
        &pivot_id_set,
    )?;

    let mut related_ids: Vec<String> = incoming_rels
        .iter()
        .map(|rel| rel.from_symbol_id.clone())
        .collect();
    related_ids.extend(outgoing_rels.iter().map(|rel| rel.to_symbol_id.clone()));
    related_ids.extend(
        identifier_edges
            .iter()
            .map(|edge| edge.container_id.clone()),
    );
    related_ids.sort();
    related_ids.dedup();

    let mut related_symbols: HashMap<String, (String, String)> = expansion
        .neighbors
        .iter()
        .map(|neighbor| {
            (
                neighbor.symbol.id.clone(),
                (
                    neighbor.symbol.name.clone(),
                    neighbor.symbol.file_path.clone(),
                ),
            )
        })
        .collect();

    if !related_ids.is_empty() {
        for symbol in db.get_symbols_by_ids(&related_ids)? {
            related_symbols
                .entry(symbol.id.clone())
                .or_insert((symbol.name, symbol.file_path));
        }
    }

    let mut incoming_by_pivot: HashMap<String, Vec<String>> = HashMap::new();
    for rel in &incoming_rels {
        incoming_by_pivot
            .entry(rel.to_symbol_id.clone())
            .or_default()
            .push(rel.from_symbol_id.clone());
    }
    for edge in &identifier_edges {
        if let Some(target_symbol_id) = &edge.target_symbol_id {
            incoming_by_pivot
                .entry(target_symbol_id.clone())
                .or_default()
                .push(edge.container_id.clone());
        }
    }

    let mut outgoing_by_pivot: HashMap<String, Vec<String>> = HashMap::new();
    for rel in &outgoing_rels {
        outgoing_by_pivot
            .entry(rel.from_symbol_id.clone())
            .or_default()
            .push(rel.to_symbol_id.clone());
    }

    Ok(PivotBatchData {
        full_symbols,
        related_symbols,
        incoming_by_pivot,
        outgoing_by_pivot,
    })
}

/// Build PivotEntry structs from pivots, selecting content based on PivotMode.
pub(crate) fn build_pivot_entries(
    pivots: &[Pivot],
    expansion: &GraphExpansion,
    db: &SymbolDatabase,
    allocation: &super::allocation::Allocation,
    reference_scores: &HashMap<String, f64>,
) -> Result<Vec<super::formatting::PivotEntry>> {
    use super::allocation::PivotMode;
    use super::content::{abbreviate_code, truncate_to_token_budget_with_hint};
    use super::formatting::PivotEntry;

    let pivot_ids: Vec<String> = pivots.iter().map(|pivot| pivot.result.id.clone()).collect();
    let per_pivot_tokens = allocation.pivot_tokens as usize / pivots.len().max(1);
    let batch = fetch_pivot_batch_data(&pivot_ids, expansion, db)?;

    let mut entries = Vec::with_capacity(pivots.len());
    for pivot in pivots {
        let content = match allocation.pivot_mode {
            PivotMode::FullBody => batch
                .full_symbols
                .get(&pivot.result.id)
                .and_then(|symbol| symbol.code_context.as_deref())
                .map(str::to_string)
                .unwrap_or_else(|| pivot.result.signature.clone()),
            PivotMode::SignatureAndKey => batch
                .full_symbols
                .get(&pivot.result.id)
                .and_then(|symbol| symbol.code_context.as_deref())
                .map(abbreviate_code)
                .unwrap_or_else(|| pivot.result.signature.clone()),
            PivotMode::SignatureOnly => pivot.result.signature.clone(),
        };

        let content = truncate_to_token_budget_with_hint(
            &content,
            per_pivot_tokens,
            Some(&pivot.result.name),
        );
        let (incoming_names, outgoing_names) = get_pivot_relationship_names_batched(
            &pivot.result.id,
            &batch.incoming_by_pivot,
            &batch.outgoing_by_pivot,
            &batch.related_symbols,
        );
        let reference_score = reference_scores
            .get(&pivot.result.id)
            .copied()
            .unwrap_or(0.0);
        let test_quality_label = batch
            .full_symbols
            .get(&pivot.result.id)
            .and_then(|symbol| symbol.metadata.as_ref())
            .and_then(|metadata| metadata.get("test_quality"))
            .and_then(|quality| quality.get("quality_tier"))
            .and_then(|tier| tier.as_str())
            .map(String::from);

        entries.push(PivotEntry {
            name: pivot.result.name.clone(),
            file_path: pivot.result.file_path.clone(),
            start_line: pivot.result.start_line,
            kind: pivot.result.kind.clone(),
            reference_score,
            content,
            incoming_names,
            outgoing_names,
            test_quality_label,
        });
    }

    Ok(entries)
}

fn get_pivot_relationship_names_batched(
    pivot_id: &str,
    incoming_by_pivot: &HashMap<String, Vec<String>>,
    outgoing_by_pivot: &HashMap<String, Vec<String>>,
    symbols_by_id: &HashMap<String, (String, String)>,
) -> (Vec<String>, Vec<String>) {
    let should_include = |name: &str, path: &str| -> bool {
        !NOISE_CALLEE_NAMES.contains(&name) && !is_test_path(path)
    };

    let mut incoming_names = Vec::new();
    if let Some(incoming_ids) = incoming_by_pivot.get(pivot_id) {
        for related_id in incoming_ids {
            if let Some((name, path)) = symbols_by_id.get(related_id) {
                if should_include(name, path) {
                    incoming_names.push(name.clone());
                }
            }
        }
    }

    let mut outgoing_names = Vec::new();
    if let Some(outgoing_ids) = outgoing_by_pivot.get(pivot_id) {
        for related_id in outgoing_ids {
            if let Some((name, path)) = symbols_by_id.get(related_id) {
                if should_include(name, path) {
                    outgoing_names.push(name.clone());
                }
            }
        }
    }

    (incoming_names, outgoing_names)
}

const MAX_NEIGHBOR_ENTRIES: usize = 200;

#[derive(Default)]
pub(crate) struct NeighborBuildOutput {
    pub entries: Vec<super::formatting::NeighborEntry>,
    pub overflow_entries: Vec<super::formatting::NeighborEntry>,
}

/// Build NeighborEntry structs from graph expansion results, filtering noise.
pub(crate) fn build_neighbor_entries(
    expansion: &GraphExpansion,
    neighbor_token_budget: u32,
    prefer_tests: bool,
) -> NeighborBuildOutput {
    use super::formatting::NeighborEntry;

    let char_budget = ((neighbor_token_budget as usize) * 4).max(800);
    let mut estimated_chars = 0usize;
    let mut entries = Vec::new();
    let mut overflow_entries = Vec::new();

    for neighbor in expansion
        .neighbors
        .iter()
        .filter(|neighbor| !NOISE_CALLEE_NAMES.contains(&neighbor.symbol.name.as_str()))
        .filter(|neighbor| prefer_tests || !is_test_path(&neighbor.symbol.file_path))
        .take(MAX_NEIGHBOR_ENTRIES)
    {
        let entry_char_estimate = neighbor.symbol.name.len()
            + neighbor.symbol.file_path.len()
            + 10
            + neighbor
                .symbol
                .signature
                .as_ref()
                .map(|signature| signature.len())
                .unwrap_or(0)
            + neighbor
                .symbol
                .doc_comment
                .as_ref()
                .map(|doc| doc.len().min(120))
                .unwrap_or(0);

        let entry = NeighborEntry {
            name: neighbor.symbol.name.clone(),
            file_path: neighbor.symbol.file_path.clone(),
            start_line: neighbor.symbol.start_line,
            kind: format!("{:?}", neighbor.symbol.kind).to_lowercase(),
            signature: neighbor.symbol.signature.clone(),
            doc_summary: neighbor
                .symbol
                .doc_comment
                .as_ref()
                .map(|doc| crate::embeddings::metadata::first_sentence(doc))
                .filter(|summary| !summary.is_empty()),
        };

        if estimated_chars + entry_char_estimate > char_budget && !entries.is_empty() {
            overflow_entries.push(entry);
            continue;
        }

        estimated_chars += entry_char_estimate;
        entries.push(entry);
    }

    NeighborBuildOutput {
        entries,
        overflow_entries,
    }
}
