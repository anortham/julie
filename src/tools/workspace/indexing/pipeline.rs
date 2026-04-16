use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use anyhow::Result;
use tracing::{debug, info, trace, warn};

use super::resolver;
use super::route::IndexRoute;
use super::state::{IndexedFileDisposition, IndexingBatchState, IndexingOperation, IndexingStage};
use crate::extractors::{Identifier, PendingRelationship, Relationship, Symbol};
use crate::handler::JulieServerHandler;
use crate::tools::workspace::commands::ManageWorkspaceTool;

pub(crate) struct IndexingPipelineResult {
    pub state: IndexingBatchState,
    pub files_processed: usize,
    pub canonical_revision: Option<i64>,
}

struct ExtractedBatch {
    all_symbols: Vec<Symbol>,
    all_relationships: Vec<Relationship>,
    all_pending_relationships: Vec<PendingRelationship>,
    all_identifiers: Vec<Identifier>,
    all_types: Vec<crate::extractors::base::TypeInfo>,
    all_file_infos: Vec<crate::database::FileInfo>,
    files_to_clean: Vec<String>,
    repair_entries: Vec<(String, String)>,
    files_processed: usize,
}

struct PersistBatchResult {
    canonical_revision: Option<i64>,
}

impl ExtractedBatch {
    fn new() -> Self {
        Self {
            all_symbols: Vec::new(),
            all_relationships: Vec::new(),
            all_pending_relationships: Vec::new(),
            all_identifiers: Vec::new(),
            all_types: Vec::new(),
            all_file_infos: Vec::new(),
            files_to_clean: Vec::new(),
            repair_entries: Vec::new(),
            files_processed: 0,
        }
    }
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
    let batch = tool
        .extract_index_batch(files_by_language, &route.workspace_root, &mut state)
        .await?;

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
    let persist_result = persist_batch(&db, route, &batch)?;

    transition_stage(&mut state, route, IndexingStage::Projecting);
    project_batch(
        &db,
        route,
        &batch,
        &mut state,
        persist_result.canonical_revision,
    )
    .await?;

    transition_stage(&mut state, route, IndexingStage::Resolving);
    resolve_pending_relationships(&db, &batch.all_pending_relationships);

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

impl ManageWorkspaceTool {
    async fn extract_index_batch(
        &self,
        files_by_language: HashMap<String, Vec<PathBuf>>,
        workspace_root: &Path,
        state: &mut IndexingBatchState,
    ) -> Result<ExtractedBatch> {
        let mut batch = ExtractedBatch::new();

        for (language, file_paths) in files_by_language {
            if file_paths.is_empty() {
                continue;
            }

            debug!(
                "Processing {} {} files with reused parser",
                file_paths.len(),
                language
            );

            if crate::language::get_tree_sitter_language(&language).is_ok() {
                for file_path in file_paths {
                    match self
                        .process_file_with_parser(&file_path, &language, workspace_root)
                        .await
                    {
                        Ok((
                            symbols,
                            relationships,
                            pending_rels,
                            identifiers,
                            types,
                            file_info,
                        )) => {
                            let relative_path =
                                relative_path_for_storage(&file_path, workspace_root);
                            state.record_file(
                                relative_path.clone(),
                                language.clone(),
                                IndexedFileDisposition::Parsed,
                                None,
                            );
                            batch.files_processed += 1;

                            trace!(
                                "File {} extracted {} symbols, {} pending relationships",
                                file_path.display(),
                                symbols.len(),
                                pending_rels.len()
                            );

                            batch.files_to_clean.push(relative_path);
                            batch.all_symbols.extend(symbols);
                            batch.all_relationships.extend(relationships);
                            batch.all_pending_relationships.extend(pending_rels);
                            batch.all_identifiers.extend(identifiers);
                            batch.all_types.extend(types.into_values());
                            batch.all_file_infos.push(file_info);

                            if batch.files_processed.is_multiple_of(50) {
                                debug!(
                                    "Progress: {} files processed, {} symbols collected",
                                    batch.files_processed,
                                    batch.all_symbols.len()
                                );
                            }
                        }
                        Err(e) => {
                            let relative_path =
                                relative_path_for_storage(&file_path, workspace_root);
                            warn!("Failed to process file {:?}: {}", file_path, e);
                            self.queue_failed_parser_file_for_cleanup(
                                &file_path,
                                &language,
                                workspace_root,
                                &mut batch.files_to_clean,
                                &mut batch.all_file_infos,
                            )
                            .await;
                            state.record_file(
                                relative_path.clone(),
                                language.clone(),
                                IndexedFileDisposition::RepairNeeded,
                                Some(e.to_string()),
                            );
                            batch.repair_entries.push((relative_path, e.to_string()));
                        }
                    }
                }
            } else {
                debug!(
                    "No parser for {} - indexing {} files for text search only",
                    language,
                    file_paths.len()
                );

                for file_path in file_paths {
                    match self
                        .process_file_without_parser(&file_path, &language, workspace_root)
                        .await
                    {
                        Ok((symbols, relationships, file_info)) => {
                            let relative_path =
                                relative_path_for_storage(&file_path, workspace_root);
                            debug!("📄 Processed file without parser: {:?}", file_path);
                            state.record_file(
                                relative_path.clone(),
                                language.clone(),
                                IndexedFileDisposition::TextOnly,
                                None,
                            );
                            batch.files_processed += 1;
                            batch.files_to_clean.push(relative_path);
                            batch.all_symbols.extend(symbols);
                            batch.all_relationships.extend(relationships);
                            batch.all_file_infos.push(file_info);
                        }
                        Err(e) => {
                            let relative_path =
                                relative_path_for_storage(&file_path, workspace_root);
                            warn!(
                                "Failed to process file without parser {:?}: {}",
                                file_path, e
                            );
                            self.queue_failed_parser_file_for_cleanup(
                                &file_path,
                                &language,
                                workspace_root,
                                &mut batch.files_to_clean,
                                &mut batch.all_file_infos,
                            )
                            .await;
                            state.record_file(
                                relative_path.clone(),
                                language.clone(),
                                IndexedFileDisposition::RepairNeeded,
                                Some(e.to_string()),
                            );
                            batch.repair_entries.push((relative_path, e.to_string()));
                        }
                    }
                }
            }
        }

        Ok(batch)
    }
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

fn relative_path_for_storage(file_path: &Path, workspace_root: &Path) -> String {
    if file_path.is_absolute() {
        crate::utils::paths::to_relative_unix_style(file_path, workspace_root)
            .unwrap_or_else(|_| file_path.to_string_lossy().replace('\\', "/"))
    } else {
        file_path.to_string_lossy().replace('\\', "/")
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
    batch: &ExtractedBatch,
) -> Result<PersistBatchResult> {
    let bulk_start = std::time::Instant::now();

    let canonical_revision = if !batch.files_to_clean.is_empty() {
        info!(
            "🔐 Starting ATOMIC incremental update: {} files to clean, {} symbols, {} relationships, {} files",
            batch.files_to_clean.len(),
            batch.all_symbols.len(),
            batch.all_relationships.len(),
            batch.all_file_infos.len()
        );

        let mut db_lock = match db.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                warn!(
                    "Database mutex poisoned during atomic incremental update, recovering: {}",
                    poisoned
                );
                poisoned.into_inner()
            }
        };

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
        info!(
            "🔐 Starting ATOMIC fresh bulk storage of {} files, {} symbols, {} relationships...",
            batch.all_file_infos.len(),
            batch.all_symbols.len(),
            batch.all_relationships.len(),
        );

        let mut db_lock = match db.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                warn!(
                    "Database mutex poisoned during fresh bulk storage, recovering: {}",
                    poisoned
                );
                poisoned.into_inner()
            }
        };

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

async fn project_batch(
    db: &std::sync::Arc<std::sync::Mutex<crate::database::SymbolDatabase>>,
    route: &IndexRoute,
    batch: &ExtractedBatch,
    state: &mut IndexingBatchState,
    canonical_revision: Option<i64>,
) -> Result<()> {
    let symbol_docs: Vec<_> = batch
        .all_symbols
        .iter()
        .map(crate::search::SymbolDocument::from_symbol)
        .collect();
    let file_docs: Vec<_> = batch
        .all_file_infos
        .iter()
        .map(crate::search::FileDocument::from_file_info)
        .collect();
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
            let mut db = match db.lock() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    warn!("Database mutex poisoned during projection write, recovering");
                    poisoned.into_inner()
                }
            };
            let idx = match search_index.lock() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    warn!("Search index mutex poisoned (prior panic during indexing), recovering");
                    poisoned.into_inner()
                }
            };
            let projection = crate::search::SearchProjection::tantivy(workspace_id);
            projection.project_documents(
                &mut db,
                &idx,
                &symbol_docs,
                &file_docs,
                &files_to_clean,
                canonical_revision,
            )
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

fn resolve_pending_relationships(
    db: &std::sync::Arc<std::sync::Mutex<crate::database::SymbolDatabase>>,
    pending_relationships: &[PendingRelationship],
) {
    if pending_relationships.is_empty() {
        return;
    }

    let resolution_start = std::time::Instant::now();
    let mut db_lock = match db.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            warn!("Database mutex poisoned during relationship resolution, recovering");
            poisoned.into_inner()
        }
    };

    let (resolved_relationships, stats) = resolver::resolve_batch(pending_relationships, &db_lock);
    if !resolved_relationships.is_empty() {
        if let Err(e) = db_lock.bulk_store_relationships(&resolved_relationships) {
            warn!("Failed to store resolved relationships: {}", e);
        }
    }

    stats.log_summary();
    info!(
        "⏱️  resolve_pending_relationships: {:.2}s",
        resolution_start.elapsed().as_secs_f64()
    );
}

fn analyze_batch(
    handler: &JulieServerHandler,
    route: &IndexRoute,
    db: &std::sync::Arc<std::sync::Mutex<crate::database::SymbolDatabase>>,
) -> Result<()> {
    let db_lock = match db.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            warn!("Database mutex poisoned during post-indexing analysis, recovering");
            poisoned.into_inner()
        }
    };

    let t = std::time::Instant::now();
    if let Err(e) = db_lock.compute_reference_scores() {
        warn!("Failed to compute reference scores: {}", e);
    }
    info!(
        "⏱️  compute_reference_scores: {:.2}s",
        t.elapsed().as_secs_f64()
    );

    let t = std::time::Instant::now();
    if let Err(e) = crate::analysis::compute_test_quality_metrics(&db_lock) {
        warn!("Failed to compute test quality metrics: {}", e);
    }
    info!(
        "⏱️  compute_test_quality_metrics: {:.2}s",
        t.elapsed().as_secs_f64()
    );

    if let Some(ref daemon_db) = handler.daemon_db {
        let current_primary_id = if route.is_primary {
            handler
                .current_workspace_id()
                .or_else(|| handler.loaded_workspace_id())
        } else {
            None
        };
        let snapshot_ws_id = current_primary_id.as_deref().unwrap_or(&route.workspace_id);
        if let Err(e) = daemon_db.snapshot_codehealth_from_db(snapshot_ws_id, &db_lock) {
            warn!("Failed to capture codehealth snapshot: {}", e);
        } else {
            info!(workspace_id = %snapshot_ws_id, "Codehealth snapshot captured");
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
