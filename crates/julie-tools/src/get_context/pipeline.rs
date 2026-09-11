//! Main pipeline: search -> rank -> expand -> allocate -> format

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use tracing::debug;

use super::GetContextTool;
#[cfg(any(test, feature = "test-support"))]
pub use super::content::truncate_to_token_budget;
pub use super::graph::{
    GraphExpansion, Neighbor, NeighborDirection, expand_graph, expand_graph_from_symbols,
};
pub use super::scoring::{Pivot, select_pivots};
use super::second_hop::{merge_expansions, select_second_hop_seeds, should_expand_second_hop};
use super::task_signals::{
    TaskSignals, hydrate_failing_test_links, merge_task_signal_seed_results,
};
use crate::navigation::resolution::WorkspaceTarget;
use julie_context::ToolContext;
use julie_core::embeddings_contract::{EmbeddingRequestBudget, SemanticMode, TaggedQueryEmbedding};
use julie_index::graph::{Graph, SymbolId};
use julie_index::snapshot::Snapshot;

/// Run the full get_context pipeline: search → rank → expand → allocate → format.
pub fn run_pipeline(
    query: &str,
    max_tokens: Option<u32>,
    language: Option<String>,
    file_pattern: Option<String>,
    format: Option<String>,
    snapshot: &Snapshot,
) -> Result<String> {
    run_pipeline_with_options(
        query,
        max_tokens,
        language,
        file_pattern,
        format,
        snapshot,
        None,
        None,
    )
}

/// Run the get_context pipeline with full option control.
///
/// `precomputed_embedding` is the query embedding computed before the search
/// (see [`julie_index::search::hybrid::compute_tagged_query_embedding_for_hybrid`])
/// so the sidecar round-trip (up to 30 s) never sits on the search path.
#[allow(clippy::too_many_arguments)]
pub fn run_pipeline_with_options(
    query: &str,
    max_tokens: Option<u32>,
    language: Option<String>,
    file_pattern: Option<String>,
    format: Option<String>,
    snapshot: &Snapshot,
    precomputed_embedding: Option<TaggedQueryEmbedding>,
    task_signals: Option<&TaskSignals>,
) -> Result<String> {
    run_pipeline_with_mode(
        query,
        max_tokens,
        language,
        file_pattern,
        format,
        snapshot,
        precomputed_embedding,
        task_signals,
        SemanticMode::Auto,
    )
    .map(|(text, _)| text)
}

fn reference_scores<'a>(
    graph: &Graph,
    ids: impl IntoIterator<Item = &'a str>,
) -> HashMap<String, f64> {
    ids.into_iter()
        .filter_map(|id| {
            graph
                .symbol_by_row_id(id)
                .map(|symbol| (id.to_string(), graph.reference_score(symbol)))
        })
        .collect()
}

/// Run the get_context pipeline with explicit semantic mode control.
#[allow(clippy::too_many_arguments)]
pub fn run_pipeline_with_mode(
    query: &str,
    max_tokens: Option<u32>,
    language: Option<String>,
    file_pattern: Option<String>,
    format: Option<String>,
    snapshot: &Snapshot,
    precomputed_embedding: Option<TaggedQueryEmbedding>,
    task_signals: Option<&TaskSignals>,
    semantic_mode: SemanticMode,
) -> Result<(String, u32)> {
    use super::allocation::TokenBudget;
    use super::entries::{build_neighbor_entries, build_pivot_entries};
    use super::formatting::{ContextData, format_context_with_mode};
    use julie_index::search::index::SearchFilter;

    let graph = snapshot.graph();
    let mut resolved_signals = task_signals.cloned().unwrap_or_default();
    hydrate_failing_test_links(
        (0..graph.len() as u32).map(|index| graph.symbol(SymbolId(index))),
        &mut resolved_signals,
    );

    let filter = SearchFilter {
        language,
        kind: None,
        file_pattern,
        exclude_tests: false,
    };
    let profile = julie_index::search::weights::SearchWeightProfile::get_context();
    let mut search_results = julie_index::search::hybrid::hybrid_search_with_tagged_embedding(
        snapshot,
        query,
        &filter,
        30,
        precomputed_embedding,
        Some(profile),
        semantic_mode,
    )?;
    merge_task_signal_seed_results(
        &mut search_results.results,
        graph,
        &filter,
        &resolved_signals,
    );
    let output_format = super::formatting::OutputFormat::from_option(format.as_deref());

    if search_results.results.is_empty() {
        let empty_data = ContextData {
            query: query.to_string(),
            pivots: vec![],
            neighbors: vec![],
            allocation: TokenBudget::new(0).allocate(0, 0),
            truncated: false,
        };
        return Ok((format_context_with_mode(&empty_data, output_format), 0));
    }

    let ref_scores = reference_scores(
        graph,
        search_results
            .results
            .iter()
            .map(|result| result.id.as_str()),
    );
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

    let expansion = expand_graph(&pivots, graph);
    let pivot_id_set: std::collections::HashSet<&str> = pivots
        .iter()
        .map(|pivot| pivot.result.id.as_str())
        .collect();
    let mut expansion = if should_expand_second_hop(&resolved_signals, &expansion) {
        let second_hop_seeds = select_second_hop_seeds(&expansion, resolved_signals.prefer_tests);
        if second_hop_seeds.is_empty() {
            expansion
        } else {
            merge_expansions(
                expansion,
                expand_graph_from_symbols(&second_hop_seeds, graph),
            )
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

    let pivot_ref_scores =
        reference_scores(graph, pivots.iter().map(|pivot| pivot.result.id.as_str()));
    let pivot_entries = build_pivot_entries(
        &pivots,
        &expansion,
        snapshot,
        &allocation,
        &pivot_ref_scores,
    );

    let neighbor_output = build_neighbor_entries(
        &expansion,
        allocation.neighbor_tokens,
        resolved_signals.prefer_tests,
    );
    let context_data = ContextData {
        query: query.to_string(),
        pivots: pivot_entries,
        neighbors: neighbor_output.entries,
        allocation,
        truncated: !neighbor_output.overflow_entries.is_empty(),
    };

    let count = (context_data.pivots.len() + context_data.neighbors.len()) as u32;
    Ok((
        format_context_with_mode(&context_data, output_format),
        count,
    ))
}

/// Handler entry point: resolves the workspace, takes its snapshot, delegates to run_pipeline.
pub async fn run(tool: &GetContextTool, handler: &dyn ToolContext) -> Result<String> {
    let workspace_target = handler
        .resolve_workspace_target(tool.workspace.as_deref())
        .await?;
    run_with_target(tool, handler, workspace_target).await
}

/// Same as `run`, but uses a workspace target the caller has already resolved.
/// Tool wrappers in `src/handler/tools/` call this so the workspace is resolved
/// exactly once per request (used for both metrics attribution and the actual
/// tool call).
pub async fn run_with_target(
    tool: &GetContextTool,
    handler: &dyn ToolContext,
    workspace_target: WorkspaceTarget,
) -> Result<String> {
    run_with_target_and_budget(tool, handler, workspace_target, None)
        .await
        .map(|(text, _)| text)
}

/// Same as `run_with_target`, but accepts an optional `EmbeddingRequestBudget`
/// for request-level timeout and cancellation enforcement.
pub async fn run_with_target_and_budget(
    tool: &GetContextTool,
    handler: &dyn ToolContext,
    workspace_target: WorkspaceTarget,
    budget: Option<EmbeddingRequestBudget>,
) -> Result<(String, u32)> {
    let budget = budget.unwrap_or_default();
    let query = tool.query.clone();
    let max_tokens = tool.max_tokens;
    let language = tool.language.clone();
    let file_pattern = tool.file_pattern.clone();
    let format = tool.format.clone();
    let task_signals = TaskSignals::from_tool(tool);
    let semantic_mode = tool.semantics.unwrap_or(SemanticMode::Auto);

    if let WorkspaceTarget::Target(target_workspace_id) = &workspace_target {
        debug!("get_context: using workspace {}", target_workspace_id);
    }
    let snapshot: Arc<Snapshot> = handler.snapshot(&workspace_target).await?;
    let embedding_provider = handler.embedding_provider().await;

    tokio::task::spawn_blocking(move || -> Result<(String, u32)> {
        let precomputed_embedding = if semantic_mode == SemanticMode::Off {
            None
        } else {
            julie_index::search::hybrid::compute_tagged_query_embedding_for_hybrid(
                &query,
                embedding_provider.as_deref(),
                &budget,
                semantic_mode,
            )?
        };
        run_pipeline_with_mode(
            &query,
            max_tokens,
            language,
            file_pattern,
            format,
            &snapshot,
            precomputed_embedding,
            Some(&task_signals),
            semantic_mode,
        )
    })
    .await
    .map_err(|e| anyhow::anyhow!("spawn_blocking error: {}", e))?
}
