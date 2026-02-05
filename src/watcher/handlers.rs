//! File change handlers for incremental indexing operations
//!
//! This module implements the core logic for handling Create, Modify, Delete,
//! and Rename operations on indexed files.

use crate::database::SymbolDatabase;
use crate::extractors::ExtractorManager;
use crate::language; // Centralized language support
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

/// Handle file creation or modification with Blake3 change detection
pub async fn handle_file_created_or_modified_static(
    path: PathBuf,
    db: &Arc<std::sync::Mutex<SymbolDatabase>>,
    extractor_manager: &Arc<ExtractorManager>,
    workspace_root: &Path,
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

    // 4. Detect language and extract symbols
    let language = path
        .extension()
        .and_then(|ext| ext.to_str())
        .and_then(|ext| language::detect_language_from_extension(ext))
        .map(|s| s.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let content_str = String::from_utf8_lossy(&content);

    let symbols =
        match extractor_manager.extract_symbols(&relative_path, &content_str, workspace_root) {
            Ok(symbols) => symbols,
            Err(e) => {
                error!("Symbol extraction failed for {}: {}", relative_path, e);
                return Ok(()); // Skip update to preserve existing data
            }
        };

    info!(
        "Extracted {} symbols from {} ({})",
        symbols.len(),
        path.display(),
        language
    );

    // 5. Update SQLite database
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
        // This handles cases where a previous operation failed mid-transaction
        // SQLite doesn't have a "check if transaction is open" API, so we speculatively rollback
        // If no transaction is open, this is a no-op (SQLite ignores ROLLBACK outside transaction)
        let _ = db_lock.rollback_transaction(); // Ignore error - transaction might not be open

        let existing_symbols = db_lock.get_symbols_for_file(&relative_path)?;

        // Safeguard against data loss
        if symbols.is_empty() && !existing_symbols.is_empty() {
            warn!(
                "SAFEGUARD: Refusing to delete {} existing symbols from {}",
                existing_symbols.len(),
                relative_path
            );
            return Ok(());
        }

        // Use transaction for atomic updates
        // CRITICAL: Ensure rollback on ANY error to prevent transaction leaks
        db_lock.begin_transaction()?;

        // Perform all database operations, capturing errors instead of using `?`
        let transaction_result = (|| -> Result<()> {
            // Ensure file record exists (required for foreign key constraint)
            let file_info = crate::database::create_file_info(&path, &language, workspace_root)?;
            db_lock.store_file_info(&file_info)?;

            // Delete old symbols for this file
            db_lock.delete_symbols_for_file(&relative_path)?;

            // Insert new symbols (within the transaction)
            db_lock.store_symbols(&symbols)?;

            // Update file hash
            let new_hash_str = hex::encode(new_hash.as_bytes());
            db_lock.update_file_hash(&relative_path, &new_hash_str)?;

            Ok(())
        })();

        // Handle transaction result: commit on success, rollback on error
        match transaction_result {
            Ok(()) => {
                db_lock.commit_transaction()?;
            }
            Err(e) => {
                // Rollback transaction before returning error
                if let Err(rollback_err) = db_lock.rollback_transaction() {
                    warn!(
                        "Failed to rollback transaction after error: {}. Original error: {}",
                        rollback_err, e
                    );
                }
                return Err(e);
            }
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
) -> Result<()> {
    info!(
        "Handling file rename: {} -> {}",
        from.display(),
        to.display()
    );

    // Delete + create
    handle_file_deleted_static(from, db, workspace_root).await?;
    handle_file_created_or_modified_static(to, db, extractor_manager, workspace_root).await?;

    Ok(())
}
