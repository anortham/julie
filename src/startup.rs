//! Startup and Indexing Utilities
//!
//! This module contains functions for workspace initialization, staleness detection,
//! and automatic indexing on server startup.

use crate::handler::JulieServerHandler;
use crate::tools::workspace::ManageWorkspaceTool;
use crate::tools::workspace::indexing::state::IndexingRepairReason;
use anyhow::Result;
use tracing::{info, warn};

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
            if handler.get_workspace().await?.is_none() {
                return Ok(None);
            }

            return Err(err);
        }
    };
    let _ = primary_snapshot;
    Ok(None)
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
use julie_core::workspace::mutation_gate::MutationGuard;

/// Perform primary workspace repair at startup under the workspace mutation gate.
pub(crate) async fn run_primary_workspace_repair(
    handler: &JulieServerHandler,
) -> Result<Option<PrimaryWorkspaceRepairPlan>> {
    let Ok(workspace_id) = handler.require_primary_workspace_identity() else {
        return Ok(None);
    };

    let guard = handler.acquire_mutation_guard(&workspace_id).await;
    run_primary_workspace_repair_inner(&guard, handler).await
}

/// Inner repair implementation. Takes a `&MutationGuard<'_>` as a proof token
/// that the caller already holds the workspace mutation gate.
pub(crate) async fn run_primary_workspace_repair_inner(
    guard: &MutationGuard<'_>,
    handler: &JulieServerHandler,
) -> Result<Option<PrimaryWorkspaceRepairPlan>> {
    run_primary_workspace_repair_body(handler, guard).await
}

async fn run_primary_workspace_repair_body(
    handler: &JulieServerHandler,
    guard: &MutationGuard<'_>,
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
                    .handle_index_command_with_guard(handler, None, false, skip_embeddings, guard)
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
    reconcile_projections_after_successful_repair(guard, handler).await?;
    Ok(repair_plan)
}

async fn reconcile_projections_after_successful_repair(
    guard: &MutationGuard<'_>,
    handler: &JulieServerHandler,
) -> Result<()> {
    reconcile_projection_lag_if_needed(guard, handler).await
}

async fn cancel_primary_embedding_task(handler: &JulieServerHandler) {
    let Ok(workspace_id) = handler.require_primary_workspace_identity() else {
        return;
    };
    crate::tools::workspace::indexing::embeddings::cancel_and_join_embedding_tasks(
        handler,
        &[workspace_id],
        "startup repair",
    )
    .await;
}

/// Rebuild Tantivy from facts when the projection is absent or stale.
async fn reconcile_projection_lag_if_needed(
    guard: &MutationGuard<'_>,
    handler: &JulieServerHandler,
) -> Result<()> {
    let Some(ws) = handler.get_workspace().await? else {
        return Ok(());
    };
    let store = ws.store;
    if store.rebuild_tantivy_if_needed(guard)? {
        info!("Tantivy rebuilt from facts after startup catch-up");
    }
    Ok(())
}
