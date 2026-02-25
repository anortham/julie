//! Main pipeline: search -> rank -> expand -> allocate -> format

use std::collections::{HashMap, HashSet};

use anyhow::Result;

use super::GetContextTool;
pub use super::scoring::{select_pivots, Pivot};
use super::scoring::is_test_path;
use crate::database::SymbolDatabase;
use crate::extractors::base::{RelationshipKind, Symbol};
use crate::handler::JulieServerHandler;

/// Direction of a neighbor relative to the pivot symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NeighborDirection {
    /// Symbol calls/uses/imports the pivot (incoming relationship).
    Incoming,
    /// Pivot calls/uses/imports this symbol (outgoing relationship).
    Outgoing,
}

/// A neighbor symbol discovered through graph expansion from a pivot.
pub struct Neighbor {
    pub symbol: Symbol,
    pub relationship_kind: RelationshipKind,
    pub direction: NeighborDirection,
    pub reference_score: f64,
}

/// Result of graph expansion — deduplicated neighbors sorted by reference_score.
pub struct GraphExpansion {
    pub neighbors: Vec<Neighbor>,
}

/// Expand pivots into a graph of related neighbor symbols.
///
/// For each pivot:
/// 1. Fetch incoming relationships (callers, implementors, importers)
/// 2. Fetch outgoing relationships (callees, types used, modules imported)
/// 3. Deduplicate neighbors across all pivots (each symbol appears once)
/// 4. Exclude pivot symbols themselves from the neighbor list
/// 5. Look up neighbor metadata and reference_scores
/// 6. Sort by reference_score descending (most important first)
pub fn expand_graph(pivots: &[Pivot], db: &SymbolDatabase) -> Result<GraphExpansion> {
    if pivots.is_empty() {
        return Ok(GraphExpansion {
            neighbors: Vec::new(),
        });
    }

    // Collect pivot IDs for exclusion
    let pivot_ids: HashSet<&str> = pivots.iter().map(|p| p.result.id.as_str()).collect();

    // For each neighbor, track: (relationship_kind, direction) — first seen wins
    let mut neighbor_map: HashMap<String, (RelationshipKind, NeighborDirection)> = HashMap::new();

    for pivot in pivots {
        let symbol_id = &pivot.result.id;

        // Incoming: other symbols that reference this pivot
        let incoming = db.get_relationships_to_symbol(symbol_id)?;
        for rel in incoming {
            let neighbor_id = &rel.from_symbol_id;
            if !pivot_ids.contains(neighbor_id.as_str()) {
                neighbor_map
                    .entry(neighbor_id.clone())
                    .or_insert_with(|| (rel.kind, NeighborDirection::Incoming));
            }
        }

        // Outgoing: symbols that this pivot references
        let outgoing = db.get_outgoing_relationships(symbol_id)?;
        for rel in outgoing {
            let neighbor_id = &rel.to_symbol_id;
            if !pivot_ids.contains(neighbor_id.as_str()) {
                neighbor_map
                    .entry(neighbor_id.clone())
                    .or_insert_with(|| (rel.kind, NeighborDirection::Outgoing));
            }
        }
    }

    if neighbor_map.is_empty() {
        return Ok(GraphExpansion {
            neighbors: Vec::new(),
        });
    }

    // Batch-fetch symbol metadata
    let neighbor_ids: Vec<String> = neighbor_map.keys().cloned().collect();
    let symbols = db.get_symbols_by_ids(&neighbor_ids)?;

    // Batch-fetch reference scores
    let id_refs: Vec<&str> = neighbor_ids.iter().map(|s| s.as_str()).collect();
    let ref_scores = db.get_reference_scores(&id_refs)?;

    // Build neighbors with metadata
    let mut neighbors: Vec<Neighbor> = symbols
        .into_iter()
        .filter_map(|sym| {
            let (kind, direction) = neighbor_map.remove(&sym.id)?;
            let reference_score = ref_scores.get(&sym.id).copied().unwrap_or(0.0);
            Some(Neighbor {
                symbol: sym,
                relationship_kind: kind,
                direction,
                reference_score,
            })
        })
        .collect();

    // Sort by reference_score descending
    neighbors.sort_by(|a, b| {
        b.reference_score
            .partial_cmp(&a.reference_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(GraphExpansion { neighbors })
}

/// Run the full get_context pipeline: search → rank → expand → allocate → format.
///
/// This is the testable core — takes raw DB and SearchIndex references,
/// independent of the MCP handler. Called by `run()` inside `spawn_blocking`.
pub fn run_pipeline(
    query: &str,
    max_tokens: Option<u32>,
    language: Option<String>,
    file_pattern: Option<String>,
    db: &SymbolDatabase,
    search_index: &crate::search::SearchIndex,
) -> Result<String> {
    use super::allocation::TokenBudget;
    use super::formatting::{format_context, ContextData};
    use crate::search::index::SearchFilter;

    // 1. Search for relevant symbols
    let filter = SearchFilter {
        language,
        kind: None,
        file_pattern,
    };
    let search_results = search_index.search_symbols(query, &filter, 30)?;

    if search_results.results.is_empty() {
        return Ok(format!(
            "\u{2550}\u{2550}\u{2550} Context: \"{}\" \u{2550}\u{2550}\u{2550}\nNo relevant symbols found.",
            query
        ));
    }

    // 2. Get reference scores for centrality-weighted ranking
    let result_ids: Vec<&str> = search_results.results.iter().map(|r| r.id.as_str()).collect();
    let ref_scores = db.get_reference_scores(&result_ids)?;

    // 3. Select pivots using centrality-weighted scoring
    let pivots = select_pivots(search_results.results, &ref_scores);

    // 4. Expand graph from pivots
    let expansion = expand_graph(&pivots, db)?;

    // 5. Allocate token budget
    let budget = match max_tokens {
        Some(tokens) => TokenBudget::new(tokens),
        None => TokenBudget::adaptive(pivots.len()),
    };
    let allocation = budget.allocate(pivots.len(), expansion.neighbors.len());

    // 6. Get reference scores for pivots (reuse batch query, not per-pivot)
    let pivot_ids: Vec<&str> = pivots.iter().map(|p| p.result.id.as_str()).collect();
    let pivot_ref_scores = db.get_reference_scores(&pivot_ids)?;

    // 7. Build PivotEntries
    let pivot_entries = build_pivot_entries(&pivots, &expansion, db, &allocation, &pivot_ref_scores)?;

    // 8. Build NeighborEntries
    let neighbor_entries = build_neighbor_entries(&expansion);

    // 9. Format and return
    let context_data = ContextData {
        query: query.to_string(),
        pivots: pivot_entries,
        neighbors: neighbor_entries,
        allocation,
    };

    Ok(format_context(&context_data))
}

/// Build PivotEntry structs from pivots, selecting content based on PivotMode.
fn build_pivot_entries(
    pivots: &[Pivot],
    expansion: &GraphExpansion,
    db: &SymbolDatabase,
    allocation: &super::allocation::Allocation,
    reference_scores: &HashMap<String, f64>,
) -> Result<Vec<super::formatting::PivotEntry>> {
    use super::allocation::PivotMode;
    use super::formatting::PivotEntry;

    let mut entries = Vec::with_capacity(pivots.len());

    for pivot in pivots {
        // Determine content based on PivotMode
        let content = match allocation.pivot_mode {
            PivotMode::FullBody => {
                // Get full symbol from DB for code_context
                if let Ok(Some(full_symbol)) = db.get_symbol_by_id(&pivot.result.id) {
                    full_symbol
                        .code_context
                        .unwrap_or_else(|| pivot.result.signature.clone())
                } else {
                    pivot.result.signature.clone()
                }
            }
            PivotMode::SignatureAndKey => {
                // Get code_context, take first 5 + last 5 lines
                if let Ok(Some(full_symbol)) = db.get_symbol_by_id(&pivot.result.id) {
                    if let Some(code) = full_symbol.code_context {
                        abbreviate_code(&code)
                    } else {
                        pivot.result.signature.clone()
                    }
                } else {
                    pivot.result.signature.clone()
                }
            }
            PivotMode::SignatureOnly => pivot.result.signature.clone(),
        };

        // Get incoming/outgoing relationship names for this pivot
        let (incoming_names, outgoing_names) = get_pivot_relationship_names(pivot, expansion, db);

        // Use pre-fetched reference scores (no redundant DB query)
        let ref_score = reference_scores
            .get(&pivot.result.id)
            .copied()
            .unwrap_or(0.0);

        entries.push(PivotEntry {
            name: pivot.result.name.clone(),
            file_path: pivot.result.file_path.clone(),
            start_line: pivot.result.start_line,
            kind: pivot.result.kind.clone(),
            reference_score: ref_score,
            content,
            incoming_names,
            outgoing_names,
        });
    }

    Ok(entries)
}

/// Abbreviate a code body: first 5 lines + "..." + last 5 lines.
/// Returns the full code if it has 12 or fewer lines (not worth abbreviating).
fn abbreviate_code(code: &str) -> String {
    let lines: Vec<&str> = code.lines().collect();
    if lines.len() <= 12 {
        return code.to_string();
    }
    let mut out = String::new();
    for line in &lines[..5] {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("    // ... (abbreviated)\n");
    for (i, line) in lines[lines.len() - 5..].iter().enumerate() {
        out.push_str(line);
        if i < 4 {
            out.push('\n');
        }
    }
    out
}

/// Get incoming and outgoing relationship names for a pivot symbol.
///
/// Uses the neighbor list from graph expansion where possible,
/// and falls back to direct DB queries for callers/callees of this pivot.
fn get_pivot_relationship_names(
    pivot: &Pivot,
    expansion: &GraphExpansion,
    db: &SymbolDatabase,
) -> (Vec<String>, Vec<String>) {
    let pivot_id = &pivot.result.id;

    // Collect names from expansion neighbors that relate to this pivot
    // For incoming: neighbors with Incoming direction whose relationship targets this pivot
    // For outgoing: neighbors with Outgoing direction whose relationship sources from this pivot
    //
    // Since expand_graph deduplicates across pivots and doesn't track per-pivot associations,
    // we query the DB directly for accuracy.
    let mut incoming_names = Vec::new();
    let mut outgoing_names = Vec::new();

    // Helper: resolve symbol name and file path from neighbor list or DB
    let resolve_symbol = |id: &str| -> Option<(String, String)> {
        if let Some(n) = expansion.neighbors.iter().find(|n| n.symbol.id == id) {
            Some((n.symbol.name.clone(), n.symbol.file_path.clone()))
        } else if let Ok(Some(sym)) = db.get_symbol_by_id(id) {
            let path = sym.file_path.clone();
            Some((sym.name, path))
        } else {
            None
        }
    };

    // Filter: skip noise trait methods and test file symbols
    let should_include = |name: &str, path: &str| -> bool {
        !NOISE_NEIGHBOR_NAMES.contains(&name) && !is_test_path(path)
    };

    // Incoming callers
    if let Ok(incoming_rels) = db.get_relationships_to_symbol(pivot_id) {
        for rel in &incoming_rels {
            if let Some((name, path)) = resolve_symbol(&rel.from_symbol_id) {
                if should_include(&name, &path) {
                    incoming_names.push(name);
                }
            }
        }
    }

    // Outgoing callees
    if let Ok(outgoing_rels) = db.get_outgoing_relationships(pivot_id) {
        for rel in &outgoing_rels {
            if let Some((name, path)) = resolve_symbol(&rel.to_symbol_id) {
                if should_include(&name, &path) {
                    outgoing_names.push(name);
                }
            }
        }
    }

    (incoming_names, outgoing_names)
}

/// Common trait method names that provide no useful context as neighbors.
/// These are boilerplate implementations that appear everywhere but tell you nothing
/// about the actual code architecture.
const NOISE_NEIGHBOR_NAMES: &[&str] = &[
    "clone", "to_string", "fmt", "eq", "ne", "cmp", "partial_cmp",
    "hash", "drop", "deref", "deref_mut", "is_empty", "len",
];

/// Build NeighborEntry structs from graph expansion results, filtering noise.
/// Filters out: common trait methods (clone, fmt, etc.) and test file symbols.
fn build_neighbor_entries(expansion: &GraphExpansion) -> Vec<super::formatting::NeighborEntry> {
    use super::formatting::NeighborEntry;

    expansion
        .neighbors
        .iter()
        .filter(|neighbor| !NOISE_NEIGHBOR_NAMES.contains(&neighbor.symbol.name.as_str()))
        .filter(|neighbor| !is_test_path(&neighbor.symbol.file_path))
        .map(|neighbor| NeighborEntry {
            name: neighbor.symbol.name.clone(),
            file_path: neighbor.symbol.file_path.clone(),
            start_line: neighbor.symbol.start_line,
            kind: format!("{:?}", neighbor.symbol.kind).to_lowercase(),
            signature: neighbor.symbol.signature.clone(),
            doc_summary: neighbor.symbol.doc_comment.as_ref().map(|d| {
                d.lines().next().unwrap_or("").to_string()
            }),
        })
        .collect()
}

/// Handler entry point: extracts DB and SearchIndex from handler, delegates to run_pipeline.
///
/// Currently only supports the primary workspace. Reference workspace support
/// can be added later by opening separate DB/Tantivy files (same pattern as text_search_impl).
pub async fn run(tool: &GetContextTool, handler: &JulieServerHandler) -> Result<String> {
    // Validate workspace parameter — only primary is supported for now
    let workspace_param = tool.workspace.as_deref().unwrap_or("primary");
    if workspace_param != "primary" {
        anyhow::bail!(
            "get_context currently only supports the primary workspace. \
             Use workspace=\"primary\" (default) or omit the parameter."
        );
    }

    let workspace = handler
        .get_workspace()
        .await?
        .ok_or_else(|| anyhow::anyhow!("No workspace initialized"))?;

    let search_index = workspace
        .search_index
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Search index not initialized"))?
        .clone();

    let db = workspace
        .db
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Database not initialized"))?
        .clone();

    let query = tool.query.clone();
    let max_tokens = tool.max_tokens;
    let language = tool.language.clone();
    let file_pattern = tool.file_pattern.clone();

    // Use spawn_blocking since Tantivy and SQLite use std::sync::Mutex
    let result = tokio::task::spawn_blocking(move || -> Result<String> {
        let index = search_index.lock().unwrap();
        let db_guard = db.lock().unwrap();
        run_pipeline(&query, max_tokens, language, file_pattern, &db_guard, &index)
    })
    .await??;

    Ok(result)
}
