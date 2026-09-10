use std::sync::atomic::Ordering;

use anyhow::Result;
use julie_core::workspace::mutation_gate::MutationGuard;
use tracing::info;

use super::incremental::{existing_path_hashes, path_changes, scan_indexable_files};
use super::pipeline_persistence::{apply_changes, store_for_route};
use super::route::IndexRoute;
use super::state::{IndexedFileDisposition, IndexingBatchState, IndexingOperation, IndexingStage};
use crate::handler::JulieServerHandler;
use crate::tools::workspace::commands::ManageWorkspaceTool;
use julie_core::workspace::projection_stamp::SourceCheckState;
use julie_index::checkout_store::PathChange;

pub(crate) struct IndexingPipelineResult {
    pub state: IndexingBatchState,
    pub files_processed: usize,
    pub facts_revision: Option<i64>,
    #[allow(dead_code)]
    pub source_check_state: SourceCheckState,
}

pub(crate) async fn run_indexing_pipeline(
    _tool: &ManageWorkspaceTool,
    handler: &JulieServerHandler,
    files_to_index: Vec<std::path::PathBuf>,
    route: &IndexRoute,
    operation: IndexingOperation,
    guard: &MutationGuard<'_>,
) -> Result<IndexingPipelineResult> {
    let mut state = IndexingBatchState::new(route.workspace_id.clone());
    update_runtime_begin(route, operation);
    transition_stage(&mut state, route, IndexingStage::Grouped);

    transition_stage(&mut state, route, IndexingStage::Extracting);
    let scanned = scan_indexable_files(&route.workspace_root, &files_to_index)?;
    let store = store_for_route(handler, route).await?;
    let existing = existing_path_hashes(&store)?;
    let changes = path_changes(&scanned, &existing, operation);
    record_upserts(&mut state, &changes);

    transition_stage(&mut state, route, IndexingStage::Persisting);
    let applied = apply_changes(&store, &changes, guard)?;

    transition_stage(&mut state, route, IndexingStage::Resolving);
    transition_stage(&mut state, route, IndexingStage::Projecting);
    store.rebuild_tantivy_if_needed(guard)?;

    transition_stage(&mut state, route, IndexingStage::Analyzing);
    handler
        .indexing_status
        .search_ready
        .store(true, Ordering::Release);

    transition_stage(&mut state, route, IndexingStage::Completed);
    update_runtime_finish(route);

    let files_processed = applied.new_blobs + applied.reused_blobs;
    Ok(IndexingPipelineResult {
        state,
        files_processed,
        facts_revision: None,
        source_check_state: SourceCheckState::Verified {
            files_checked: scanned.len(),
        },
    })
}

fn record_upserts(state: &mut IndexingBatchState, changes: &[PathChange]) {
    for change in changes {
        if let PathChange::Upsert { path, language, .. } = change {
            state.record_file(
                path.clone(),
                language.clone(),
                IndexedFileDisposition::Parsed,
                None,
            );
        }
    }
}

fn transition_stage(state: &mut IndexingBatchState, route: &IndexRoute, stage: IndexingStage) {
    state.transition_to(stage);
    if let Some(runtime) = route.indexing_runtime.as_ref() {
        runtime
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .transition_stage(stage);
    }
    info!(
        workspace_id = %state.workspace_id,
        stage = %state.current_stage,
        repair_needed = state.repair_needed(),
        "Indexing stage transition"
    );
}

fn update_runtime_begin(route: &IndexRoute, operation: IndexingOperation) {
    if let Some(runtime) = route.indexing_runtime.as_ref() {
        runtime
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .begin_operation(operation);
    }
}

fn update_runtime_finish(route: &IndexRoute) {
    if let Some(runtime) = route.indexing_runtime.as_ref() {
        runtime
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .finish_operation();
    }
}
