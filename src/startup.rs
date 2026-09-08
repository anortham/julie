//! Startup and Indexing Utilities
//!
//! This module contains functions for workspace initialization, staleness detection,
//! and automatic indexing on server startup.

use crate::handler::JulieServerHandler;
use crate::tools::workspace::ManageWorkspaceTool;
use crate::tools::workspace::indexing::state::IndexingRepairReason;
use crate::workspace::startup_hint::WorkspaceStartupSource;
use anyhow::Result;
use std::time::Duration;
use tracing::{info, warn};

pub(crate) fn startup_source_prefers_request_roots(source: Option<WorkspaceStartupSource>) -> bool {
    matches!(source, Some(WorkspaceStartupSource::Cwd))
}
pub(crate) use crate::startup_repair_plan::{
    PrimaryWorkspaceRepairPlan, plan_primary_workspace_repair,
};

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
use julie_core::workspace::ownership::WriterPermit;

/// Perform primary workspace repair at startup or on leader promotion while
/// holding an authentic `WriterPermit`.
pub(crate) async fn run_primary_workspace_repair(
    handler: &JulieServerHandler,
) -> Result<Option<PrimaryWorkspaceRepairPlan>> {
    if handler.is_in_process_follower() {
        return Ok(None);
    }

    let Ok(workspace_id) = handler.require_primary_workspace_identity() else {
        return Ok(None);
    };

    let permit = match handler.acquire_writer_permit(&workspace_id).await {
        Ok(p) => p,
        Err(julie_core::workspace::ownership::OwnershipError::NotOwner) => return Ok(None),
        Err(e) => {
            return Err(anyhow::anyhow!(
                "Cannot acquire writer permit for repair: {e}"
            ));
        }
    };
    run_primary_workspace_repair_inner(&permit, handler).await
}

/// Inner repair implementation. Takes a `&WriterPermit<'_>` as a proof token
/// that the caller already holds an authentic OS owner permit.
pub(crate) async fn run_primary_workspace_repair_inner(
    permit: &WriterPermit<'_>,
    handler: &JulieServerHandler,
) -> Result<Option<PrimaryWorkspaceRepairPlan>> {
    run_primary_workspace_repair_body(handler, permit).await
}

/// The actual repair body requiring a live `WriterPermit`.
async fn run_primary_workspace_repair_body(
    handler: &JulieServerHandler,
    permit: &WriterPermit<'_>,
) -> Result<Option<PrimaryWorkspaceRepairPlan>> {
    let indexing_runtime = handler
        .primary_workspace_snapshot()
        .await
        .ok()
        .and_then(|snapshot| snapshot.indexing_runtime);

    let repair_result: Result<Option<PrimaryWorkspaceRepairPlan>> = async {
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
                index_tool
                    .handle_index_command_with_permit(handler, None, false, skip_embeddings, permit)
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

    let repair_plan = repair_result?;
    reconcile_projections_after_successful_repair(permit, handler).await?;
    Ok(repair_plan)
}

async fn reconcile_projections_after_successful_repair(
    permit: &WriterPermit<'_>,
    handler: &JulieServerHandler,
) -> Result<()> {
    reconcile_projection_lag_if_needed(permit, handler).await
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
            Ok(_) => {}
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
/// Called after every successful startup repair outcome. Idempotent: if each
/// projection is current, the checks return without touching derived state.
async fn reconcile_projection_lag_if_needed(
    _permit: &WriterPermit<'_>,
    handler: &JulieServerHandler,
) -> Result<()> {
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
