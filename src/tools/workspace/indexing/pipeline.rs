use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

use anyhow::Context;
use anyhow::Result;
use tracing::{debug, info, warn};

use super::finalize::{analyze_batch, resolve_pending_relationships};
use super::route::IndexRoute;
use super::state::{IndexedFileDisposition, IndexingBatchState, IndexingOperation, IndexingStage};
use crate::extractors::Symbol;
use crate::handler::JulieServerHandler;
use crate::indexing_core::batch::ExtractedBatch;
use crate::indexing_core::extraction::{
    ExtractedFileDisposition, ExtractedFileRecord, extract_files_for_indexing_with_records,
};
use crate::tools::workspace::commands::ManageWorkspaceTool;

pub(crate) struct IndexingPipelineResult {
    pub state: IndexingBatchState,
    pub files_processed: usize,
    pub canonical_revision: Option<i64>,
}

struct PersistBatchResult {
    canonical_revision: Option<i64>,
}

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
    let (mut batch, extracted_records) =
        extract_files_for_indexing_with_records(files_by_language, &route.workspace_root).await?;
    record_extracted_file_records(&mut state, extracted_records);

    // Classify test roles from annotation configs before persisting.
    // This enriches symbol metadata with test_role and is_test fields.
    {
        let configs = crate::search::LanguageConfigs::load_embedded();
        let role_configs = configs.build_test_role_configs();
        crate::analysis::test_roles::classify_symbols_by_role(
            &mut batch.all_symbols,
            &role_configs,
        );
    }

    let Some(db) = route.database_for_write(handler).await? else {
        transition_stage(&mut state, route, IndexingStage::Completed);
        update_runtime_finish(route, &state);
        return Ok(IndexingPipelineResult {
            state,
            files_processed: batch.files_processed,
            canonical_revision: None,
        });
    };

    transition_stage(&mut state, route, IndexingStage::Persisting);
    let persist_result = persist_batch(&db, route, operation, &batch)?;

    transition_stage(&mut state, route, IndexingStage::Resolving);
    resolve_pending_relationships(
        &db,
        &batch.all_pending_relationships,
        &batch.all_structured_pending_relationships,
    );

    transition_stage(&mut state, route, IndexingStage::Projecting);
    project_batch(
        &db,
        route,
        &batch,
        &mut state,
        persist_result.canonical_revision,
    )
    .await?;

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
        files_processed: batch.files_processed,
        canonical_revision: persist_result.canonical_revision,
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

fn persist_batch(
    db: &std::sync::Arc<std::sync::Mutex<crate::database::SymbolDatabase>>,
    route: &IndexRoute,
    operation: IndexingOperation,
    batch: &ExtractedBatch,
) -> Result<PersistBatchResult> {
    let bulk_start = std::time::Instant::now();
    let mut db_lock = match db.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            warn!(
                "Database mutex poisoned during canonical persistence, recovering: {}",
                poisoned
            );
            poisoned.into_inner()
        }
    };

    let stats = db_lock.get_stats().unwrap_or_default();
    let database_empty =
        stats.total_files == 0 && stats.total_symbols == 0 && stats.total_relationships == 0;
    let use_fresh_storage = matches!(operation, IndexingOperation::Full)
        || batch.files_to_clean.is_empty()
        || database_empty;

    let canonical_revision = if !use_fresh_storage {
        info!(
            "🔐 Starting ATOMIC incremental update: {} files to clean, {} symbols, {} relationships, {} files",
            batch.files_to_clean.len(),
            batch.all_symbols.len(),
            batch.all_relationships.len(),
            batch.all_file_infos.len()
        );

        db_lock.incremental_update_atomic(
            &batch.files_to_clean,
            &batch.all_file_infos,
            &batch.all_symbols,
            &batch.all_relationships,
            &batch.all_identifiers,
            &batch.all_types,
            &route.workspace_id,
        )?;
        let canonical_revision = db_lock.get_current_canonical_revision(&route.workspace_id)?;
        let successful_paths: Vec<String> = batch
            .all_file_infos
            .iter()
            .map(|file_info| file_info.path.clone())
            .collect();
        db_lock.clear_indexing_repairs(&successful_paths)?;
        store_parse_diagnostics(&db_lock, batch)?;
        for (path, detail) in &batch.repair_entries {
            db_lock.record_indexing_repair(
                path,
                crate::tools::workspace::indexing::state::IndexingRepairReason::ExtractorFailure
                    .as_str(),
                Some(detail),
            )?;
        }
        log_documentation_symbol_count(&batch.all_symbols);

        info!(
            workspace_id = %route.workspace_id,
            canonical_revision = canonical_revision,
            "Canonical persistence committed"
        );
        canonical_revision
    } else {
        if matches!(operation, IndexingOperation::Full) && !database_empty {
            let cleanup = db_lock.delete_workspace_data()?;
            info!(
                workspace_id = %route.workspace_id,
                symbols_deleted = cleanup.symbols_deleted,
                relationships_deleted = cleanup.relationships_deleted,
                files_deleted = cleanup.files_deleted,
                "Cleared canonical database state for full indexing"
            );
        }

        info!(
            "🔐 Starting ATOMIC fresh bulk storage of {} files, {} symbols, {} relationships...",
            batch.all_file_infos.len(),
            batch.all_symbols.len(),
            batch.all_relationships.len(),
        );

        db_lock.bulk_store_fresh_atomic(
            &batch.all_file_infos,
            &batch.all_symbols,
            &batch.all_relationships,
            &batch.all_identifiers,
            &batch.all_types,
            &route.workspace_id,
        )?;
        let canonical_revision = db_lock.get_current_canonical_revision(&route.workspace_id)?;
        let successful_paths: Vec<String> = batch
            .all_file_infos
            .iter()
            .map(|file_info| file_info.path.clone())
            .collect();
        db_lock.clear_indexing_repairs(&successful_paths)?;
        store_parse_diagnostics(&db_lock, batch)?;
        for (path, detail) in &batch.repair_entries {
            db_lock.record_indexing_repair(
                path,
                crate::tools::workspace::indexing::state::IndexingRepairReason::ExtractorFailure
                    .as_str(),
                Some(detail),
            )?;
        }
        log_documentation_symbol_count(&batch.all_symbols);

        info!(
            workspace_id = %route.workspace_id,
            canonical_revision = canonical_revision,
            "Canonical persistence committed"
        );
        canonical_revision
    };

    info!(
        "✅ Bulk storage complete in {:.2}s - data now persisted in SQLite!",
        bulk_start.elapsed().as_secs_f64()
    );

    Ok(PersistBatchResult { canonical_revision })
}

fn store_parse_diagnostics(
    db: &crate::database::SymbolDatabase,
    batch: &ExtractedBatch,
) -> Result<()> {
    for (path, diagnostics) in &batch.parse_diagnostics_by_file {
        db.store_file_parse_diagnostics(path, diagnostics)?;
    }
    Ok(())
}

async fn project_batch(
    db: &std::sync::Arc<std::sync::Mutex<crate::database::SymbolDatabase>>,
    route: &IndexRoute,
    batch: &ExtractedBatch,
    state: &mut IndexingBatchState,
    canonical_revision: Option<i64>,
) -> Result<()> {
    let symbols = batch.all_symbols.clone();
    let file_infos = batch.all_file_infos.clone();
    let files_to_clean = batch.files_to_clean.clone();

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

fn log_documentation_symbol_count(symbols: &[Symbol]) {
    let doc_count = symbols
        .iter()
        .filter(|symbol| symbol.language == "markdown")
        .count();
    if doc_count > 0 {
        debug!(
            "📚 Stored {} documentation symbols in symbols table",
            doc_count
        );
    }
}
