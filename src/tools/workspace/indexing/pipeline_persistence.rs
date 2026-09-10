//! src/tools/workspace/indexing/pipeline_persistence.rs
//! Canonical persistence operations for the indexing pipeline.

use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;

use anyhow::Result;
use tracing::{debug, info, warn};

use super::route::IndexRoute;
use super::state::IndexingOperation;
use crate::database::SymbolDatabase;
use crate::handler::JulieServerHandler;
use crate::indexing_core::batch::ExtractedBatch;
use julie_core::Symbol;
use julie_core::workspace::mutation_gate::MutationGuard;
use julie_index::checkout_store::{CheckoutStore, PathChange};
use julie_pipeline::indexing_core::web_edges::rebuild_web_edges_for_workspace;

async fn store_for_route(
    handler: &JulieServerHandler,
    route: &IndexRoute,
) -> Result<Arc<CheckoutStore>> {
    if route.is_primary {
        if let Some(store) = handler.get_workspace().await?.and_then(|ws| ws.store) {
            return Ok(store);
        }
    }
    handler
        .checkout_store_for_workspace(&route.workspace_id, &route.workspace_root)
        .await
}

fn store_changes(
    route: &IndexRoute,
    batch: &ExtractedBatch,
    operation: IndexingOperation,
    store: &CheckoutStore,
) -> Vec<PathChange> {
    let upserted: HashSet<&str> = batch
        .all_file_infos
        .iter()
        .map(|info| info.path.as_str())
        .collect();
    let mut removed: BTreeSet<&str> = batch
        .files_to_clean
        .iter()
        .map(String::as_str)
        .filter(|path| !upserted.contains(path))
        .collect();
    let snapshot = store.current();
    if matches!(operation, IndexingOperation::Full) {
        removed.extend(
            snapshot
                .graph()
                .paths()
                .iter()
                .map(String::as_str)
                .filter(|path| !upserted.contains(path)),
        );
    }
    let mut changes: Vec<PathChange> = removed
        .into_iter()
        .map(|path| PathChange::Remove {
            path: path.to_string(),
        })
        .collect();
    for info in &batch.all_file_infos {
        let bytes = match &info.content {
            Some(content) => content.as_bytes().to_vec(),
            None => match std::fs::read(route.workspace_root.join(&info.path)) {
                Ok(bytes) => bytes,
                Err(_) => continue,
            },
        };
        changes.push(PathChange::Upsert {
            path: info.path.clone(),
            bytes,
            language: info.language.clone(),
        });
    }
    changes
}

/// Write the batch into the checkout store beside the canonical persist. A
/// failure is logged, not fatal, while the old store still serves the tools.
pub(crate) async fn apply_checkout_store(
    handler: &JulieServerHandler,
    route: &IndexRoute,
    batch: &ExtractedBatch,
    operation: IndexingOperation,
    guard: &MutationGuard<'_>,
) {
    let store = match store_for_route(handler, route).await {
        Ok(store) => store,
        Err(err) => {
            warn!(workspace_id = %route.workspace_id, "checkout store unavailable: {err:#}");
            return;
        }
    };
    let changes = store_changes(route, batch, operation, &store);
    match store.apply(&changes, guard) {
        Ok(applied) => info!(
            workspace_id = %route.workspace_id,
            new_blobs = applied.new_blobs,
            reused_blobs = applied.reused_blobs,
            removed_paths = applied.removed_paths,
            "Checkout store applied batch"
        ),
        Err(err) => {
            warn!(workspace_id = %route.workspace_id, "checkout store apply failed: {err:#}")
        }
    }
}

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
