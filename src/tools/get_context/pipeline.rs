//! Main pipeline: search -> rank -> expand -> allocate -> format

use anyhow::Result;
use tracing::debug;

use super::GetContextTool;
#[cfg(test)]
pub(crate) use super::content::truncate_to_token_budget;
pub use super::graph::{
    GraphExpansion, Neighbor, NeighborDirection, expand_graph, expand_graph_from_symbols,
};
pub use super::scoring::{Pivot, select_pivots};
use super::second_hop::{merge_expansions, select_second_hop_seeds, should_expand_second_hop};
use super::task_signals::{
    TaskSignals, hydrate_failing_test_links, merge_task_signal_seed_results,
};
use crate::database::SymbolDatabase;
use crate::handler::JulieServerHandler;
use crate::tools::navigation::resolution::{WorkspaceTarget, resolve_workspace_filter};
use crate::tools::spillover::{SpilloverFormat, SpilloverStore};

/// Run the full get_context pipeline: search → rank → expand → allocate → format.
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
    run_pipeline_with_options(
        query,
        max_tokens,
        language,
        file_pattern,
        format,
        db,
        search_index,
        embedding_provider,
        None,
        None,
        None,
    )
}

pub fn run_pipeline_with_options(
    query: &str,
    max_tokens: Option<u32>,
    language: Option<String>,
    file_pattern: Option<String>,
    format: Option<String>,
    db: &SymbolDatabase,
    search_index: &crate::search::SearchIndex,
    embedding_provider: Option<&dyn crate::embeddings::EmbeddingProvider>,
    task_signals: Option<&TaskSignals>,
    spillover_store: Option<&SpilloverStore>,
    spillover_session: Option<(&str, SpilloverFormat)>,
) -> Result<String> {
    use super::allocation::TokenBudget;
    use super::entries::{build_neighbor_entries, build_pivot_entries};
    use super::formatting::{ContextData, format_context_with_mode, format_neighbor_rows};
    use crate::search::index::SearchFilter;

    let mut resolved_signals = task_signals.cloned().unwrap_or_default();
    hydrate_failing_test_links(db, &mut resolved_signals)?;

    let filter = SearchFilter {
        language,
        kind: None,
        file_pattern,
        exclude_tests: false,
    };
    let profile = crate::search::weights::SearchWeightProfile::get_context();
    let mut search_results = crate::search::hybrid::hybrid_search(
        query,
        &filter,
        30,
        search_index,
        db,
        embedding_provider,
        Some(profile),
    )?;
    merge_task_signal_seed_results(&mut search_results.results, db, &filter, &resolved_signals)?;
    let output_format = super::formatting::OutputFormat::from_option(format.as_deref());

    if search_results.results.is_empty() {
        let empty_data = ContextData {
            query: query.to_string(),
            pivots: vec![],
            neighbors: vec![],
            allocation: TokenBudget::new(0).allocate(0, 0),
            spillover_handle: None,
        };
        return Ok(format_context_with_mode(&empty_data, output_format));
    }

    let result_ids: Vec<&str> = search_results
        .results
        .iter()
        .map(|result| result.id.as_str())
        .collect();
    let ref_scores = db.get_reference_scores(&result_ids)?;
    let pivots = if resolved_signals.is_empty() {
        super::scoring::select_pivots_with_code_fallback_for_query(
            query,
            search_results.results,
            &ref_scores,
        )
    } else {
        super::scoring::select_pivots_with_task_signals_for_query(
            query,
            search_results.results,
            &ref_scores,
            &resolved_signals,
        )
    };

    let expansion = expand_graph(&pivots, db)?;
    let pivot_id_set: std::collections::HashSet<&str> = pivots
        .iter()
        .map(|pivot| pivot.result.id.as_str())
        .collect();
    let mut expansion = if should_expand_second_hop(&resolved_signals, &expansion) {
        let second_hop_seeds = select_second_hop_seeds(&expansion, resolved_signals.prefer_tests);
        if second_hop_seeds.is_empty() {
            expansion
        } else {
            merge_expansions(expansion, expand_graph_from_symbols(&second_hop_seeds, db)?)
        }
    } else {
        expansion
    };
    expansion
        .neighbors
        .retain(|neighbor| !pivot_id_set.contains(neighbor.symbol.id.as_str()));

    let budget = match max_tokens {
        Some(tokens) => TokenBudget::new(tokens),
        None => TokenBudget::adaptive(pivots.len()),
    };
    let allocation = budget.allocate(pivots.len(), expansion.neighbors.len());

    let pivot_ids: Vec<&str> = pivots
        .iter()
        .map(|pivot| pivot.result.id.as_str())
        .collect();
    let pivot_ref_scores = db.get_reference_scores(&pivot_ids)?;
    let pivot_entries =
        build_pivot_entries(&pivots, &expansion, db, &allocation, &pivot_ref_scores)?;

    let neighbor_output = build_neighbor_entries(
        &expansion,
        allocation.neighbor_tokens,
        resolved_signals.prefer_tests,
    );
    let spillover_handle = if !neighbor_output.overflow_entries.is_empty() {
        spillover_store.zip(spillover_session).and_then(
            |(store, (session_id, spillover_format))| {
                store.store_rows(
                    session_id,
                    "gc",
                    "get_context overflow",
                    format_neighbor_rows(
                        &neighbor_output.overflow_entries,
                        &allocation.neighbor_mode,
                    ),
                    0,
                    10,
                    spillover_format,
                )
            },
        )
    } else {
        None
    };

    let context_data = ContextData {
        query: query.to_string(),
        pivots: pivot_entries,
        neighbors: neighbor_output.entries,
        allocation,
        spillover_handle,
    };

    Ok(format_context_with_mode(&context_data, output_format))
}

/// Handler entry point: extracts DB and SearchIndex from handler, delegates to run_pipeline.
pub async fn run(tool: &GetContextTool, handler: &JulieServerHandler) -> Result<String> {
    let workspace_target = resolve_workspace_filter(tool.workspace.as_deref(), handler).await?;

    let query = tool.query.clone();
    let max_tokens = tool.max_tokens;
    let language = tool.language.clone();
    let file_pattern = tool.file_pattern.clone();
    let format = tool.format.clone();
    let task_signals = TaskSignals::from_tool(tool);
    let spillover_store = handler.spillover_store.clone();
    let session_id = handler.session_metrics.session_id.clone();
    let spillover_format = SpilloverFormat::from_option(tool.format.as_deref());

    match workspace_target {
        WorkspaceTarget::Target(target_workspace_id) => {
            debug!("get_context: using workspace {}", target_workspace_id);

            let db_arc = handler
                .get_database_for_workspace(&target_workspace_id)
                .await?;
            let si_arc = handler
                .get_search_index_for_workspace(&target_workspace_id)
                .await?;
            let embedding_provider = handler.embedding_provider().await;

            let result = tokio::task::spawn_blocking(move || -> Result<String> {
                let si = si_arc.ok_or_else(|| {
                    anyhow::anyhow!(
                        "No search index for workspace. Run manage_workspace(operation=\"refresh\") first."
                    )
                })?;
                let index = si
                    .lock()
                    .map_err(|e| anyhow::anyhow!("Search index lock error: {}", e))?;
                let db = db_arc
                    .lock()
                    .map_err(|e| anyhow::anyhow!("Database lock error: {}", e))?;
                run_pipeline_with_options(
                    &query,
                    max_tokens,
                    language,
                    file_pattern,
                    format,
                    &db,
                    &index,
                    embedding_provider.as_deref(),
                    Some(&task_signals),
                    Some(&spillover_store),
                    Some((&session_id, spillover_format)),
                )
            })
            .await
            .map_err(|e| anyhow::anyhow!("spawn_blocking error: {}", e))??;

            Ok(result)
        }
        WorkspaceTarget::Primary => {
            let (db, search_index) = handler.primary_database_and_search_index().await?;
            let embedding_provider = handler.embedding_provider().await;

            let result = tokio::task::spawn_blocking(move || -> Result<String> {
                let index = search_index
                    .lock()
                    .map_err(|e| anyhow::anyhow!("Search index lock error: {}", e))?;
                let db_guard = db
                    .lock()
                    .map_err(|e| anyhow::anyhow!("Database lock error: {}", e))?;
                run_pipeline_with_options(
                    &query,
                    max_tokens,
                    language,
                    file_pattern,
                    format,
                    &db_guard,
                    &index,
                    embedding_provider.as_deref(),
                    Some(&task_signals),
                    Some(&spillover_store),
                    Some((&session_id, spillover_format)),
                )
            })
            .await??;

            Ok(result)
        }
    }
}
