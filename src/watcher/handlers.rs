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
pub async fn handle_file_created_or_modified_static(
    path: PathBuf,
    db: &Arc<std::sync::Mutex<SymbolDatabase>>,
    extractor_manager: &Arc<ExtractorManager>,
    workspace_root: &Path,
    search_index: Option<&Arc<std::sync::Mutex<SearchIndex>>>,
) -> Result<()> {
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
                return Ok(());
            }
        }
    }

    // 4. Detect language and extract ALL data (symbols + identifiers + types + relationships)
    let language = path
        .extension()
        .and_then(|ext| ext.to_str())
        .and_then(|ext| language::detect_language_from_extension(ext))
        .map(|s| s.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let content_str = String::from_utf8_lossy(&content);

    let results =
        match extractor_manager.extract_all(&relative_path, &content_str, workspace_root) {
            Ok(results) => results,
            Err(e) => {
                error!("Extraction failed for {}: {}", relative_path, e);
                return Ok(()); // Skip update to preserve existing data
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

        // DEFENSIVE: Rollback any leaked transaction before starting new one
        let _ = db_lock.rollback_transaction();

        let existing_symbols = db_lock.get_symbols_for_file(&relative_path)?;

        // Safeguard against data loss
        if results.symbols.is_empty() && !existing_symbols.is_empty() {
            warn!(
                "SAFEGUARD: Refusing to delete {} existing symbols from {}",
                existing_symbols.len(),
                relative_path
            );
            return Ok(());
        }

        // Create file info and update hash
        let file_info = crate::database::create_file_info(&path, &language, workspace_root)?;
        let new_hash_str = hex::encode(new_hash.as_bytes());

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
    if let Some(search_index) = search_index {
        let symbol_docs: Vec<_> = results
            .symbols
            .iter()
            .map(crate::search::SymbolDocument::from_symbol)
            .collect();
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

            // Delete old documents for this file, then add new ones
            if let Err(e) = idx.remove_by_file_path(&file_to_clean) {
                warn!("Failed to remove Tantivy docs for {}: {}", file_to_clean, e);
            }
            for doc in &symbol_docs {
                if let Err(e) = idx.add_symbol(doc) {
                    warn!("Failed to add Tantivy symbol doc: {}", e);
                }
            }
            if let Err(e) = idx.commit() {
                warn!("Failed to commit Tantivy updates: {}", e);
            }
        })
        .await;

        if let Err(e) = tantivy_result {
            warn!("Tantivy update task panicked: {}", e);
        }
    }

    info!("Successfully indexed {}", path.display());
    Ok(())
}

/// Handle file deletion
pub async fn handle_file_deleted_static(
    path: PathBuf,
    db: &Arc<std::sync::Mutex<SymbolDatabase>>,
    workspace_root: &Path,
) -> Result<()> {
    info!("Processing file deletion: {}", path.display());

    // CRITICAL FIX: Convert absolute path to relative for database operations
    let relative_path = crate::utils::paths::to_relative_unix_style(&path, workspace_root)
        .context("Failed to convert path to relative")?;
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

    info!("Successfully removed indexes for {}", path.display());
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
) -> Result<()> {
    info!(
        "Handling file rename: {} -> {}",
        from.display(),
        to.display()
    );

    // Delete + create
    handle_file_deleted_static(from, db, workspace_root).await?;
    handle_file_created_or_modified_static(to, db, extractor_manager, workspace_root, search_index)
        .await?;

    Ok(())
}
