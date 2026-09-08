use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

use anyhow::Context;
use anyhow::Result;
use tracing::{debug, info, warn};

use super::finalize::{analyze_batch, resolve_pending_relationships};
use super::route::IndexRoute;
use super::state::{IndexedFileDisposition, IndexingBatchState, IndexingOperation, IndexingStage};
use crate::handler::JulieServerHandler;
use crate::indexing_core::batch::ExtractedBatch;
use crate::indexing_core::extraction::{
    ExtractedFileDisposition, ExtractedFileRecord, extract_files_for_indexing_with_records,
};
use crate::tools::workspace::commands::ManageWorkspaceTool;

#[allow(unused_imports)]
pub(crate) use super::source_check::{
    filter_mismatched_batch_files, verify_and_partition_batch, verify_source_hashes_before_commit,
};

pub(crate) struct IndexingPipelineResult {
    pub state: IndexingBatchState,
    pub files_processed: usize,
    pub canonical_revision: Option<i64>,
    #[allow(dead_code)]
    pub source_check_state: julie_core::workspace::ownership::SourceCheckState,
}

pub(crate) use super::pipeline_persistence::persist_batch;
#[cfg(test)]
pub(crate) use super::pipeline_persistence::persist_batch_for_test;

pub(crate) async fn run_indexing_pipeline(
    tool: &ManageWorkspaceTool,
    handler: &JulieServerHandler,
    files_to_index: Vec<PathBuf>,
    route: &IndexRoute,
    operation: IndexingOperation,
) -> Result<IndexingPipelineResult> {
    let mut state = IndexingBatchState::new(route.workspace_id.clone());
    update_runtime_begin(route, operation);
    transition_stage(&mut state, route, IndexingStage::Grouped);

    let files_by_language = group_files_by_language(tool, files_to_index);
    info!("🚀 Processing {} languages", files_by_language.len());

    transition_stage(&mut state, route, IndexingStage::Extracting);
    let (batch, extracted_records) =
        extract_files_for_indexing_with_records(files_by_language, &route.workspace_root).await?;
    record_extracted_file_records(&mut state, extracted_records);

    // Test-role classification (and literal carrier gating) now happens inside
    // the shared chokepoint `extract_files_for_indexing_with_records` above, so
    // the live pipeline, the external-extract CLI, and the watcher all classify
    // through one source of truth. (Previously this was a pipeline-only call,
    // which is exactly why the extract DB Miller reads lacked `test_role`.)

    let Some(db) = route.database_for_write(handler).await? else {
        transition_stage(&mut state, route, IndexingStage::Completed);
        update_runtime_finish(route, &state);
        return Ok(IndexingPipelineResult {
            state,
            files_processed: batch.files_processed,
            canonical_revision: None,
            source_check_state: julie_core::workspace::ownership::SourceCheckState::Verified {
                files_checked: 0,
            },
        });
    };

    // 1. Pre-commit verify and partition batch
    let partition_result = verify_and_partition_batch(&route.workspace_root, batch);
    let batch = partition_result.intact_batch;
    let requeue_paths = partition_result.requeue_paths;
    let source_check_state = partition_result.state;

    // 2. Requeue dirty paths if any
    if !requeue_paths.is_empty() {
        warn!(
            workspace_id = %route.workspace_id,
            requeued_count = requeue_paths.len(),
            "Pre-commit source check detected modified or deleted files; excluding and requeueing"
        );
        for path in &requeue_paths {
            state.mark_repair_needed(format!(
                "file modified or deleted during extraction requeued: {}",
                path.display()
            ));
        }
        requeue_dirty_paths(handler, route, &requeue_paths).await;
    }

    // 3. Clean empty commit check
    if batch.all_file_infos.is_empty() && batch.files_to_clean.is_empty() {
        info!(
            workspace_id = %route.workspace_id,
            "Batch is empty after pre-commit partitioning; skipping commit without advancing canonical revision"
        );
        let current_canonical = {
            let db_lock = db.lock().unwrap_or_else(|p| p.into_inner());
            db_lock
                .get_current_canonical_revision(&route.workspace_id)
                .ok()
                .flatten()
        };
        transition_stage(&mut state, route, IndexingStage::Completed);
        update_runtime_finish(route, &state);
        return Ok(IndexingPipelineResult {
            state,
            files_processed: 0,
            canonical_revision: current_canonical,
            source_check_state,
        });
    }

    // 4. Acquire exclusive publication lock
    let publication_lock_path = route.publication_lock_path();
    let _publication_guard =
        julie_core::workspace::publication_lock::PublicationLock::acquire_exclusive_path(
            &publication_lock_path,
        )
        .context("acquiring exclusive publication lock for pipeline commit")?;

    transition_stage(&mut state, route, IndexingStage::Persisting);
    let persist_result = persist_batch(&db, route, operation, &batch)?;

    transition_stage(&mut state, route, IndexingStage::Resolving);
    resolve_pending_relationships(
        &db,
        &batch.all_pending_relationships,
        &batch.all_structured_pending_relationships,
    );

    if let Ok(barrier_dir) = std::env::var("JULIE_IPC_BARRIER_DIR") {
        if std::env::var("JULIE_FAULT_INJECTION").as_deref() == Ok("pause_after_canonical_commit") {
            let barrier_path = std::path::PathBuf::from(&barrier_dir);
            let reached = barrier_path.join("reached_canonical_commit");
            let release = barrier_path.join("release_canonical_commit");
            let _ = std::fs::write(&reached, "1");
            while !release.exists() {
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
        }
    }

    let files_processed = batch.files_processed;
    transition_stage(&mut state, route, IndexingStage::Projecting);
    project_batch(
        &db,
        route,
        batch,
        &mut state,
        persist_result.canonical_revision,
    )
    .await?;

    drop(_publication_guard);

    transition_stage(&mut state, route, IndexingStage::Analyzing);
    analyze_batch(handler, route, &db)?;

    if !state.repair_needed() {
        handler
            .indexing_status
            .search_ready
            .store(true, Ordering::Release);
        debug!("🔍 Search now available");
    } else {
        handler
            .indexing_status
            .search_ready
            .store(false, Ordering::Release);
        warn!(
            workspace_id = %route.workspace_id,
            repair_files = state.repair_file_count(),
            repair_issues = state.repair_issue_count(),
            "Search remains unready because projection or routing repair is needed"
        );
    }

    transition_stage(&mut state, route, IndexingStage::Completed);
    update_runtime_finish(route, &state);
    if state.repair_needed() {
        warn!(
            workspace_id = %route.workspace_id,
            repair_files = state.repair_file_count(),
            "Indexing completed with repair-needed files"
        );
    }

    Ok(IndexingPipelineResult {
        state,
        files_processed,
        canonical_revision: persist_result.canonical_revision,
        source_check_state,
    })
}

fn group_files_by_language(
    tool: &ManageWorkspaceTool,
    files_to_index: Vec<PathBuf>,
) -> HashMap<String, Vec<PathBuf>> {
    let mut files_by_language: HashMap<String, Vec<PathBuf>> = HashMap::new();

    for file_path in files_to_index {
        let language = tool.detect_language(&file_path);
        files_by_language
            .entry(language)
            .or_default()
            .push(file_path);
    }

    files_by_language
}

fn record_extracted_file_records(
    state: &mut IndexingBatchState,
    records: Vec<ExtractedFileRecord>,
) {
    for record in records {
        match record.disposition {
            ExtractedFileDisposition::Parsed => {
                state.record_file(
                    record.relative_path,
                    record.language,
                    IndexedFileDisposition::Parsed,
                    None,
                );
            }
            ExtractedFileDisposition::TextOnly => {
                state.record_file(
                    record.relative_path,
                    record.language,
                    IndexedFileDisposition::TextOnly,
                    None,
                );
            }
            ExtractedFileDisposition::RepairNeeded { detail } => {
                state.record_file(
                    record.relative_path,
                    record.language,
                    IndexedFileDisposition::RepairNeeded,
                    Some(detail),
                );
            }
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

fn update_runtime_finish(route: &IndexRoute, state: &IndexingBatchState) {
    if let Some(runtime) = route.indexing_runtime.as_ref() {
        let mut runtime = runtime
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        runtime.replace_repair_details(state.repair_issues().to_vec());
        runtime.finish_operation();
    }
}

async fn requeue_dirty_paths(handler: &JulieServerHandler, _route: &IndexRoute, paths: &[PathBuf]) {
    if let Some(ws) = handler.workspace.read().await.as_ref() {
        if let Some(watcher) = &ws.watcher {
            let mut q = watcher.index_queue.lock().await;
            for path in paths {
                let change_type = if path.exists() {
                    crate::watcher::FileChangeType::Modified
                } else {
                    crate::watcher::FileChangeType::Deleted
                };
                q.push_back(crate::watcher::FileChangeEvent {
                    path: path.clone(),
                    change_type,
                    timestamp: std::time::SystemTime::now(),
                });
            }
        }
    }
}

async fn project_batch(
    db: &std::sync::Arc<std::sync::Mutex<crate::database::SymbolDatabase>>,
    route: &IndexRoute,
    batch: ExtractedBatch,
    state: &mut IndexingBatchState,
    canonical_revision: Option<i64>,
) -> Result<()> {
    let ExtractedBatch {
        all_symbols: symbols,
        all_file_infos: file_infos,
        files_to_clean,
        ..
    } = batch;

    debug!(
        workspace_id = %route.workspace_id,
        canonical_revision = canonical_revision,
        "Starting projection phase"
    );

    let search_index = match route.search_index_for_write().await {
        Ok(search_index) => search_index,
        Err(e) => {
            warn!("Failed to open Tantivy index for projection: {}", e);
            if let Some(revision) = canonical_revision {
                if let Ok(db) = db.lock() {
                    let _ = db.upsert_projection_state(
                        crate::search::projection::TANTIVY_PROJECTION_NAME,
                        &route.workspace_id,
                        crate::database::ProjectionStatus::Stale,
                        Some(revision),
                        None,
                        Some(&e.to_string()),
                    );
                }
            }
            state.mark_repair_needed(match canonical_revision {
                Some(revision) => {
                    format!("tantivy projection unavailable at canonical revision {revision}: {e}")
                }
                None => format!("tantivy projection unavailable: {e}"),
            });
            return Ok(());
        }
    };

    if let Some(search_index) = search_index {
        let db = std::sync::Arc::clone(db);
        let workspace_id = route.workspace_id.clone();
        let tantivy_result = tokio::task::spawn_blocking(move || {
            crate::search::SearchProjection::tantivy(workspace_id)
                .project_documents_with_locks(
                    &db,
                    &search_index,
                    &symbols,
                    &file_infos,
                    &files_to_clean,
                    canonical_revision,
                )
                .context("projecting batch through SearchProjection")
        })
        .await;

        match tantivy_result {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                warn!("Tantivy projection failed: {e:#}");
                state.mark_repair_needed(match canonical_revision {
                    Some(revision) => {
                        format!("tantivy projection failed at canonical revision {revision}: {e}")
                    }
                    None => format!("tantivy projection failed: {e}"),
                });
            }
            Err(e) => {
                warn!("Tantivy indexing task panicked: {}", e);
                state.mark_repair_needed(match canonical_revision {
                    Some(revision) => format!(
                        "tantivy projection task panicked at canonical revision {revision}: {e}"
                    ),
                    None => format!("tantivy projection task panicked: {e}"),
                });
            }
        }
    }

    Ok(())
}
