//! src/tools/workspace/indexing/pipeline_persistence.rs
//! Canonical persistence operations for the indexing pipeline.

use anyhow::Result;
use tracing::{debug, info, warn};

use super::route::IndexRoute;
use super::state::IndexingOperation;
use crate::database::SymbolDatabase;
use crate::indexing_core::batch::ExtractedBatch;
use julie_core::Symbol;
use julie_pipeline::indexing_core::web_edges::rebuild_web_edges_for_workspace;

pub(crate) struct PersistBatchResult {
    pub(crate) canonical_revision: Option<i64>,
}

pub(crate) fn persist_batch(
    db: &std::sync::Arc<std::sync::Mutex<SymbolDatabase>>,
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

        db_lock.incremental_update_atomic_with_metadata(
            &batch.files_to_clean,
            &batch.canonical_write_set(),
            &route.workspace_id,
            crate::database::bulk::atomic::AtomicPersistenceMetadata::default(),
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

        db_lock.bulk_store_fresh_atomic_with_metadata(
            &batch.canonical_write_set(),
            &route.workspace_id,
            crate::database::bulk::atomic::AtomicPersistenceMetadata::default(),
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

    rebuild_web_edges_for_workspace(&mut db_lock, &route.workspace_id)?;

    info!(
        "✅ Bulk storage complete in {:.2}s - data now persisted in SQLite!",
        bulk_start.elapsed().as_secs_f64()
    );

    Ok(PersistBatchResult { canonical_revision })
}

#[cfg(test)]
pub(crate) fn persist_batch_for_test(
    db: &std::sync::Arc<std::sync::Mutex<SymbolDatabase>>,
    route: &IndexRoute,
    operation: IndexingOperation,
    batch: &ExtractedBatch,
) -> Result<PersistBatchResult> {
    persist_batch(db, route, operation, batch)
}

fn store_parse_diagnostics(db: &SymbolDatabase, batch: &ExtractedBatch) -> Result<()> {
    for (path, diagnostics) in &batch.parse_diagnostics_by_file {
        db.store_file_parse_diagnostics(path, diagnostics)?;
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
