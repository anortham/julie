//! File change handlers for incremental indexing operations
//!
//! This module implements the core logic for handling Create, Modify, Delete,
//! and Rename operations on indexed files.

use crate::database::SymbolDatabase;
use crate::extractors::ExtractorManager;
use crate::language; // Centralized language support
use crate::search::SearchIndex;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

/// Handle file creation or modification with Blake3 change detection.
///
/// Extracts ALL data (symbols, identifiers, types, relationships) and updates
/// both SQLite and Tantivy atomically. Pass `None` for `search_index` if
/// Tantivy updates are not needed (e.g., in tests).
///
/// Returns `Ok(true)` if both SQLite and Tantivy succeeded, `Ok(false)` if SQLite
/// succeeded but Tantivy had errors. The caller can use this to track files for
/// Tantivy retry on the next queue-processor tick.
pub async fn handle_file_created_or_modified_static(
    path: PathBuf,
    db: &Arc<std::sync::Mutex<SymbolDatabase>>,
    extractor_manager: &Arc<ExtractorManager>,
    workspace_root: &Path,
    search_index: Option<&Arc<std::sync::Mutex<SearchIndex>>>,
) -> Result<bool> {
    info!("Processing file: {}", path.display());

    // 1. Read file content and calculate hash
    let content = tokio::fs::read(&path)
        .await
        .context("Failed to read file content")?;
    let new_hash = blake3::hash(&content);

    // 2. Normalize path to relative Unix-style for database operations
    // CRITICAL FIX: Watcher provides absolute paths, but database stores relative paths
    // This caused hash lookups to fail, triggering unnecessary re-indexing on every save
    let relative_path = crate::utils::paths::to_relative_unix_style(&path, workspace_root)
        .context("Failed to convert path to relative")?;

    // 3. Check if file actually changed using Blake3
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
                debug!(
                    "File {} unchanged (Blake3 hash match), skipping",
                    path.display()
                );
                return Ok(true); // Hash match = nothing to do, not a failure
            }
        }
    }

    // 4. Detect language and extract ALL data (symbols + identifiers + types + relationships)
    let language = Path::new(&relative_path)
        .extension()
        .and_then(|ext| ext.to_str())
        .and_then(|ext| language::detect_language_from_extension(ext))
        .map(|s| s.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let content_str = String::from_utf8_lossy(&content);

    let results = match extractor_manager.extract_all(&relative_path, &content_str, workspace_root)
    {
        Ok(results) => results,
        Err(e) => {
            error!("Extraction failed for {}: {}", relative_path, e);
            return Ok(true); // Skip update to preserve existing data; not a Tantivy failure
        }
    };

    info!(
        "Extracted {} symbols, {} identifiers, {} relationships from {} ({})",
        results.symbols.len(),
        results.identifiers.len(),
        results.relationships.len(),
        path.display(),
        language
    );

    // 5. Update SQLite database atomically (symbols + identifiers + types + relationships)
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

        // Safeguard against data loss
        if results.symbols.is_empty() && !existing_symbols.is_empty() {
            warn!(
                "SAFEGUARD: Refusing to delete {} existing symbols from {}",
                existing_symbols.len(),
                relative_path
            );
            return Ok(true); // Safeguard skip; not a Tantivy failure
        }

        // Build FileInfo from the already-read content and hash to avoid a TOCTOU race:
        // create_file_info() re-reads the file, so a rapid save between the initial read
        // and this point would cause the stored hash to mismatch the stored content,
        // leading to perpetual re-indexing on every subsequent save.
        let new_hash_str = hex::encode(new_hash.as_bytes());
        let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
        let file_info_rel_path =
            crate::utils::paths::to_relative_unix_style(&canonical, workspace_root)
                .context("Failed to convert path to relative for file info")?;
        let metadata = std::fs::metadata(&path)
            .map_err(|e| anyhow::anyhow!("Failed to read metadata for {:?}: {}", path, e))?;
        let last_modified = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let file_content_str = String::from_utf8_lossy(&content).into_owned();
        let line_count = file_content_str.lines().count() as i32;
        let file_info = crate::database::FileInfo {
            path: file_info_rel_path,
            language: language.clone(),
            hash: new_hash_str.clone(),
            size: metadata.len() as i64,
            last_modified,
            last_indexed: 0,
            symbol_count: 0,
            line_count,
            content: Some(file_content_str),
        };

        // Convert types HashMap to Vec for bulk storage
        let types_vec: Vec<_> = results.types.into_values().collect();

        // Use incremental_update_atomic for a single atomic transaction that stores
        // ALL extracted data: symbols, identifiers, types, and relationships.
        // This replaces the old approach that only stored symbols.
        db_lock.incremental_update_atomic(
            &[relative_path.clone()],
            &[file_info],
            &results.symbols,
            &results.relationships,
            &results.identifiers,
            &types_vec,
            "", // workspace_id unused by this method
        )?;

        // Update file hash after successful atomic update
        db_lock.update_file_hash(&relative_path, &new_hash_str)?;
    }

    // 6. Update Tantivy search index (if available)
    // CRITICAL: Must re-add BOTH symbol docs AND file content doc after removal.
    // remove_by_file_path() deletes all doc types for the file path.
    // Fix B-b: Track Tantivy success so callers can add to a dirty-file retry set.
    let tantivy_ok = if let Some(search_index) = search_index {
        let symbol_docs: Vec<_> = results
            .symbols
            .iter()
            .map(crate::search::SymbolDocument::from_symbol)
            .collect();
        let file_content_doc = crate::search::FileDocument {
            file_path: relative_path.clone(),
            content: content_str.to_string(),
            language: language.clone(),
        };
        let file_to_clean = relative_path.clone();

        let search_index = Arc::clone(search_index);
        let tantivy_result = tokio::task::spawn_blocking(move || {
            let idx = match search_index.lock() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    warn!("Search index mutex poisoned, recovering");
                    poisoned.into_inner()
                }
            };

            let mut ok = true;
            // Delete old documents for this file, then add new ones
            if let Err(e) = idx.remove_by_file_path(&file_to_clean) {
                warn!("Failed to remove Tantivy docs for {}: {}", file_to_clean, e);
                ok = false;
            }
            for doc in &symbol_docs {
                if let Err(e) = idx.add_symbol(doc) {
                    warn!("Failed to add Tantivy symbol doc: {}", e);
                    ok = false;
                }
            }
            // Re-add file content doc (required for content search / line-mode search)
            if let Err(e) = idx.add_file_content(&file_content_doc) {
                warn!("Failed to add Tantivy file content doc: {}", e);
                ok = false;
            }
            // NOTE: commit is intentionally deferred — the caller batches
            // multiple file operations and commits once per tick to avoid
            // Tantivy segment-merge conflicts (FileDoesNotExist on .term files).
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

    info!("Successfully indexed {}", path.display());
    Ok(tantivy_ok)
}

/// Handle file deletion.
///
/// Fix B-a: The `path.exists()` guard has been removed. The caller (`dispatch_file_event`)
/// already performs this check before deciding to call this function. Having a second
/// check here creates a TOCTOU race: embeddings can be deleted by the caller while
/// this function bails out if the file is recreated between the two checks, leaving
/// symbols/Tantivy docs orphaned. Trust the caller's decision.
pub async fn handle_file_deleted_static(
    path: PathBuf,
    db: &Arc<std::sync::Mutex<SymbolDatabase>>,
    workspace_root: &Path,
    search_index: Option<&Arc<std::sync::Mutex<crate::search::SearchIndex>>>,
) -> Result<()> {
    info!("Processing file deletion: {}", path.display());

    // CRITICAL FIX: Convert absolute path to relative for database operations
    let relative_path = crate::utils::paths::to_relative_unix_style(&path, workspace_root)
        .context("Failed to convert path to relative")?;

    {
        let db_lock = match db.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                warn!(
                    "Database mutex poisoned during file deletion, recovering: {}",
                    poisoned
                );
                poisoned.into_inner()
            }
        };

        // Handle transient DELETE events gracefully (e.g., editor save operations)
        // Editors often delete-then-recreate files, causing DELETE events before the file
        // was ever indexed. "no such table" errors are harmless in this case.
        match db_lock.delete_symbols_for_file(&relative_path) {
            Ok(_) => {}
            Err(e) => {
                let err_msg = e.to_string();
                if err_msg.contains("no such table") {
                    // Transient state - file was never indexed, nothing to delete
                    info!("Skipping deletion for {} (not yet indexed)", path.display());
                    return Ok(());
                } else {
                    // Real error - propagate it
                    return Err(e);
                }
            }
        }

        match db_lock.delete_file_record(&relative_path) {
            Ok(_) => {}
            Err(e) => {
                let err_msg = e.to_string();
                if err_msg.contains("no such table") {
                    // Transient state - file record never existed
                    info!(
                        "Skipping file record deletion for {} (not yet indexed)",
                        path.display()
                    );
                    return Ok(());
                } else {
                    // Real error - propagate it
                    return Err(e);
                }
            }
        }
    } // db_lock is dropped here

    info!("Successfully removed indexes for {}", path.display());

    // Clean up Tantivy search index
    if let Some(search_index) = search_index {
        let search_index = Arc::clone(search_index);
        let rel_path = relative_path.clone();
        let tantivy_result = tokio::task::spawn_blocking(move || {
            let idx = match search_index.lock() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    warn!("Search index mutex poisoned during deletion, recovering");
                    poisoned.into_inner()
                }
            };
            if let Err(e) = idx.remove_by_file_path(&rel_path) {
                warn!("Failed to remove Tantivy docs for {}: {}", rel_path, e);
            }
            // NOTE: commit is intentionally deferred — the caller batches
            // multiple file operations and commits once per tick to avoid
            // Tantivy segment-merge conflicts (FileDoesNotExist on .term files).
        })
        .await;
        if let Err(e) = tantivy_result {
            warn!("Tantivy deletion task panicked: {}", e);
        }
    }

    Ok(())
}

/// Handle file rename
pub async fn handle_file_renamed_static(
    from: PathBuf,
    to: PathBuf,
    db: &Arc<std::sync::Mutex<SymbolDatabase>>,
    extractor_manager: &Arc<ExtractorManager>,
    workspace_root: &Path,
    search_index: Option<&Arc<std::sync::Mutex<SearchIndex>>>,
) -> Result<bool> {
    info!(
        "Handling file rename: {} -> {}",
        from.display(),
        to.display()
    );

    // Delete old path, then create/modify new path.
    // Returns the Tantivy success status from the create side so the caller
    // can track failures in the dirty-retry set.
    handle_file_deleted_static(from, db, workspace_root, search_index).await?;
    handle_file_created_or_modified_static(to, db, extractor_manager, workspace_root, search_index)
        .await
}
