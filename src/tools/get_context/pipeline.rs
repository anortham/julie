//! Main pipeline: search -> rank -> expand -> allocate -> format

use std::collections::{HashMap, HashSet};

use anyhow::Result;

use super::GetContextTool;
use super::content::abbreviate_code;
pub(crate) use super::content::truncate_to_token_budget;
pub use super::scoring::{Pivot, select_pivots};
use crate::search::scoring::is_test_path;
use tracing::debug;

use crate::database::SymbolDatabase;
use crate::extractors::base::{RelationshipKind, Symbol};
use crate::handler::JulieServerHandler;
use crate::tools::navigation::resolution::{WorkspaceTarget, resolve_workspace_filter};

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

    // Collect pivot IDs for exclusion and batched relationship queries
    let pivot_ids_vec: Vec<String> = pivots.iter().map(|p| p.result.id.clone()).collect();
    let pivot_ids: HashSet<String> = pivot_ids_vec.iter().cloned().collect();

    // For each neighbor, track: (relationship_kind, direction) — first seen wins
    let mut neighbor_map: HashMap<String, (RelationshipKind, NeighborDirection)> = HashMap::new();

    let incoming = db.get_relationships_to_symbols(&pivot_ids_vec)?;
    for rel in incoming {
        let neighbor_id = &rel.from_symbol_id;
        if !pivot_ids.contains(neighbor_id) {
            neighbor_map
                .entry(neighbor_id.clone())
                .or_insert_with(|| (rel.kind, NeighborDirection::Incoming));
        }
    }

    let outgoing = db.get_outgoing_relationships_for_symbols(&pivot_ids_vec)?;
    for rel in outgoing {
        let neighbor_id = &rel.to_symbol_id;
        if !pivot_ids.contains(neighbor_id) {
            neighbor_map
                .entry(neighbor_id.clone())
                .or_insert_with(|| (rel.kind, NeighborDirection::Outgoing));
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
    format: Option<String>,
    db: &SymbolDatabase,
    search_index: &crate::search::SearchIndex,
    embedding_provider: Option<&dyn crate::embeddings::EmbeddingProvider>,
) -> Result<String> {
    use super::allocation::TokenBudget;
    use super::formatting::{ContextData, format_context_with_mode};
    use crate::search::index::SearchFilter;

    // 1. Search for relevant symbols (hybrid: keyword + optional semantic)
    let filter = SearchFilter {
        language,
        kind: None,
        file_pattern,
    };
    let profile = crate::search::weights::SearchWeightProfile::get_context();
    let search_results = crate::search::hybrid::hybrid_search(
        query,
        &filter,
        30,
        search_index,
        db,
        embedding_provider,
        Some(profile),
    )?;

    if search_results.results.is_empty() {
        return Ok(format!(
            "\u{2550}\u{2550}\u{2550} Context: \"{}\" \u{2550}\u{2550}\u{2550}\nNo relevant symbols found.",
            query
        ));
    }

    // 2. Get reference scores for centrality-weighted ranking
    let result_ids: Vec<&str> = search_results
        .results
        .iter()
        .map(|r| r.id.as_str())
        .collect();
    let ref_scores = db.get_reference_scores(&result_ids)?;

    // 3. Select pivots using centrality-weighted scoring
    let pivots =
        super::scoring::select_pivots_with_code_fallback(search_results.results, &ref_scores);

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
    let pivot_entries =
        build_pivot_entries(&pivots, &expansion, db, &allocation, &pivot_ref_scores)?;

    // 8. Build NeighborEntries
    let neighbor_entries = build_neighbor_entries(&expansion);

    // 9. Format and return
    let context_data = ContextData {
        query: query.to_string(),
        pivots: pivot_entries,
        neighbors: neighbor_entries,
        allocation,
    };

    Ok(format_context_with_mode(
        &context_data,
        super::formatting::OutputFormat::from_option(format.as_deref()),
    ))
}

/// Pre-fetched data for building pivot entries without N+1 DB queries.
///
/// All data is loaded in batch before the per-pivot loop runs.
struct PivotBatchData {
    /// Full symbol bodies, keyed by symbol ID (empty if SignatureOnly mode).
    full_symbols: HashMap<String, Symbol>,
    /// Related symbol names/paths, keyed by symbol ID.
    related_symbols: HashMap<String, (String, String)>,
    /// Incoming relationship source IDs, grouped by target pivot ID.
    incoming_by_pivot: HashMap<String, Vec<String>>,
    /// Outgoing relationship target IDs, grouped by source pivot ID.
    outgoing_by_pivot: HashMap<String, Vec<String>>,
}

/// Batch-fetch all data needed to build pivot entries.
///
/// Replaces per-pivot N+1 queries with 3-4 batched DB calls.
fn fetch_pivot_batch_data(
    pivot_ids: &[String],
    expansion: &GraphExpansion,
    db: &SymbolDatabase,
    pivot_mode: &super::allocation::PivotMode,
) -> Result<PivotBatchData> {
    use super::allocation::PivotMode;

    // 1. Full symbol bodies (skip if we only need signatures)
    let full_symbols: HashMap<String, Symbol> = if matches!(pivot_mode, PivotMode::SignatureOnly) {
        HashMap::new()
    } else {
        db.get_symbols_by_ids(pivot_ids)?
            .into_iter()
            .map(|s| (s.id.clone(), s))
            .collect()
    };

    // 2. Relationships (batched)
    let incoming_rels = db.get_relationships_to_symbols(pivot_ids)?;
    let outgoing_rels = db.get_outgoing_relationships_for_symbols(pivot_ids)?;

    // 3. Resolve related symbol names — seed from expansion neighbors, fill gaps from DB
    let mut related_ids: Vec<String> = incoming_rels
        .iter()
        .map(|r| r.from_symbol_id.clone())
        .collect();
    related_ids.extend(outgoing_rels.iter().map(|r| r.to_symbol_id.clone()));
    related_ids.sort();
    related_ids.dedup();

    let mut related_symbols: HashMap<String, (String, String)> = expansion
        .neighbors
        .iter()
        .map(|n| {
            (
                n.symbol.id.clone(),
                (n.symbol.name.clone(), n.symbol.file_path.clone()),
            )
        })
        .collect();

    if !related_ids.is_empty() {
        for sym in db.get_symbols_by_ids(&related_ids)? {
            related_symbols
                .entry(sym.id.clone())
                .or_insert((sym.name, sym.file_path));
        }
    }

    // 4. Group relationships by pivot
    let mut incoming_by_pivot: HashMap<String, Vec<String>> = HashMap::new();
    for rel in &incoming_rels {
        incoming_by_pivot
            .entry(rel.to_symbol_id.clone())
            .or_default()
            .push(rel.from_symbol_id.clone());
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
fn build_pivot_entries(
    pivots: &[Pivot],
    expansion: &GraphExpansion,
    db: &SymbolDatabase,
    allocation: &super::allocation::Allocation,
    reference_scores: &HashMap<String, f64>,
) -> Result<Vec<super::formatting::PivotEntry>> {
    use super::allocation::PivotMode;
    use super::formatting::PivotEntry;

    let pivot_ids: Vec<String> = pivots.iter().map(|p| p.result.id.clone()).collect();
    let per_pivot_tokens = allocation.pivot_tokens as usize / pivots.len().max(1);
    let batch = fetch_pivot_batch_data(&pivot_ids, expansion, db, &allocation.pivot_mode)?;

    let mut entries = Vec::with_capacity(pivots.len());

    for pivot in pivots {
        let content = match allocation.pivot_mode {
            PivotMode::FullBody => {
                if let Some(full_symbol) = batch.full_symbols.get(&pivot.result.id) {
                    full_symbol
                        .code_context
                        .as_deref()
                        .map(str::to_string)
                        .unwrap_or_else(|| pivot.result.signature.clone())
                } else {
                    pivot.result.signature.clone()
                }
            }
            PivotMode::SignatureAndKey => {
                if let Some(full_symbol) = batch.full_symbols.get(&pivot.result.id) {
                    if let Some(code) = full_symbol.code_context.as_deref() {
                        abbreviate_code(code)
                    } else {
                        pivot.result.signature.clone()
                    }
                } else {
                    pivot.result.signature.clone()
                }
            }
            PivotMode::SignatureOnly => pivot.result.signature.clone(),
        };

        let content = truncate_to_token_budget(&content, per_pivot_tokens);

        let (incoming_names, outgoing_names) = get_pivot_relationship_names_batched(
            &pivot.result.id,
            &batch.incoming_by_pivot,
            &batch.outgoing_by_pivot,
            &batch.related_symbols,
        );

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

/// Get incoming and outgoing relationship names for a pivot symbol.
///
/// Uses the neighbor list from graph expansion where possible,
/// and falls back to direct DB queries for callers/callees of this pivot.
fn get_pivot_relationship_names_batched(
    pivot_id: &str,
    incoming_by_pivot: &HashMap<String, Vec<String>>,
    outgoing_by_pivot: &HashMap<String, Vec<String>>,
    symbols_by_id: &HashMap<String, (String, String)>,
) -> (Vec<String>, Vec<String>) {
    let mut incoming_names = Vec::new();
    let mut outgoing_names = Vec::new();

    // Filter: skip noise trait methods and test file symbols
    let should_include = |name: &str, path: &str| -> bool {
        !NOISE_NEIGHBOR_NAMES.contains(&name) && !is_test_path(path)
    };

    if let Some(incoming_ids) = incoming_by_pivot.get(pivot_id) {
        for related_id in incoming_ids {
            if let Some((name, path)) = symbols_by_id.get(related_id) {
                if should_include(&name, &path) {
                    incoming_names.push(name.clone());
                }
            }
        }
    }

    if let Some(outgoing_ids) = outgoing_by_pivot.get(pivot_id) {
        for related_id in outgoing_ids {
            if let Some((name, path)) = symbols_by_id.get(related_id) {
                if should_include(&name, &path) {
                    outgoing_names.push(name.clone());
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
    "clone",
    "to_string",
    "fmt",
    "eq",
    "ne",
    "cmp",
    "partial_cmp",
    "hash",
    "drop",
    "deref",
    "deref_mut",
    "is_empty",
    "len",
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
            doc_summary: neighbor
                .symbol
                .doc_comment
                .as_ref()
                .map(|d| crate::embeddings::metadata::first_sentence(d))
                .filter(|s| !s.is_empty()),
        })
        .collect()
}

/// Handler entry point: extracts DB and SearchIndex from handler, delegates to run_pipeline.
/// Supports both primary and reference workspaces.
pub async fn run(tool: &GetContextTool, handler: &JulieServerHandler) -> Result<String> {
    let workspace_target = resolve_workspace_filter(tool.workspace.as_deref(), handler).await?;

    let query = tool.query.clone();
    let max_tokens = tool.max_tokens;
    let language = tool.language.clone();
    let file_pattern = tool.file_pattern.clone();
    let format = tool.format.clone();

    match workspace_target {
        WorkspaceTarget::Reference(ref_workspace_id) => {
            // Reference workspace: use handler helpers for DB + SearchIndex access
            debug!(
                "get_context: using reference workspace {}",
                ref_workspace_id
            );

            // Get Arcs first (fast in daemon mode — just Arc clones)
            let db_arc = handler
                .get_database_for_workspace(&ref_workspace_id)
                .await?;
            let si_arc = handler
                .get_search_index_for_workspace(&ref_workspace_id)
                .await?;

            // Get embedding provider from primary workspace (no handler helper for this yet)
            let embedding_provider = if let Some(workspace) = handler.get_workspace().await? {
                workspace.embedding_provider.clone()
            } else {
                None
            };

            let result = tokio::task::spawn_blocking(move || -> Result<String> {
                let si = si_arc.ok_or_else(|| {
                    anyhow::anyhow!("No search index for reference workspace. Run manage_workspace(operation=\"refresh\") first.")
                })?;
                // Lock order: SearchIndex first, then DB (matches text_search.rs and Primary arm)
                let index = si
                    .lock()
                    .map_err(|e| anyhow::anyhow!("Search index lock error: {}", e))?;
                let db = db_arc
                    .lock()
                    .map_err(|e| anyhow::anyhow!("Database lock error: {}", e))?;
                run_pipeline(&query, max_tokens, language, file_pattern, format, &db, &index, embedding_provider.as_deref())
            })
            .await
            .map_err(|e| anyhow::anyhow!("spawn_blocking error: {}", e))??;

            Ok(result)
        }
        WorkspaceTarget::All => {
            let result = super::federated::run_federated(
                query, max_tokens, language, file_pattern, format, handler,
            ).await?;
            Ok(result)
        }
        WorkspaceTarget::Primary => {
            // Primary workspace: use shared DB and SearchIndex via Arc<Mutex>
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

            let embedding_provider = workspace.embedding_provider.clone();

            let result = tokio::task::spawn_blocking(move || -> Result<String> {
                let index = search_index.lock().unwrap();
                let db_guard = db.lock().unwrap();
                run_pipeline(
                    &query,
                    max_tokens,
                    language,
                    file_pattern,
                    format,
                    &db_guard,
                    &index,
                    embedding_provider.as_deref(),
                )
            })
            .await??;

            Ok(result)
        }
    }
}
