//! Federated search across multiple workspaces with parallel fan-out.
//!
//! Given a list of workspace entries (pre-extracted from `DaemonState`),
//! fans out search queries in parallel and merges results with RRF.

use std::sync::{Arc, Mutex};

use anyhow::Result;
use tokio::task::JoinSet;
use tracing::{debug, warn};

use super::rrf::{self, RrfItem, RRF_K};
use crate::database::SymbolDatabase;
use crate::search::index::{
    ContentSearchResult, SearchFilter, SearchIndex, SymbolSearchResult,
};
use crate::search::scoring::{apply_centrality_boost, promote_exact_name_matches};

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
    /// Database for centrality boost (definition search) and content verification.
    /// Optional: gracefully degrades to unranked/unverified results when absent.
    pub db: Option<Arc<Mutex<SymbolDatabase>>>,
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
        let db = ws.db.clone();
        let workspace_id = ws.workspace_id.clone();
        let project_name = ws.project_name.clone();

        join_set.spawn(async move {
            let results = tokio::task::spawn_blocking(move || -> Result<(String, String, Vec<SymbolSearchResult>)> {
                let index = search_index.lock().map_err(|e| {
                    anyhow::anyhow!("Failed to lock search index for {}: {}", workspace_id, e)
                })?;
                let search_results = index.search_symbols(&query, &filter, per_ws_limit)?;
                let mut symbols = search_results.results;

                // Apply centrality boost per-workspace (same as single-workspace path)
                if let Some(ref db_arc) = db {
                    if let Ok(db_lock) = db_arc.lock() {
                        let ids: Vec<&str> = symbols.iter().map(|s| s.id.as_str()).collect();
                        if let Ok(ref_scores) = db_lock.get_reference_scores(&ids) {
                            apply_centrality_boost(&mut symbols, &ref_scores);
                        }
                    }
                }
                promote_exact_name_matches(&mut symbols, &query);

                Ok((workspace_id, project_name, symbols))
            })
            .await??;
            Ok::<(String, String, Vec<SymbolSearchResult>), anyhow::Error>(results)
        });
    }

    // Collect per-workspace result lists
    let mut per_workspace_lists: Vec<Vec<FederatedSymbolResult>> = Vec::new();

    while let Some(join_result) = join_set.join_next().await {
        match join_result {
            Ok(Ok((workspace_id, project_name, symbols))) => {
                debug!(
                    "Federated symbol search: workspace '{}' returned {} results",
                    project_name,
                    symbols.len()
                );
                let federated: Vec<FederatedSymbolResult> = symbols
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
        // Over-fetch for post-verification (same as single-workspace content path)
        let per_ws_limit = limit.saturating_mul(5).max(50);
        let search_index = Arc::clone(&ws.search_index);
        let db = ws.db.clone();
        let workspace_id = ws.workspace_id.clone();
        let project_name = ws.project_name.clone();

        join_set.spawn(async move {
            let results = tokio::task::spawn_blocking(move || -> Result<(String, String, Vec<ContentSearchResult>)> {
                let index = search_index.lock().map_err(|e| {
                    anyhow::anyhow!("Failed to lock search index for {}: {}", workspace_id, e)
                })?;
                let search_results = index.search_content(&query, &filter, per_ws_limit)?;

                // Post-verify content matches if DB is available
                if let Some(ref db_arc) = db {
                    if let Ok(db_lock) = db_arc.lock() {
                        let query_words: Vec<String> = query
                            .split(|c: char| !c.is_alphanumeric())
                            .filter(|w| !w.is_empty())
                            .map(|w| w.to_lowercase())
                            .collect();

                        let mut verified = Vec::with_capacity(limit);
                        for result in search_results.results {
                            if verified.len() >= limit {
                                break;
                            }
                            match db_lock.get_file_content(&result.file_path) {
                                Ok(Some(content)) => {
                                    let content_lower = content.to_lowercase();
                                    if query_words.iter().all(|w| content_lower.contains(w.as_str())) {
                                        verified.push(result);
                                    }
                                }
                                _ => verified.push(result),
                            }
                        }
                        return Ok((workspace_id, project_name, verified));
                    }
                }

                // No DB — return unverified, truncated to limit
                let results: Vec<_> = search_results.results.into_iter().take(limit).collect();
                Ok((workspace_id, project_name, results))
            })
            .await??;
            Ok::<(String, String, Vec<ContentSearchResult>), anyhow::Error>(results)
        });
    }

    // Collect per-workspace result lists
    let mut per_workspace_lists: Vec<Vec<FederatedContentResult>> = Vec::new();

    while let Some(join_result) = join_set.join_next().await {
        match join_result {
            Ok(Ok((workspace_id, project_name, content_results))) => {
                debug!(
                    "Federated content search: workspace '{}' returned {} results",
                    project_name,
                    content_results.len()
                );
                let federated: Vec<FederatedContentResult> = content_results
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
///
/// Applies centrality boost and exact name promotion when the DB is available,
/// matching the quality of single-workspace definition search.
async fn search_symbols_in_workspace(
    query: &str,
    filter: &SearchFilter,
    limit: usize,
    ws: &WorkspaceSearchEntry,
) -> Result<Vec<FederatedSymbolResult>> {
    let query = query.to_string();
    let filter = filter.clone();
    let search_index = Arc::clone(&ws.search_index);
    let db = ws.db.clone();
    let workspace_id = ws.workspace_id.clone();

    let search_results = tokio::task::spawn_blocking(move || -> Result<Vec<SymbolSearchResult>> {
        let index = search_index.lock().map_err(|e| {
            anyhow::anyhow!("Failed to lock search index for {}: {}", workspace_id, e)
        })?;
        let results = index.search_symbols(&query, &filter, limit)?;
        let mut symbols = results.results;

        // Apply centrality boost if DB is available (same as single-workspace path)
        if let Some(ref db_arc) = db {
            if let Ok(db_lock) = db_arc.lock() {
                let ids: Vec<&str> = symbols.iter().map(|s| s.id.as_str()).collect();
                if let Ok(ref_scores) = db_lock.get_reference_scores(&ids) {
                    apply_centrality_boost(&mut symbols, &ref_scores);
                }
            }
        }

        // Promote exact name matches (three-tier: definition kinds > other exact > rest)
        promote_exact_name_matches(&mut symbols, &query);

        Ok(symbols)
    })
    .await??;

    let federated: Vec<FederatedSymbolResult> = search_results
        .into_iter()
        .map(|r| {
            FederatedSymbolResult::new(r, ws.workspace_id.clone(), ws.project_name.clone())
        })
        .collect();

    Ok(federated)
}

/// Search content in a single workspace (used for single-workspace fast path).
///
/// Over-fetches and post-verifies results against actual file content to
/// eliminate false positives from CodeTokenizer over-splitting, matching
/// the quality of single-workspace content search.
async fn search_content_in_workspace(
    query: &str,
    filter: &SearchFilter,
    limit: usize,
    ws: &WorkspaceSearchEntry,
) -> Result<Vec<FederatedContentResult>> {
    let query_str = query.to_string();
    let filter = filter.clone();
    let search_index = Arc::clone(&ws.search_index);
    let db = ws.db.clone();
    let workspace_id = ws.workspace_id.clone();

    let search_results = tokio::task::spawn_blocking(move || -> Result<Vec<ContentSearchResult>> {
        let index = search_index.lock().map_err(|e| {
            anyhow::anyhow!("Failed to lock search index for {}: {}", workspace_id, e)
        })?;

        // Over-fetch for post-verification (same ratio as single-workspace path)
        let fetch_limit = limit.saturating_mul(5).max(50);
        let results = index.search_content(&query_str, &filter, fetch_limit)?;

        // Post-verify if DB is available
        if let Some(ref db_arc) = db {
            if let Ok(db_lock) = db_arc.lock() {
                let query_words: Vec<String> = query_str
                    .split(|c: char| !c.is_alphanumeric())
                    .filter(|w| !w.is_empty())
                    .map(|w| w.to_lowercase())
                    .collect();

                let mut verified = Vec::with_capacity(limit);
                for result in results.results {
                    if verified.len() >= limit {
                        break;
                    }
                    match db_lock.get_file_content(&result.file_path) {
                        Ok(Some(content)) => {
                            let content_lower = content.to_lowercase();
                            let all_match = query_words
                                .iter()
                                .all(|word| content_lower.contains(word.as_str()));
                            if all_match {
                                verified.push(result);
                            } else {
                                debug!(
                                    "Filtered federated false positive: {} (missing query words)",
                                    result.file_path
                                );
                            }
                        }
                        // File not in DB or error — include as-is (graceful degradation)
                        _ => verified.push(result),
                    }
                }
                return Ok(verified);
            }
        }

        // No DB — return unverified results truncated to limit
        Ok(results.results.into_iter().take(limit).collect())
    })
    .await??;

    let federated: Vec<FederatedContentResult> = search_results
        .into_iter()
        .map(|r| {
            FederatedContentResult::new(r, ws.workspace_id.clone(), ws.project_name.clone())
        })
        .collect();

    Ok(federated)
}
