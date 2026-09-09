//! Handler for file created or modified events.

use crate::watcher::extraction_write::WatcherExtractionWrite;
use anyhow::{Context, Result};
use julie_core::database::SymbolDatabase;
use julie_core::file_policy::{
    ExtractionMode, detect_language_for_indexing_with_content, determine_extraction_mode,
};
use julie_core::indexing_state::IndexingRepairReason;
use julie_core::workspace::mutation_gate::MutationGuard;
use julie_index::search::SearchIndex;
use julie_pipeline::finalize::resolve_pending_relationships;
use julie_pipeline::indexing_core::normalized::normalize_extraction_results;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use super::{FileIndexOutcome, persist_repair_state};

/// Handle file creation or modification with Blake3 change detection.
///
/// Extracts ALL data (symbols, identifiers, types, relationships), commits SQLite,
/// and stages Tantivy writes for the watcher runtime's batch commit. Pass `None`
/// for `search_index` if Tantivy updates are not needed (e.g., in tests).
///
/// Returns a repair-aware outcome so callers can track projection failures and
/// extraction drift without inferring meaning from a bare bool.
pub async fn handle_file_created_or_modified_static(
    path: PathBuf,
    db: &Arc<std::sync::Mutex<SymbolDatabase>>,
    workspace_root: &Path,
    search_index: Option<&Arc<SearchIndex>>,
    _guard: &MutationGuard<'_>,
) -> Result<FileIndexOutcome> {
    debug!("Processing file: {}", path.display());

    let content = tokio::fs::read(&path)
        .await
        .context("Failed to read file content")?;
    let new_hash = blake3::hash(&content);

    let relative_path = julie_core::paths::to_relative_unix_style(&path, workspace_root)
        .context("Failed to convert path to relative")?;

    {
        let db_lock = match db.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                warn!(
                    "Database mutex poisoned during file hash check, recovering: {}",
                    poisoned
                );
                poisoned.into_inner()
            }
        };
        if let Some(old_hash_str) = db_lock.get_file_hash(&relative_path)? {
            let new_hash_str = hex::encode(new_hash.as_bytes());
            if new_hash_str == old_hash_str {
                info!(
                    "Watcher: {} unchanged (hash match), skipping re-index",
                    relative_path
                );
                // Clear any stale repair entry so retry_persisted_repairs
                // doesn't re-dispatch this file every cycle.
                db_lock.clear_indexing_repair(&relative_path)?;
                return Ok(FileIndexOutcome::clean());
            }
        }
    }

    let content_str = String::from_utf8_lossy(&content).into_owned();
    let language =
        detect_language_for_indexing_with_content(Path::new(&relative_path), &content_str);
    let extraction_mode = determine_extraction_mode(&language, &content_str);

    let results = match extraction_mode {
        ExtractionMode::ParserBacked => {
            let relative_path_clone = relative_path.clone();
            let content_clone = content_str.clone();
            let workspace_root_clone = workspace_root.to_path_buf();
            match tokio::task::spawn_blocking(move || {
                julie_extractors::extract_canonical(
                    &relative_path_clone,
                    &content_clone,
                    &workspace_root_clone,
                )
            })
            .await
            {
                Ok(Ok(results)) => results,
                Ok(Err(e)) => {
                    error!("Extraction failed for {}: {}", relative_path, e);
                    persist_repair_state(
                        db,
                        &relative_path,
                        IndexingRepairReason::ExtractorFailure,
                        Some(&e.to_string()),
                    );
                    return Ok(FileIndexOutcome::repair_needed(
                        true,
                        IndexingRepairReason::ExtractorFailure,
                    ));
                }
                Err(e) => {
                    error!("Extraction task panicked for {}: {}", relative_path, e);
                    persist_repair_state(
                        db,
                        &relative_path,
                        IndexingRepairReason::ExtractorFailure,
                        Some(&format!("spawn_blocking panic: {e}")),
                    );
                    return Ok(FileIndexOutcome::repair_needed(
                        true,
                        IndexingRepairReason::ExtractorFailure,
                    ));
                }
            }
        }
        ExtractionMode::TextOnly => julie_extractors::ExtractionResults::empty(),
    };

    info!(
        "Watcher: extracted {} symbols, {} identifiers, {} relationships from {} ({})",
        results.symbols.len(),
        results.identifiers.len(),
        results.relationships.len(),
        relative_path,
        language
    );

    let configs = julie_index::search::LanguageConfigs::load_embedded();
    let normalized = match normalize_extraction_results(results, &content_str, &configs) {
        Ok(norm) => norm,
        Err(e) => {
            warn!(
                "Watcher: symbol normalization/projection failed for {}: {}",
                relative_path, e
            );
            persist_repair_state(
                db,
                &relative_path,
                IndexingRepairReason::ExtractorFailure,
                Some(&format!("symbol normalization/projection failed: {e}")),
            );
            return Ok(FileIndexOutcome::repair_needed(
                true,
                IndexingRepairReason::ExtractorFailure,
            ));
        }
    };
    let pending_relationships = normalized.pending_relationships.clone();
    let structured_pending_relationships = normalized.structured_pending_relationships.clone();
    let parse_diagnostics = normalized.parse_diagnostics.clone();

    let new_hash_str = hex::encode(new_hash.as_bytes());
    let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
    let file_info_rel_path = julie_core::paths::to_relative_unix_style(&canonical, workspace_root)
        .context("Failed to convert path to relative for file info")?;
    let metadata = std::fs::metadata(&path)
        .map_err(|e| anyhow::anyhow!("Failed to read metadata for {:?}: {}", path, e))?;
    let last_modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    let line_count = content_str.lines().count() as i32;
    let watcher_write = WatcherExtractionWrite {
        file_info: julie_core::database::FileInfo {
            path: file_info_rel_path,
            language: language.clone(),
            hash: new_hash_str.clone(),
            size: metadata.len() as i64,
            last_modified,
            last_indexed: 0,
            symbol_count: normalized.symbols.len() as i32,
            line_count,
            content: Some(content_str.clone()),
        },
        normalized,
    };

    let old_symbol_ids: Vec<String>;
    let new_symbol_ids: Vec<String>;
    let old_partner_set: HashSet<String>;

    let workspace_key = workspace_root.to_string_lossy();
    let workspace_id = crate::workspace::registry::generate_workspace_id(&workspace_key)
        .unwrap_or_else(|_| workspace_key.into_owned());

    {
        let mut db_lock = match db.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                warn!(
                    "Database mutex poisoned during file update, recovering: {}",
                    poisoned
                );
                poisoned.into_inner()
            }
        };

        let existing_symbols = db_lock.get_symbols_for_file(&relative_path)?;

        // Collect the IDs of symbols that have relationships with the current symbols.
        // These "partners" live in other files and their relationship_text may go stale
        // once we delete or replace the current file's symbols below.
        old_symbol_ids = existing_symbols.iter().map(|s| s.id.clone()).collect();
        old_partner_set = julie_index::search::projection::collect_relationship_partner_symbol_ids(
            &db_lock,
            &old_symbol_ids,
        )?
        .into_iter()
        .collect();

        // Safeguard against data loss
        if extraction_mode == ExtractionMode::ParserBacked
            && watcher_write.normalized.symbols.is_empty()
            && !existing_symbols.is_empty()
        {
            let detail = format!(
                "refused to drop {} existing symbols after empty extraction result",
                existing_symbols.len()
            );
            warn!(
                "SAFEGUARD: Refusing to delete {} existing symbols from {}",
                existing_symbols.len(),
                relative_path
            );
            let _ = db_lock.record_indexing_repair(
                &relative_path,
                IndexingRepairReason::ExtractorFailure.as_str(),
                Some(&detail),
            );
            return Ok(FileIndexOutcome::repair_needed(
                true,
                IndexingRepairReason::ExtractorFailure,
            ));
        }

        let files_to_clean = [relative_path.clone()];
        let write_set = watcher_write.canonical_write_set();
        db_lock.incremental_update_atomic_with_metadata(
            &files_to_clean,
            &write_set,
            &workspace_id,
            julie_core::database::bulk::atomic::AtomicPersistenceMetadata::default(),
        )?;

        // Recompute derived web edges on every watcher save.
        julie_pipeline::indexing_core::web_edges::rebuild_web_edges_for_workspace(
            &mut *db_lock,
            &workspace_id,
        )?;

        new_symbol_ids = watcher_write
            .normalized
            .symbols
            .iter()
            .map(|symbol| symbol.id.clone())
            .collect();

        db_lock.update_file_hash(&relative_path, &new_hash_str)?;
        db_lock.store_file_parse_diagnostics(&relative_path, &parse_diagnostics)?;
        db_lock.clear_indexing_repair(&relative_path)?;
    }

    #[cfg(test)]
    if super::FAIL_AFTER_COMMIT_FOR_TEST.swap(false, std::sync::atomic::Ordering::AcqRel) {
        anyhow::bail!("injected post-commit failure for test");
    }

    resolve_pending_relationships(
        db,
        &pending_relationships,
        &structured_pending_relationships,
    );

    let partner_symbol_ids = {
        let db_lock = match db.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                warn!(
                    "Database mutex poisoned during partner collection, recovering: {}",
                    poisoned
                );
                poisoned.into_inner()
            }
        };
        let new_partner_set: HashSet<String> =
            julie_index::search::projection::collect_relationship_partner_symbol_ids(
                &db_lock,
                &new_symbol_ids,
            )?
            .into_iter()
            .collect();
        let changed_symbol_ids: HashSet<&str> = old_symbol_ids
            .iter()
            .map(String::as_str)
            .chain(new_symbol_ids.iter().map(String::as_str))
            .collect();
        let mut candidates: Vec<String> = old_partner_set
            .union(&new_partner_set)
            .filter(|id| !changed_symbol_ids.contains(id.as_str()))
            .cloned()
            .collect();
        candidates.sort_unstable();
        candidates
    };

    let tantivy_ok = if let Some(search_index) = search_index {
        let symbols = watcher_write.normalized.symbols.clone();
        let file_to_clean = relative_path.clone();
        let file_content = content_str.clone();
        let file_language = language.clone();

        let search_index = Arc::clone(search_index);
        let db_for_tantivy = Arc::clone(db);
        let partner_ids_for_tantivy = partner_symbol_ids;
        let tantivy_result =
            tokio::task::spawn_blocking(move || {
                let idx = &*search_index;
                let db_guard = match db_for_tantivy.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => {
                        warn!("Database mutex poisoned during Tantivy projection, recovering");
                        poisoned.into_inner()
                    }
                };

                let ok =
                    match julie_index::search::projection::apply_uncommitted_documents_from_symbols(
                        &idx,
                        &symbols,
                        &file_to_clean,
                        &file_content,
                        &file_language,
                        std::slice::from_ref(&file_to_clean),
                        &db_guard,
                    ) {
                        Ok(()) => true,
                        Err(e) => {
                            warn!("Failed to update Tantivy docs for {}: {}", file_to_clean, e);
                            false
                        }
                    };

                let ok = if ok && !partner_ids_for_tantivy.is_empty() {
                    match julie_index::search::projection::reproject_partner_symbols(
                        &idx,
                        &db_guard,
                        &partner_ids_for_tantivy,
                    ) {
                        Ok(()) => true,
                        Err(e) => {
                            warn!(
                                "Failed to reproject {} relationship partner symbol(s): {}",
                                partner_ids_for_tantivy.len(),
                                e
                            );
                            false
                        }
                    }
                } else {
                    ok
                };

                ok
            })
            .await;

        match tantivy_result {
            Ok(ok) => ok,
            Err(e) => {
                warn!("Tantivy update task panicked: {}", e);
                false
            }
        }
    } else {
        true // No search index configured — nothing to fail
    };

    debug!("Watcher: indexed {}", relative_path);
    if tantivy_ok {
        Ok(FileIndexOutcome::clean())
    } else {
        Ok(FileIndexOutcome::repair_needed(
            false,
            IndexingRepairReason::ProjectionFailure,
        ))
    }
}
