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
use crate::workspace::mutation_gate::MutationGuard;
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
    // Defense-in-depth (codex 3c.2 F-A follow-up): in-process FOLLOWERS are
    // pure readers — the leader owns every SQLite/Tantivy write. The deferred
    // path already guards this in `complete_deferred_auto_index_if_needed`, but
    // the non-deferred `on_initialized` → `run_auto_indexing` path reaches here
    // with NO follower guard. Refuse repair/index for followers on EVERY entry
    // path so a second process can never race the leader's writes.
    if handler.is_in_process_follower() {
        return Ok(None);
    }

    match handler.require_primary_workspace_identity() {
        Ok(workspace_id) => {
            let guard = handler.acquire_mutation_gate(&workspace_id).await;
            run_primary_workspace_repair_inner(&guard, handler).await
        }
        Err(_) => {
            // No workspace bound yet (first-run / empty-database path).
            // There is no concurrent writer to serialize against, so run
            // the repair directly without a gate. The downstream index call
            // will acquire its own gate when it discovers the workspace
            // identity.
            run_primary_workspace_repair_body(handler, None).await
        }
    }
}

/// Inner repair implementation. Takes a `&MutationGuard<'_>` as a proof token
/// that the caller already holds the workspace mutation gate. Does not acquire
/// another guard — doing so would deadlock since the gate is not reentrant.
///
/// Call chains originating from here must use other `_inner` variants (never
/// `_gated` variants) to avoid re-acquiring the gate.
pub(crate) async fn run_primary_workspace_repair_inner(
    _guard: &MutationGuard<'_>,
    handler: &JulieServerHandler,
) -> Result<Option<PrimaryWorkspaceRepairPlan>> {
    run_primary_workspace_repair_body(handler, Some(_guard)).await
}

/// The actual repair body, shared by the gated and ungated paths.
///
/// Private — callers must go through `run_primary_workspace_repair` (gated) or
/// `run_primary_workspace_repair_inner` (already-gated proof-token path).
///
/// When `existing_guard` is `Some`, the indexing helper is invoked via
/// `handle_index_command_with_guard` so we don't try to re-acquire the same
/// per-workspace mutex (which would deadlock — `tokio::sync::Mutex` is not
/// reentrant). When `None`, the gated entry point acquires its own gate.
async fn run_primary_workspace_repair_body(
    handler: &JulieServerHandler,
    existing_guard: Option<&MutationGuard<'_>>,
) -> Result<Option<PrimaryWorkspaceRepairPlan>> {
    let indexing_runtime = handler
        .primary_workspace_snapshot()
        .await
        .ok()
        .and_then(|snapshot| snapshot.indexing_runtime);

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
                    runtime.set_catchup_active(true);
                    runtime.set_watcher_paused(true);
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

                // `repair_rebuilds_embedding_inputs` covers reasons that
                // require running the embedding pipeline after repair:
                //   - Reasons that rebuild symbol text (EmptyDatabase,
                //     StaleFiles, NewFiles, DeletedFiles, ExtractorFailure,
                //     WatcherOverflow, SemanticVersionChanged) — embedding
                //     inputs changed and existing vectors are now stale.
                //   - MissingEmbeddings — symbols are intact but no
                //     vectors exist. The index path does not rebuild
                //     symbols here, but the "no files changed but
                //     embedding_count == 0" catch-up branch in
                //     `commands/index.rs` fires only when
                //     `skip_embeddings` is false. Including
                //     MissingEmbeddings here is what threads the catch-up
                //     through the existing scheduling logic instead of
                //     duplicating it.
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
                            | IndexingRepairReason::MissingEmbeddings
                    )
                });
                let skip_embeddings = !repair_rebuilds_embedding_inputs;
                match existing_guard {
                    Some(guard) => {
                        // Caller already holds the gate — call the variant
                        // that skips re-acquisition (would deadlock).
                        index_tool
                            .handle_index_command_with_guard(
                                handler,
                                None,
                                false,
                                skip_embeddings,
                                guard,
                            )
                            .await?;
                    }
                    None => {
                        // No gate held yet — let the gated entry point
                        // acquire it once it has resolved the workspace_id.
                        index_tool
                            .call_tool_with_options(handler, skip_embeddings)
                            .await?;
                    }
                }
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
            None => {
                // No file-level repair needed. Still check for Tantivy
                // projection lag: a crash between SQLite commit and Tantivy
                // apply leaves canonical_revision > projected_revision. This
                // case is invisible to the file-staleness scan above, so we
                // reconcile it here with a targeted Tantivy rebuild from the
                // already-correct SQLite state.
                reconcile_projection_lag_if_needed(handler).await?;
                Ok(None)
            }
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

/// Reconcile derived projection lag at startup / handoff.
///
/// A crash between the SQLite commit (which advances `canonical_revision`) and
/// the Tantivy apply leaves `canonical_revision > projected_revision`. The
/// file-staleness scan in `plan_primary_workspace_repair` cannot see this gap
/// because the source files are unchanged. This function detects the lag via
/// the `projection_states` table and calls `ensure_current_from_database` to
/// rebuild Tantivy from the already-correct canonical SQLite state, then stamps
/// `projected_revision = canonical_revision`.
///
/// Called from `run_primary_workspace_repair_body` when no file-level repair
/// was needed. Idempotent: if `projected_revision == canonical_revision` the
/// check returns immediately without touching the index.
async fn reconcile_projection_lag_if_needed(handler: &JulieServerHandler) -> Result<()> {
    let snapshot = match handler.primary_workspace_snapshot().await {
        Ok(s) => s,
        Err(_) => return Ok(()), // No workspace bound yet — nothing to reconcile
    };

    let search_index = snapshot.search_index;
    let workspace_id = snapshot.binding.workspace_id.clone();
    let db_arc = snapshot.database;

    let web_edges_rebuilt = {
        let mut db = db_arc.lock().unwrap_or_else(|p| p.into_inner());
        julie_pipeline::indexing_core::web_edges::ensure_web_edges_current(&mut db, &workspace_id)?
    };
    if web_edges_rebuilt {
        info!(%workspace_id, "Web-edge projection reconciled from canonical SQLite state");
    }

    let Some(search_index) = search_index else {
        return Ok(());
    };

    // Read projection and canonical revision under a short-lived lock so we
    // don't hold it across the potentially-expensive rebuild.
    let has_lag = {
        let db = db_arc.lock().unwrap_or_else(|p| p.into_inner());
        let canonical = db.get_latest_canonical_revision(&workspace_id)?;
        let Some(canonical) = canonical else {
            return Ok(()); // No canonical revision yet — nothing to reconcile
        };
        match db.get_projection_state(
            crate::search::projection::TANTIVY_PROJECTION_NAME,
            &workspace_id,
        )? {
            Some(state) => match state.projected_revision {
                Some(projected) => canonical.revision > projected,
                None => true, // canonical exists but projected is unset → lag
            },
            None => true, // no projection state at all → lag
        }
    };

    if !has_lag {
        return Ok(());
    }

    info!(
        %workspace_id,
        "📊 Projection lag detected (canonical_revision > projected_revision); \
         reconciling Tantivy from canonical SQLite state"
    );

    let projection = crate::search::SearchProjection::tantivy(workspace_id.clone());

    tokio::task::spawn_blocking(move || {
        let mut db = db_arc.lock().unwrap_or_else(|p| p.into_inner());
        let index = search_index;
        projection.ensure_current_from_database(&mut db, &index)?;
        info!(
            %workspace_id,
            "✅ Projection lag reconciled — Tantivy is now current with canonical SQLite state"
        );
        Ok::<_, anyhow::Error>(())
    })
    .await
    .map_err(|e| anyhow::anyhow!("Projection lag reconciliation task panicked: {}", e))??;

    Ok(())
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
    let db_arc = match route.database_for_read(handler).await? {
        Some(db) => db,
        None => {
            // Database file does not exist yet — workspace has never been indexed.
            // Treat as empty: schedule a full initial index.
            debug!(
                "No database at {} for workspace '{}' — initial indexing needed",
                db_path.display(),
                route.workspace_id
            );
            return Ok(Some(PrimaryWorkspaceRepairPlan {
                reasons: vec![IndexingRepairReason::EmptyDatabase],
            }));
        }
    };

    let (has_symbols_result, semantic_version_matches, indexed_files_raw, stored_repairs) = {
        // Keep the SQLite mutex scoped to database reads. Filesystem scans below
        // can be slow on large workspaces, and holding this lock makes first
        // health checks report a false SQLite BUSY state while catch-up is only
        // planning.
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

        let has_symbols_result = db.has_symbols_for_workspace();
        match has_symbols_result {
            Ok(true) => (
                Ok(true),
                Some(db.index_engine_version_matches(
                    &route.workspace_id,
                    SEMANTIC_INDEX_ENGINE_COMPONENT,
                    SEMANTIC_INDEX_ENGINE_VERSION,
                )?),
                db.get_all_indexed_files()?,
                db.list_indexing_repairs()?,
            ),
            Ok(false) => (Ok(false), None, Vec::new(), db.list_indexing_repairs()?),
            Err(err) => (Err(err), None, Vec::new(), Vec::new()),
        }
    };

    match has_symbols_result {
        Ok(has_symbols) => {
            if !has_symbols {
                info!("📊 Database is empty - indexing needed");
                let mut reasons = vec![IndexingRepairReason::EmptyDatabase];
                for repair in stored_repairs {
                    if let Some(reason) = IndexingRepairReason::from_str(&repair.reason) {
                        if !reasons.contains(&reason) {
                            reasons.push(reason);
                        }
                    }
                }
                return Ok(Some(PrimaryWorkspaceRepairPlan { reasons }));
            }

            let mut reasons = Vec::new();

            if !semantic_version_matches.unwrap_or(false) {
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

            // Database stores relative Unix-style paths per CLAUDE.md Path Handling Contract
            // No normalization needed - indexed_files are already relative
            let indexed_files: HashSet<String> = indexed_files_raw.into_iter().collect();

            let workspace_files =
                julie_core::workspace_scan::scan_workspace_files(&current_primary_root)?;
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

            for repair in stored_repairs {
                if let Some(reason) = IndexingRepairReason::from_str(&repair.reason) {
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
            }

            // Catch-up: the workspace may have been indexed before the
            // embedding sidecar finished bootstrapping (cold start can take
            // 30-60s while the index path runs in <5s). If symbols are
            // present but no embeddings exist, schedule embedding
            // generation instead of reporting "up-to-date" indefinitely.
            //
            // Only check when no other repair reason fires — if files
            // changed or the semantic version drifted, the full re-index
            // path already covers embedding regeneration. We also skip
            // this when MissingEmbeddings is already recorded as a stored
            // repair (handled in the loop above) to avoid double-counting.
            if reasons.is_empty() {
                let embedding_count = match db_arc.lock() {
                    Ok(db) => db.embedding_count().unwrap_or(0),
                    Err(poisoned) => poisoned.into_inner().embedding_count().unwrap_or(0),
                };
                if embedding_count == 0 {
                    // Skip MissingEmbeddings if an embedding task is
                    // already in flight for this workspace. Otherwise
                    // every concurrent session connect would see
                    // `embedding_count == 0` (the running task hasn't
                    // stored its first batch yet), build a
                    // MissingEmbeddings plan, and the body's
                    // `cancel_primary_embedding_task` call would kill
                    // and restart the in-flight task. Repeated session
                    // connects would cancel-loop the embedding pipeline
                    // indefinitely.
                    //
                    // We can't reach `handler.embedding_tasks` from this
                    // synchronous SQLite section without taking a tokio
                    // Mutex, so the lookup is hoisted out of the lock
                    // scope. Pattern mirrors the `task_already_running`
                    // guard in `commands/index.rs:462-465`.
                    let workspace_id = handler.require_primary_workspace_identity().ok();
                    let task_already_running = match workspace_id.as_ref() {
                        Some(ws_id) => handler.embedding_tasks.lock().await.contains_key(ws_id),
                        None => false,
                    };

                    if task_already_running {
                        debug!(
                            "Skipping MissingEmbeddings — embedding task already in flight \
                             for the primary workspace"
                        );
                    } else {
                        info!(
                            "📊 Workspace has symbols but 0 embeddings - scheduling catch-up embedding"
                        );
                        reasons.push(IndexingRepairReason::MissingEmbeddings);
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

        if !julie_core::workspace_scan::is_code_file(entry.path()) {
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
