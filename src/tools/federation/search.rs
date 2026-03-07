//! Federated search across multiple workspaces with parallel fan-out.
//!
//! Given a list of workspace entries (pre-extracted from `DaemonState`),
//! fans out search queries in parallel and merges results with RRF.

use std::sync::{Arc, Mutex};

use anyhow::Result;
use tokio::task::JoinSet;
use tracing::{debug, warn};

use super::rrf::{self, RrfItem, RRF_K};
use crate::search::index::{
    ContentSearchResult, ContentSearchResults, SearchFilter, SearchIndex, SymbolSearchResult,
    SymbolSearchResults,
};

// ---------------------------------------------------------------------------
// Federated result wrappers
// ---------------------------------------------------------------------------

/// A symbol search result annotated with its source workspace.
#[derive(Clone)]
pub struct FederatedSymbolResult {
    /// The underlying search result (with `file_path` relative to workspace root).
    pub result: SymbolSearchResult,
    /// Workspace ID this result came from.
    pub workspace_id: String,
    /// Human-readable project name for display.
    pub project_name: String,
    /// Cross-workspace unique ID: `"{workspace_id}:{symbol_id}"`.
    /// Pre-computed so `RrfItem::rrf_id()` can return `&str`.
    global_id: String,
}

impl FederatedSymbolResult {
    pub fn new(
        result: SymbolSearchResult,
        workspace_id: String,
        project_name: String,
    ) -> Self {
        let global_id = format!("{}:{}", workspace_id, result.id);
        Self {
            result,
            workspace_id,
            project_name,
            global_id,
        }
    }
}

impl RrfItem for FederatedSymbolResult {
    fn rrf_id(&self) -> &str {
        &self.global_id
    }

    fn set_score(&mut self, score: f32) {
        self.result.score = score;
    }

    fn score(&self) -> f32 {
        self.result.score
    }
}

/// A content search result annotated with its source workspace.
#[derive(Clone)]
pub struct FederatedContentResult {
    /// The underlying content search result.
    pub result: ContentSearchResult,
    /// Workspace ID this result came from.
    pub workspace_id: String,
    /// Human-readable project name for display.
    pub project_name: String,
    /// Cross-workspace unique ID: `"{workspace_id}:{file_path}"`.
    /// Pre-computed so `RrfItem::rrf_id()` can return `&str`.
    global_id: String,
}

impl FederatedContentResult {
    pub fn new(
        result: ContentSearchResult,
        workspace_id: String,
        project_name: String,
    ) -> Self {
        let global_id = format!("{}:{}", workspace_id, result.file_path);
        Self {
            result,
            workspace_id,
            project_name,
            global_id,
        }
    }
}

impl RrfItem for FederatedContentResult {
    fn rrf_id(&self) -> &str {
        &self.global_id
    }

    fn set_score(&mut self, score: f32) {
        self.result.score = score;
    }

    fn score(&self) -> f32 {
        self.result.score
    }
}

// ---------------------------------------------------------------------------
// Workspace info passed to federation functions
// ---------------------------------------------------------------------------

/// Pre-extracted workspace info for federation.
///
/// Callers read-lock `DaemonState`, extract this data, then drop the lock
/// before calling federation functions. This avoids holding the DaemonState
/// lock across async boundaries.
#[derive(Clone)]
pub struct WorkspaceSearchEntry {
    pub workspace_id: String,
    pub project_name: String,
    pub search_index: Arc<Mutex<SearchIndex>>,
}

// ---------------------------------------------------------------------------
// Federated search functions
// ---------------------------------------------------------------------------

/// Fan out a symbol search across multiple workspaces in parallel,
/// then merge results using RRF.
///
/// Each workspace search runs in its own `spawn_blocking` task (since
/// `SearchIndex` uses `std::sync::Mutex`). Results are tagged with
/// workspace attribution and merged via N-list RRF.
///
/// Workspaces that fail to search are logged and skipped — partial
/// results from successful workspaces are still returned.
pub async fn federated_symbol_search(
    query: &str,
    filter: &SearchFilter,
    limit: usize,
    workspaces: &[WorkspaceSearchEntry],
) -> Result<Vec<FederatedSymbolResult>> {
    if workspaces.is_empty() {
        return Ok(Vec::new());
    }

    // Single workspace: skip RRF overhead
    if workspaces.len() == 1 {
        let ws = &workspaces[0];
        let results =
            search_symbols_in_workspace(query, filter, limit, ws).await?;
        return Ok(results);
    }

    // Fan out searches in parallel
    let mut join_set = JoinSet::new();

    for ws in workspaces {
        let query = query.to_string();
        let filter = filter.clone();
        // Over-fetch per workspace: 2x limit gives RRF a larger merge pool
        let per_ws_limit = limit * 2;
        let search_index = Arc::clone(&ws.search_index);
        let workspace_id = ws.workspace_id.clone();
        let project_name = ws.project_name.clone();

        join_set.spawn(async move {
            let results = tokio::task::spawn_blocking(move || -> Result<(String, String, SymbolSearchResults)> {
                let index = search_index.lock().map_err(|e| {
                    anyhow::anyhow!("Failed to lock search index for {}: {}", workspace_id, e)
                })?;
                let search_results = index.search_symbols(&query, &filter, per_ws_limit)?;
                Ok((workspace_id, project_name, search_results))
            })
            .await??;
            Ok::<(String, String, SymbolSearchResults), anyhow::Error>(results)
        });
    }

    // Collect per-workspace result lists
    let mut per_workspace_lists: Vec<Vec<FederatedSymbolResult>> = Vec::new();

    while let Some(join_result) = join_set.join_next().await {
        match join_result {
            Ok(Ok((workspace_id, project_name, search_results))) => {
                debug!(
                    "Federated symbol search: workspace '{}' returned {} results",
                    project_name,
                    search_results.results.len()
                );
                let federated: Vec<FederatedSymbolResult> = search_results
                    .results
                    .into_iter()
                    .map(|r| {
                        FederatedSymbolResult::new(
                            r,
                            workspace_id.clone(),
                            project_name.clone(),
                        )
                    })
                    .collect();
                per_workspace_lists.push(federated);
            }
            Ok(Err(e)) => {
                warn!("Federated symbol search: workspace search failed: {e}");
            }
            Err(e) => {
                warn!("Federated symbol search: task join failed: {e}");
            }
        }
    }

    // Merge via RRF
    let merged = rrf::multi_rrf_merge(per_workspace_lists, RRF_K, limit);
    Ok(merged)
}

/// Fan out a content search across multiple workspaces in parallel,
/// then merge results using RRF.
///
/// Same pattern as `federated_symbol_search` but for file-level content results.
pub async fn federated_content_search(
    query: &str,
    filter: &SearchFilter,
    limit: usize,
    workspaces: &[WorkspaceSearchEntry],
) -> Result<Vec<FederatedContentResult>> {
    if workspaces.is_empty() {
        return Ok(Vec::new());
    }

    // Single workspace: skip RRF overhead
    if workspaces.len() == 1 {
        let ws = &workspaces[0];
        let results =
            search_content_in_workspace(query, filter, limit, ws).await?;
        return Ok(results);
    }

    // Fan out searches in parallel
    let mut join_set = JoinSet::new();

    for ws in workspaces {
        let query = query.to_string();
        let filter = filter.clone();
        let per_ws_limit = limit * 2;
        let search_index = Arc::clone(&ws.search_index);
        let workspace_id = ws.workspace_id.clone();
        let project_name = ws.project_name.clone();

        join_set.spawn(async move {
            let results = tokio::task::spawn_blocking(move || -> Result<(String, String, ContentSearchResults)> {
                let index = search_index.lock().map_err(|e| {
                    anyhow::anyhow!("Failed to lock search index for {}: {}", workspace_id, e)
                })?;
                let search_results = index.search_content(&query, &filter, per_ws_limit)?;
                Ok((workspace_id, project_name, search_results))
            })
            .await??;
            Ok::<(String, String, ContentSearchResults), anyhow::Error>(results)
        });
    }

    // Collect per-workspace result lists
    let mut per_workspace_lists: Vec<Vec<FederatedContentResult>> = Vec::new();

    while let Some(join_result) = join_set.join_next().await {
        match join_result {
            Ok(Ok((workspace_id, project_name, search_results))) => {
                debug!(
                    "Federated content search: workspace '{}' returned {} results",
                    project_name,
                    search_results.results.len()
                );
                let federated: Vec<FederatedContentResult> = search_results
                    .results
                    .into_iter()
                    .map(|r| {
                        FederatedContentResult::new(
                            r,
                            workspace_id.clone(),
                            project_name.clone(),
                        )
                    })
                    .collect();
                per_workspace_lists.push(federated);
            }
            Ok(Err(e)) => {
                warn!("Federated content search: workspace search failed: {e}");
            }
            Err(e) => {
                warn!("Federated content search: task join failed: {e}");
            }
        }
    }

    // Merge via RRF
    let merged = rrf::multi_rrf_merge(per_workspace_lists, RRF_K, limit);
    Ok(merged)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Search symbols in a single workspace (used for single-workspace fast path).
async fn search_symbols_in_workspace(
    query: &str,
    filter: &SearchFilter,
    limit: usize,
    ws: &WorkspaceSearchEntry,
) -> Result<Vec<FederatedSymbolResult>> {
    let query = query.to_string();
    let filter = filter.clone();
    let search_index = Arc::clone(&ws.search_index);
    let workspace_id = ws.workspace_id.clone();
    let project_name = ws.project_name.clone();

    let search_results = tokio::task::spawn_blocking(move || -> Result<SymbolSearchResults> {
        let index = search_index.lock().map_err(|e| {
            anyhow::anyhow!("Failed to lock search index for {}: {}", workspace_id, e)
        })?;
        Ok(index.search_symbols(&query, &filter, limit)?)
    })
    .await??;

    let federated: Vec<FederatedSymbolResult> = search_results
        .results
        .into_iter()
        .map(|r| {
            FederatedSymbolResult::new(r, ws.workspace_id.clone(), ws.project_name.clone())
        })
        .collect();

    Ok(federated)
}

/// Search content in a single workspace (used for single-workspace fast path).
async fn search_content_in_workspace(
    query: &str,
    filter: &SearchFilter,
    limit: usize,
    ws: &WorkspaceSearchEntry,
) -> Result<Vec<FederatedContentResult>> {
    let query = query.to_string();
    let filter = filter.clone();
    let search_index = Arc::clone(&ws.search_index);
    let workspace_id = ws.workspace_id.clone();
    let project_name = ws.project_name.clone();

    let search_results = tokio::task::spawn_blocking(move || -> Result<ContentSearchResults> {
        let index = search_index.lock().map_err(|e| {
            anyhow::anyhow!("Failed to lock search index for {}: {}", workspace_id, e)
        })?;
        Ok(index.search_content(&query, &filter, limit)?)
    })
    .await??;

    let federated: Vec<FederatedContentResult> = search_results
        .results
        .into_iter()
        .map(|r| {
            FederatedContentResult::new(r, ws.workspace_id.clone(), ws.project_name.clone())
        })
        .collect();

    Ok(federated)
}
