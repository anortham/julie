//! Event dispatching logic for the file watcher.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use tracing::{info, warn};

use super::handlers;
use super::types::{FileChangeEvent, FileChangeType};
use julie_core::database::SymbolDatabase;
use julie_core::indexing_state::{IndexingRepairReason, SharedIndexingRuntime};
use julie_core::workspace::mutation_gate::MutationGuard;

/// Dispatch a file event to appropriate handler.
///
/// Returns `Some(path)` when a DELETE event is skipped because the file still
/// exists (atomic-save pattern). The caller should remove that path from its
/// dedup map so the follow-up Create/Modify event is not suppressed (Fix F:
/// replaces the old detached `tokio::spawn` callback approach).
pub(crate) async fn dispatch_file_event(
    event: FileChangeEvent,
    db: &Arc<StdMutex<SymbolDatabase>>,
    search_index: &Option<Arc<julie_index::search::SearchIndex>>,
    embedding_provider: &Option<Arc<dyn julie_pipeline::embeddings::EmbeddingProvider>>,
    workspace_root: &Path,
    lang_configs: &Arc<julie_index::search::language_config::LanguageConfigs>,
    tantivy_dirty: &Arc<StdMutex<std::collections::HashSet<String>>>,
    indexing_runtime: &SharedIndexingRuntime,
    guard: &MutationGuard<'_>,
) -> Option<PathBuf> {
    let relative_for_embed =
        julie_core::paths::to_relative_unix_style(&event.path, workspace_root).ok();

    match event.change_type {
        FileChangeType::Created | FileChangeType::Modified => {
            let rel_path = relative_for_embed.clone();
            match handlers::handle_file_created_or_modified_static(
                event.path,
                db,
                workspace_root,
                search_index.as_ref(),
                guard,
            )
            .await
            {
                Err(e) => {
                    warn!("Failed to handle file change: {}", e);
                    // The handler can error after the SQLite commit advanced
                    // canonical_revision but before the Tantivy apply; mark dirty
                    // so the batch stamps Stale, not a false Ready.
                    if let Some(ref rel) = rel_path {
                        tantivy_dirty
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .insert(rel.clone());
                    }
                }
                Ok(outcome) => {
                    // Fix B-b: track Tantivy failures for retry on next tick
                    if !outcome.tantivy_ok {
                        if let Some(ref rel) = rel_path {
                            let mut dirty = tantivy_dirty.lock().unwrap_or_else(|p| p.into_inner());
                            dirty.insert(rel.clone());
                            warn!("Tantivy update failed for {}; queued for retry", rel);
                        }
                    }
                    if let Some(reason) = outcome.repair_reason {
                        indexing_runtime
                            .write()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .record_repair_reason(reason);
                        warn!(%reason, "Watcher repair needed after file change");
                    }
                    // Fix E: wrap blocking IPC call in spawn_blocking
                    if let (Some(provider), Some(rel)) = (embedding_provider, &rel_path) {
                        let db_clone = Arc::clone(db);
                        let provider_clone = Arc::clone(provider);
                        let rel_owned = rel.clone();
                        let lc = Arc::clone(lang_configs);
                        if let Err(e) = tokio::task::spawn_blocking(move || {
                            julie_pipeline::embeddings::pipeline::reembed_symbols_for_file(
                                &db_clone,
                                provider_clone.as_ref(),
                                &rel_owned,
                                Some(lc.as_ref()),
                            )
                        })
                        .await
                        {
                            warn!("Incremental embedding task panicked: {}", e);
                        }
                    }
                }
            }
            None
        }
        FileChangeType::Deleted => {
            // Guard: if the file still exists, this was likely an atomic
            // save (write-temp -> delete -> rename). Skip to avoid nuking
            // valid data - the subsequent Create/Modify event will re-index.
            if event.path.exists() {
                info!(
                    "Skipping DELETE for {} (file still exists, likely atomic save)",
                    event.path.display()
                );
                // Fix F: return the path for inline dedup-map clearing instead
                // of spawning a detached tokio task (W7).
                return Some(event.path);
            }

            if let Some(ref rel) = relative_for_embed {
                if let Ok(mut db_guard) = db.lock() {
                    if let Err(e) = db_guard.delete_embeddings_for_file(rel) {
                        warn!("Failed to delete embeddings for {}: {}", rel, e);
                    }
                }
            }
            if let Err(e) = handlers::handle_file_deleted_static(
                event.path,
                db,
                workspace_root,
                search_index.as_ref(),
                guard,
            )
            .await
            {
                warn!("Failed to handle file deletion: {}", e);
            }
            // Clear dirty-retry entry: file is deleted, retrying Tantivy
            // would recreate a phantom doc for a nonexistent file.
            if let Some(ref rel) = relative_for_embed {
                tantivy_dirty
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .remove(rel);
            }
            None
        }
        FileChangeType::Renamed { from, to } => {
            let rel_from = julie_core::paths::to_relative_unix_style(&from, workspace_root).ok();
            match handlers::handle_file_renamed_static(
                from,
                to.clone(),
                db,
                workspace_root,
                search_index.as_ref(),
                guard,
            )
            .await
            {
                Err(e) => {
                    indexing_runtime
                        .write()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .record_repair_reason(IndexingRepairReason::DeletedFiles);
                    warn!("Failed to handle file rename: {}", e);
                    // Same post-commit hazard as Created/Modified: queue the rename
                    // target for Tantivy retry so a mid-handler error cannot leave
                    // the projection falsely Ready.
                    if let Ok(rel_to) =
                        julie_core::paths::to_relative_unix_style(&to, workspace_root)
                    {
                        tantivy_dirty
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .insert(rel_to);
                    }
                }
                Ok(outcome) => {
                    let source_retired =
                        outcome.repair_reason != Some(IndexingRepairReason::ExtractorFailure);
                    if source_retired {
                        if let Some(ref rel_from) = rel_from {
                            if let Ok(mut db_guard) = db.lock() {
                                let _ = db_guard.delete_embeddings_for_file(rel_from);
                            }
                            // Clear old path from dirty-retry set only after the source
                            // has been retired successfully.
                            tantivy_dirty
                                .lock()
                                .unwrap_or_else(|p| p.into_inner())
                                .remove(rel_from);
                        }
                    }
                    // Track Tantivy failure on rename's create side for dirty-retry.
                    if !outcome.tantivy_ok {
                        if let Ok(ref rel_to) =
                            julie_core::paths::to_relative_unix_style(&to, workspace_root)
                        {
                            tantivy_dirty
                                .lock()
                                .unwrap_or_else(|p| p.into_inner())
                                .insert(rel_to.clone());
                            warn!(
                                "Tantivy update failed for rename target {}; queued for retry",
                                rel_to
                            );
                        }
                    }
                    if let Some(reason) = outcome.repair_reason {
                        indexing_runtime
                            .write()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .record_repair_reason(reason);
                        warn!(%reason, "Watcher repair needed after file rename");
                    }
                }
            }
            if let (Some(provider), Ok(rel_to)) = (
                embedding_provider,
                julie_core::paths::to_relative_unix_style(&to, workspace_root),
            ) {
                // Fix E: wrap blocking IPC call in spawn_blocking
                let db_clone = Arc::clone(db);
                let provider_clone = Arc::clone(provider);
                let rel_owned = rel_to.clone();
                let lc = Arc::clone(lang_configs);
                if let Err(e) = tokio::task::spawn_blocking(move || {
                    julie_pipeline::embeddings::pipeline::reembed_symbols_for_file(
                        &db_clone,
                        provider_clone.as_ref(),
                        &rel_owned,
                        Some(lc.as_ref()),
                    )
                })
                .await
                {
                    warn!("Incremental embedding task panicked for rename: {}", e);
                }
            }
            None
        }
    }
}
