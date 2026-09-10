//! Main workspace indexing orchestration: scan, apply path changes, embed.

use super::incremental::{existing_path_hashes, path_changes, scan_indexable_files};
use super::pipeline::run_indexing_pipeline;
use super::pipeline_persistence::store_for_route;
use super::route::{IndexRoute, IndexRouteRepairReason};
use super::state::IndexingOperation;
use super::store_open::store_for_workspace;
use crate::handler::JulieServerHandler;
use crate::tools::workspace::commands::ManageWorkspaceTool;
use anyhow::{Context, Result};
use julie_core::workspace::mutation_gate::MutationGuard;
use julie_index::checkout_store::{CheckoutStore, PathChange};
use std::path::Path;
use std::sync::atomic::Ordering;
use tracing::{debug, info, warn};

/// Result of workspace indexing, distinguishing files processed from store totals.
pub(crate) struct IndexResult {
    /// Files actually processed in this indexing run (may be 0 if nothing changed)
    pub files_processed: usize,
    /// Orphaned files cleaned from the store (deleted from disk since last index)
    pub orphans_cleaned: usize,
    /// Latest canonical SQLite revision after this indexing run
    pub canonical_revision: Option<i64>,
    /// Total files in the store after indexing
    pub files_total: usize,
    /// Total symbols in the store after indexing
    pub symbols_total: usize,
    /// Total relationships in the store after indexing
    pub relationships_total: usize,
    /// Total indexing duration in milliseconds
    pub duration_ms: u64,
}

impl ManageWorkspaceTool {
    pub(crate) async fn semantic_index_engine_refresh_needed_for_path(
        &self,
        handler: &JulieServerHandler,
        workspace_path: &Path,
    ) -> Result<bool> {
        let route = match IndexRoute::for_workspace_path(handler, workspace_path).await {
            Ok(route) => route,
            Err(err)
                if matches!(
                    err.reason,
                    IndexRouteRepairReason::PrimaryBindingUnavailable
                        | IndexRouteRepairReason::StorageAnchorUnavailable
                ) =>
            {
                debug!(
                    workspace_path = %workspace_path.display(),
                    "Skipping semantic engine preflight because no readable index route exists yet"
                );
                return Ok(false);
            }
            Err(err) => return Err(anyhow::Error::new(err)),
        };
        match store_for_workspace(handler, &route.workspace_id, &route.workspace_root).await {
            Ok(store) => Ok(existing_path_hashes(&store)?.is_empty()),
            Err(_) => Ok(true),
        }
    }

    /// Index a workspace: scan files, apply path changes, schedule embeddings.
    pub(crate) async fn index_workspace_files(
        &self,
        handler: &JulieServerHandler,
        workspace_path: &Path,
        force_reindex: bool,
        guard: &MutationGuard<'_>,
    ) -> Result<IndexResult> {
        let index_start = std::time::Instant::now();
        info!("🔍 Scanning workspace: {}", workspace_path.display());

        let route = IndexRoute::for_workspace_path(handler, workspace_path)
            .await
            .map_err(anyhow::Error::new)?;
        debug!(
            workspace_id = %route.workspace_id,
            workspace_root = %route.workspace_root.display(),
            is_primary = route.is_primary,
            "Resolved indexing route"
        );

        let workspace_path_clone = workspace_path.to_path_buf();
        let tool_clone = self.clone();
        let write_julieignore = !handler
            .suppress_workspace_file_writes
            .load(Ordering::Relaxed);
        let all_discovered_files = tokio::task::spawn_blocking(move || {
            if write_julieignore {
                tool_clone.discover_indexable_files(&workspace_path_clone)
            } else {
                tool_clone.discover_indexable_files_with_options(&workspace_path_clone, false)
            }
        })
        .await
        .map_err(|e| anyhow::anyhow!("File discovery task failed: {}", e))??;

        info!(
            "📊 Discovered {} files total after filtering",
            all_discovered_files.len()
        );

        let store = store_for_route(handler, &route).await?;
        let existing = existing_path_hashes(&store)?;
        let operation = if force_reindex {
            IndexingOperation::Full
        } else {
            route
                .indexing_runtime
                .as_ref()
                .and_then(|runtime| {
                    let snapshot = runtime
                        .read()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .snapshot();
                    snapshot
                        .catchup_active
                        .then_some(IndexingOperation::CatchUp)
                })
                .unwrap_or(IndexingOperation::Incremental)
        };

        let scanned = scan_indexable_files(&route.workspace_root, &all_discovered_files)?;
        let changes = path_changes(&scanned, &existing, operation);
        let files_processed = changes
            .iter()
            .filter(|change| matches!(change, PathChange::Upsert { .. }))
            .count();
        let orphans_cleaned = changes
            .iter()
            .filter(|change| matches!(change, PathChange::Remove { .. }))
            .count();

        if files_processed == 0 && orphans_cleaned == 0 {
            let (symbols_total, files_total, relationships_total) = store_totals(&store);
            handler
                .indexing_status
                .search_ready
                .store(true, Ordering::Release);
            info!(
                "✅ Indexing skipped: no changed files; {} symbols, {} files already stored",
                symbols_total, files_total
            );
            return Ok(IndexResult {
                files_processed: 0,
                orphans_cleaned,
                canonical_revision: None,
                files_total,
                symbols_total,
                relationships_total,
                duration_ms: index_start.elapsed().as_millis() as u64,
            });
        }

        let pipeline_result = run_indexing_pipeline(
            self,
            handler,
            all_discovered_files,
            &route,
            operation,
            guard,
        )
        .await
        .context("running indexing pipeline")?;

        if pipeline_result.state.repair_needed() {
            warn!(
                workspace_id = %route.workspace_id,
                repair_files = pipeline_result.state.repair_file_count(),
                "Indexing finished with repair-needed files"
            );
        }

        let (symbols_total, files_total, relationships_total) = store_totals(&store);
        info!(
            "✅ Indexing complete: {} symbols, {} files stored",
            symbols_total, files_total
        );

        Ok(IndexResult {
            files_processed,
            orphans_cleaned,
            canonical_revision: pipeline_result.canonical_revision,
            files_total,
            symbols_total,
            relationships_total,
            duration_ms: index_start.elapsed().as_millis() as u64,
        })
    }
}

fn store_totals(store: &CheckoutStore) -> (usize, usize, usize) {
    let status = store.status();
    (
        status.graph.symbols,
        store.current().graph().paths().len(),
        status.graph.edges,
    )
}
