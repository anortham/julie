//! Startup and Indexing Utilities
//!
//! This module contains functions for workspace initialization, staleness detection,
//! and automatic indexing on server startup.

use crate::handler::JulieServerHandler;
use crate::tools::workspace::ManageWorkspaceTool;
use crate::tools::workspace::indexing::engine_version::{
    SEMANTIC_INDEX_ENGINE_COMPONENT, SEMANTIC_INDEX_ENGINE_VERSION,
};
use crate::tools::workspace::indexing::state::IndexingRepairReason;
use crate::workspace::mutation_gate::{MutationGuard, acquire_gate};
use crate::workspace::startup_hint::WorkspaceStartupSource;
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;
use std::time::SystemTime;
use tracing::{debug, info, warn};

pub(crate) fn startup_source_prefers_request_roots(source: Option<WorkspaceStartupSource>) -> bool {
    matches!(source, Some(WorkspaceStartupSource::Cwd))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PrimaryWorkspaceRepairPlan {
    pub reasons: Vec<IndexingRepairReason>,
}

/// Checkpoint the active workspace database WAL if a workspace is initialized.
pub async fn checkpoint_active_workspace_wal(
    handler: &JulieServerHandler,
) -> Result<Option<(i32, i32, i32)>> {
    let primary_snapshot = match handler.primary_workspace_snapshot().await {
        Ok(snapshot) => snapshot,
        Err(err) => {
            if handler.is_primary_workspace_swap_in_progress() {
                return Err(err);
            }

            if handler.get_workspace().await?.is_none() {
                return Ok(None);
            }

            return Err(err);
        }
    };
    let db_arc = primary_snapshot.database;

    tokio::task::spawn_blocking(move || -> Result<Option<(i32, i32, i32)>> {
        let mut db = db_arc.try_lock().map_err(|e| {
            anyhow::anyhow!("Could not acquire database lock for checkpoint: {}", e)
        })?;
        Ok(Some(db.checkpoint_wal()?))
    })
    .await
    .map_err(|e| anyhow::anyhow!("Failed to join checkpoint task: {}", e))?
}

/// Check if the workspace needs indexing by examining database state
///
/// This function checks:
/// 1. If the database is completely empty (requires full index)
/// 2. If files have been modified since last index (staleness)
/// 3. If new files exist that aren't in the database
pub async fn check_if_indexing_needed(handler: &JulieServerHandler) -> Result<bool> {
    Ok(plan_primary_workspace_repair(handler).await?.is_some())
}

/// Acquire the mutation gate for the primary workspace, then run the repair pass.
///
/// This is the public entry point. It serializes concurrent catch-up scans by
/// holding the workspace mutation gate for the duration of the repair. If no
/// workspace is bound yet (empty-database first-run path), the gate cannot be
/// keyed by workspace_id and the repair runs ungated — there is no concurrent
/// writer to contend with in that case.
pub(crate) async fn run_primary_workspace_repair(
    handler: &JulieServerHandler,
) -> Result<Option<PrimaryWorkspaceRepairPlan>> {
    match handler.require_primary_workspace_identity() {
        Ok(workspace_id) => {
            let guard = acquire_gate(&workspace_id).await;
            run_primary_workspace_repair_inner(&guard, handler).await
        }
        Err(_) => {
            // No workspace bound yet (first-run / empty-database path).
            // There is no concurrent writer to serialize against, so run
            // the repair directly without a gate.
            run_primary_workspace_repair_inner_ungated(handler).await
        }
    }
}

/// Inner repair implementation. Takes a `&MutationGuard<'_>` as a proof token
/// that the caller already holds the workspace mutation gate. Does NOT call
/// `acquire_gate` — doing so would deadlock since the gate is not reentrant.
///
/// Call chains originating from here must use other `_inner` variants (never
/// `_gated` variants) to avoid re-acquiring the gate.
pub(crate) async fn run_primary_workspace_repair_inner(
    _guard: &MutationGuard<'_>,
    handler: &JulieServerHandler,
) -> Result<Option<PrimaryWorkspaceRepairPlan>> {
    run_primary_workspace_repair_inner_ungated(handler).await
}

/// The actual repair body, shared by the gated and ungated paths.
///
/// Private — callers must go through `run_primary_workspace_repair` (gated) or
/// `run_primary_workspace_repair_inner` (already-gated proof-token path).
async fn run_primary_workspace_repair_inner_ungated(
    handler: &JulieServerHandler,
) -> Result<Option<PrimaryWorkspaceRepairPlan>> {
    let indexing_runtime = handler
        .primary_workspace_snapshot()
        .await
        .ok()
        .and_then(|snapshot| snapshot.indexing_runtime);

    if let Some(runtime) = indexing_runtime.as_ref() {
        let mut runtime = runtime
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        runtime.set_catchup_active(true);
        runtime.set_watcher_paused(true);
    }

    let repair_result = async {
        match plan_primary_workspace_repair(handler).await? {
            Some(plan) => {
                let reasons = plan
                    .reasons
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                info!(%reasons, "📚 Workspace needs indexing, starting repair run");
                if let Some(runtime) = indexing_runtime.as_ref() {
                    let mut runtime = runtime
                        .write()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    for reason in &plan.reasons {
                        runtime.record_repair_reason(*reason);
                    }
                }
                cancel_primary_embedding_task(handler).await;

                let index_tool = ManageWorkspaceTool {
                    operation: "index".to_string(),
                    path: None,
                    name: None,
                    workspace_id: None,
                    force: Some(false),
                    detailed: None,
                };

                let repair_rebuilds_embedding_inputs = plan.reasons.iter().any(|reason| {
                    matches!(
                        reason,
                        IndexingRepairReason::EmptyDatabase
                            | IndexingRepairReason::StaleFiles
                            | IndexingRepairReason::NewFiles
                            | IndexingRepairReason::DeletedFiles
                            | IndexingRepairReason::ExtractorFailure
                            | IndexingRepairReason::WatcherOverflow
                            | IndexingRepairReason::SemanticVersionChanged
                    )
                });
                let skip_embeddings = !repair_rebuilds_embedding_inputs;
                index_tool
                    .call_tool_with_options(handler, skip_embeddings)
                    .await?;
                if let Some(runtime) = indexing_runtime.as_ref() {
                    let mut runtime = runtime
                        .write()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    for reason in &plan.reasons {
                        runtime.clear_repair_reason(*reason);
                    }
                }
                Ok(Some(plan))
            }
            None => Ok(None),
        }
    }
    .await;

    if let Some(runtime) = indexing_runtime.as_ref() {
        let mut runtime = runtime
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        runtime.set_catchup_active(false);
        runtime.set_watcher_paused(false);
    }
    repair_result
}

async fn cancel_primary_embedding_task(handler: &JulieServerHandler) {
    let Ok(workspace_id) = handler.require_primary_workspace_identity() else {
        return;
    };

    let Some((cancel_flag, mut handle)) =
        handler.embedding_tasks.lock().await.remove(&workspace_id)
    else {
        return;
    };

    cancel_flag.store(true, std::sync::atomic::Ordering::Release);
    match tokio::time::timeout(Duration::from_secs(5), &mut handle).await {
        Ok(join_result) => match join_result {
            Ok(()) => info!(%workspace_id, "Cancelled embedding task before startup repair"),
            Err(err) if err.is_cancelled() => {
                info!(%workspace_id, "Embedding task was already cancelled before startup repair")
            }
            Err(err) => {
                warn!(%workspace_id, "Embedding task ended with error before startup repair: {err}")
            }
        },
        Err(_) => {
            handle.abort();
            warn!(
                %workspace_id,
                "Timed out waiting for embedding task cancellation before startup repair"
            );
        }
    }
}

pub(crate) async fn plan_primary_workspace_repair(
    handler: &JulieServerHandler,
) -> Result<Option<PrimaryWorkspaceRepairPlan>> {
    let route =
        match crate::tools::workspace::indexing::route::IndexRoute::for_current_primary(handler)
            .await
        {
            Ok(route) => route,
            Err(err) => {
                if handler.is_primary_workspace_swap_in_progress() {
                    return Err(anyhow::Error::new(err));
                }

                if handler.get_workspace().await?.is_none() {
                    debug!("No workspace found - indexing needed");
                    return Ok(Some(PrimaryWorkspaceRepairPlan {
                        reasons: vec![IndexingRepairReason::EmptyDatabase],
                    }));
                }

                return Err(anyhow::Error::new(err));
            }
        };

    let current_primary_root = route.workspace_root.clone();
    let db_path = route.db_path.clone();
    let db_arc = route
        .database_for_read(handler)
        .await?
        .ok_or_else(|| anyhow::anyhow!("No database connection - indexing needed"))?;

    if !db_path.exists() {
        debug!("No database connection - indexing needed");
        return Ok(Some(PrimaryWorkspaceRepairPlan {
            reasons: vec![IndexingRepairReason::EmptyDatabase],
        }));
    }

    // Now lock database (no await while holding this lock)
    let db: std::sync::MutexGuard<'_, crate::database::SymbolDatabase> = match db_arc.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            warn!(
                "Database mutex poisoned during startup check, recovering: {}",
                poisoned
            );
            poisoned.into_inner()
        }
    };

    match db.has_symbols_for_workspace() {
        Ok(has_symbols) => {
            if !has_symbols {
                info!("📊 Database is empty - indexing needed");
                let mut reasons = vec![IndexingRepairReason::EmptyDatabase];
                for repair in db.list_indexing_repairs()? {
                    if let Some(reason) = IndexingRepairReason::from_str(&repair.reason) {
                        if !reasons.contains(&reason) {
                            reasons.push(reason);
                        }
                    }
                }
                return Ok(Some(PrimaryWorkspaceRepairPlan { reasons }));
            }

            let mut reasons = Vec::new();

            if !db.index_engine_version_matches(
                &route.workspace_id,
                SEMANTIC_INDEX_ENGINE_COMPONENT,
                SEMANTIC_INDEX_ENGINE_VERSION,
            )? {
                info!("📊 Index semantic version changed or missing - full indexing needed");
                reasons.push(IndexingRepairReason::SemanticVersionChanged);
            }

            // Check if index is stale by comparing file modification times with database timestamp
            let db_mtime = get_database_mtime(&db_path)?;
            let max_file_mtime = get_max_file_mtime_in_workspace(&current_primary_root)?;

            debug!(
                "Staleness check: db_mtime={:?}, max_file_mtime={:?}, stale={}",
                db_mtime,
                max_file_mtime,
                max_file_mtime > db_mtime
            );

            if max_file_mtime > db_mtime {
                info!("📊 Database is stale (files modified after last index) - indexing needed");
                reasons.push(IndexingRepairReason::StaleFiles);
            }

            // Check for new files not in database
            let indexed_files_raw: Vec<String> = db.get_all_indexed_files()?;

            // Database stores relative Unix-style paths per CLAUDE.md Path Handling Contract
            // No normalization needed - indexed_files are already relative
            let indexed_files: HashSet<String> = indexed_files_raw.into_iter().collect();

            let workspace_files = scan_workspace_files(&current_primary_root)?;
            let new_files: Vec<_> = workspace_files.difference(&indexed_files).collect();

            debug!(
                "New file check: indexed={}, workspace={}, new={}",
                indexed_files.len(),
                workspace_files.len(),
                new_files.len()
            );

            if !new_files.is_empty() {
                info!(
                    "📊 Found {} new files not in database - indexing needed",
                    new_files.len()
                );
                debug!("New files: {:?}", new_files);
                reasons.push(IndexingRepairReason::NewFiles);
            }

            // Check for deleted files (indexed but no longer on disk)
            let deleted_files: Vec<_> = indexed_files.difference(&workspace_files).collect();

            if !deleted_files.is_empty() {
                info!(
                    "📊 Found {} deleted files still in database - cleanup needed",
                    deleted_files.len()
                );
                debug!("Deleted files: {:?}", deleted_files);
                reasons.push(IndexingRepairReason::DeletedFiles);
            }

            for repair in db.list_indexing_repairs()? {
                if let Some(reason) = IndexingRepairReason::from_str(&repair.reason) {
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
            }

            if reasons.is_empty() {
                info!("✅ Index is up-to-date - no indexing needed");
                Ok(None)
            } else {
                Ok(Some(PrimaryWorkspaceRepairPlan { reasons }))
            }
        }
        Err(e) => {
            debug!(
                "Error checking database symbols: {} - assuming indexing needed",
                e
            );
            Ok(Some(PrimaryWorkspaceRepairPlan {
                reasons: vec![IndexingRepairReason::EmptyDatabase],
            }))
        }
    }
}

/// Get the modification time of the SQLite database file
///
/// Returns the mtime of the symbols.db file for the given workspace
fn get_database_mtime(db_path: &Path) -> Result<SystemTime> {
    if !db_path.exists() {
        // Database doesn't exist - return epoch (very old time)
        return Ok(SystemTime::UNIX_EPOCH);
    }

    let metadata = std::fs::metadata(&db_path)
        .with_context(|| format!("Failed to get metadata for database: {}", db_path.display()))?;

    metadata
        .modified()
        .with_context(|| format!("Failed to get mtime for database: {}", db_path.display()))
}

/// Get the maximum (newest) file modification time in the workspace
///
/// Scans all supported code files and returns the newest mtime found
fn get_max_file_mtime_in_workspace(workspace_root: &Path) -> Result<SystemTime> {
    use crate::utils::walk::{WalkConfig, build_walker};

    let mut max_mtime = SystemTime::UNIX_EPOCH;

    for result in build_walker(workspace_root, &WalkConfig::stale_scan()) {
        let entry = match result {
            Ok(e) => e,
            Err(_) => continue,
        };

        if !entry.file_type().map_or(false, |ft| ft.is_file()) {
            continue;
        }

        if !is_code_file(entry.path()) {
            continue;
        }

        if let Ok(metadata) = std::fs::metadata(entry.path()) {
            if let Ok(mtime) = metadata.modified() {
                if mtime > max_mtime {
                    max_mtime = mtime;
                }
            }
        }
    }

    Ok(max_mtime)
}

/// Scan workspace and return a set of all code file paths (relative to workspace root)
///
/// This is used to detect new files that aren't in the database yet
pub(crate) fn scan_workspace_files(workspace_root: &Path) -> Result<HashSet<String>> {
    use crate::utils::walk::{WalkConfig, build_walker};

    let mut files = HashSet::new();

    for result in build_walker(workspace_root, &WalkConfig::stale_scan()) {
        let entry = match result {
            Ok(e) => e,
            Err(_) => continue,
        };

        if !entry.file_type().map_or(false, |ft| ft.is_file()) {
            continue;
        }

        if !is_code_file(entry.path()) {
            continue;
        }

        // Get relative path from workspace root in Unix-style format
        // CRITICAL: Use to_relative_unix_style() to ensure cross-platform compatibility
        // On Windows, strip_prefix() returns paths with backslashes (src\file.rs)
        // But database stores paths with forward slashes (src/file.rs)
        if let Ok(relative_path) =
            crate::utils::paths::to_relative_unix_style(entry.path(), workspace_root)
        {
            files.insert(relative_path);
        }
    }

    Ok(files)
}

/// Check if a file is a supported code file.
///
/// Accepts files through the same candidate policy used by watcher events:
/// known parser-backed extensions are included, blacklisted names/extensions
/// are rejected, and unknown or extensionless text files stay indexable as
/// text-only files. The goal is to keep startup freshness scans, overflow
/// repair scans, and live watcher events from disagreeing about tracked files.
fn is_code_file(path: &Path) -> bool {
    crate::tools::workspace::indexing::file_policy::should_index_path_candidate(
        path,
        crate::tools::workspace::indexing::file_policy::supported_extensions_for_indexing(),
    )
}
