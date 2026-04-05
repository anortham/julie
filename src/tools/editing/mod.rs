//! Transactional Editing Infrastructure
//!
//! This module provides atomic file operation primitives used by all editing tools:
//! - EditingTransaction: Single-file atomic operations (temp file + rename)
//! - MultiFileTransaction: Multi-file all-or-nothing transactions
//!
//! These primitives ensure file safety across all editing tools in Julie.

pub mod edit_file;
pub mod edit_symbol;
pub mod validation;

use anyhow::Result;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};
use uuid::Uuid;

//******************************************//
//     Transactional Editing System        //
//******************************************//

/// Memory-based single-file transaction system
///
/// Provides atomic file operations without creating persistent backup files.
/// Uses temp file + rename pattern for guaranteed atomicity.
pub struct EditingTransaction {
    file_path: PathBuf,
    original_content: Option<String>,
    temp_file_path: Option<PathBuf>,
}

impl EditingTransaction {
    /// Begin a new transaction for a file
    pub fn begin(file_path: &str) -> Result<Self> {
        let file_path = PathBuf::from(file_path);

        // Read original content if file exists
        let original_content = if file_path.exists() {
            Some(fs::read_to_string(&file_path)?)
        } else {
            None
        };

        debug!("Started transaction for: {}", file_path.display());

        Ok(EditingTransaction {
            file_path,
            original_content,
            temp_file_path: None,
        })
    }

    /// Commit new content to the file atomically
    pub fn commit(mut self, content: &str) -> Result<()> {
        // Generate unique temp file name
        let base_name = self
            .file_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("julie_edit");
        let temp_name = format!("{}.tmp.{}", base_name, Uuid::new_v4().simple());
        let temp_path = self.file_path.with_file_name(&temp_name);

        // Write to temp file first
        fs::write(&temp_path, content)?;
        self.temp_file_path = Some(temp_path.clone());

        // Atomic rename (this is the commit point)
        fs::rename(&temp_path, &self.file_path)?;

        debug!("Transaction committed for: {}", self.file_path.display());
        Ok(())
    }

    /// Rollback to original content
    pub fn rollback(self) -> Result<()> {
        match &self.original_content {
            Some(content) => {
                fs::write(&self.file_path, content)?;
                debug!("Transaction rolled back for: {}", self.file_path.display());
            }
            None => {
                // File didn't exist originally, remove it
                if self.file_path.exists() {
                    fs::remove_file(&self.file_path)?;
                    debug!(
                        "Transaction rolled back - removed file: {}",
                        self.file_path.display()
                    );
                }
            }
        }

        // Clean up temp file if it exists
        if let Some(temp_path) = &self.temp_file_path {
            if temp_path.exists() {
                let _ = fs::remove_file(temp_path);
            }
        }

        Ok(())
    }

    /// Emergency cleanup of orphaned temp files
    pub fn emergency_cleanup(directory: &Path) -> Result<()> {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();

            // Remove orphaned temp files
            if name.contains(".tmp.") {
                let path = entry.path();
                if let Err(e) = fs::remove_file(&path) {
                    warn!("Failed to clean up temp file {:?}: {}", path, e);
                } else {
                    debug!("Cleaned up orphaned temp file: {:?}", path);
                }
            }
        }
        Ok(())
    }
}

impl Drop for EditingTransaction {
    fn drop(&mut self) {
        // Clean up temp file on drop (if transaction wasn't committed)
        if let Some(temp_path) = &self.temp_file_path {
            if temp_path.exists() {
                let _ = fs::remove_file(temp_path);
            }
        }
    }
}

/// Memory-based multi-file transaction system
///
/// Provides all-or-nothing semantics across multiple files.
/// Either all files are updated successfully, or none are changed.
pub struct MultiFileTransaction {
    session_id: String,
    /// Maps each enrolled file to its pre-transaction content.
    /// `None` means the file did not exist before the transaction and must be
    /// DELETED (not truncated to empty) if rollback is needed.
    files: HashMap<PathBuf, Option<String>>,
    pending_content: HashMap<PathBuf, String>, // file_path -> new_content
    temp_files: Vec<PathBuf>,
}

impl MultiFileTransaction {
    /// Create a new multi-file transaction
    pub fn new(session_id: &str) -> Result<Self> {
        debug!("Started multi-file transaction: {}", session_id);

        Ok(MultiFileTransaction {
            session_id: session_id.to_string(),
            files: HashMap::new(),
            pending_content: HashMap::new(),
            temp_files: Vec::new(),
        })
    }

    /// Add a file to the transaction
    pub fn add_file(&mut self, file_path: &str) -> Result<()> {
        let path = PathBuf::from(file_path);

        // Store None when the file doesn't exist so rollback can DELETE it instead
        // of writing an empty placeholder.
        let original_content = if path.exists() {
            Some(fs::read_to_string(&path)?)
        } else {
            None
        };

        self.files.insert(path.clone(), original_content);
        debug!("Added file to transaction: {}", path.display());

        Ok(())
    }

    /// Set new content for a file in the transaction
    pub fn set_content(&mut self, file_path: &str, content: &str) -> Result<()> {
        let path = PathBuf::from(file_path);

        if !self.files.contains_key(&path) {
            return Err(anyhow::anyhow!(
                "File not added to transaction: {}",
                file_path
            ));
        }

        self.pending_content.insert(path, content.to_string());
        Ok(())
    }

    /// Commit all changes atomically
    pub fn commit_all(mut self) -> Result<()> {
        // Collect into a stable Vec once. Phase 1 builds self.temp_files in this order;
        // Phase 2 must iterate in the same order so temp_files[i] maps to the right
        // destination. HashMap iteration is non-deterministic across re-iterations.
        // Use std::mem::take because Drop prevents direct field moves from self.
        let file_list: Vec<(PathBuf, String)> = std::mem::take(&mut self.pending_content)
            .into_iter()
            .collect();

        // Phase 0: Pre-flight validation - check if we can write to all target files
        for (file_path, _) in &file_list {
            if file_path.exists() {
                // Check if file is readonly by trying to get metadata and permissions
                let metadata = fs::metadata(file_path)?;
                let permissions = metadata.permissions();
                if permissions.readonly() {
                    return Err(anyhow::anyhow!(
                        "Cannot write to readonly file: {}",
                        file_path.display()
                    ));
                }
            }
        }

        // Phase 1: Write all content to temp files
        for (file_path, content) in &file_list {
            let temp_name = format!("{}.tmp.{}", file_path.display(), self.session_id);
            let temp_path = file_path.with_file_name(temp_name);

            fs::write(&temp_path, content)?;
            self.temp_files.push(temp_path);
        }

        // Phase 2: Atomic rename all temp files (commit point)
        for (i, (file_path, _)) in file_list.iter().enumerate() {
            let temp_path = &self.temp_files[i];

            if let Err(e) = fs::rename(temp_path, file_path) {
                // Roll back already-committed renames using the same stable order
                let committed: Vec<PathBuf> =
                    file_list[..i].iter().map(|(p, _)| p.clone()).collect();
                self.rollback_partial_commit(&committed)?;
                return Err(e.into());
            }
        }

        debug!(
            "Multi-file transaction committed: {} files",
            file_list.len()
        );
        Ok(())
    }

    /// Rollback partial commit (used internally)
    fn rollback_partial_commit(&self, committed_paths: &[PathBuf]) -> Result<()> {
        // Restore files that were successfully renamed
        for file_path in committed_paths {
            match &self.files[file_path] {
                None => {
                    // File did not exist before the transaction: delete it rather than
                    // leaving behind an empty placeholder.
                    if file_path.exists() {
                        if let Err(e) = fs::remove_file(file_path) {
                            warn!(
                                "Failed to delete new file during rollback: {:?}: {}",
                                file_path, e
                            );
                        }
                    }
                }
                Some(original_content) => {
                    // File existed before: restore atomically via temp+rename to avoid
                    // partial writes if the process is interrupted mid-rollback.
                    let temp_name = format!(
                        "{}.rollback.{}",
                        file_path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("file"),
                        self.session_id
                    );
                    let temp_path = file_path.with_file_name(temp_name);
                    if let Err(e) = fs::write(&temp_path, original_content)
                        .and_then(|_| fs::rename(&temp_path, file_path))
                    {
                        // Clean up the temp if the rename failed
                        let _ = fs::remove_file(&temp_path);
                        warn!(
                            "Failed to restore file during rollback: {:?}: {}",
                            file_path, e
                        );
                    }
                }
            }
        }

        // Clean up remaining temp files (those not yet renamed)
        let committed_count = committed_paths.len();
        for (i, temp_path) in self.temp_files.iter().enumerate() {
            if i >= committed_count && temp_path.exists() {
                let _ = fs::remove_file(temp_path);
            }
        }

        Ok(())
    }
}

impl Drop for MultiFileTransaction {
    fn drop(&mut self) {
        // Clean up any remaining temp files
        for temp_path in &self.temp_files {
            if temp_path.exists() {
                let _ = fs::remove_file(temp_path);
            }
        }
    }
}

#[cfg(test)]
impl MultiFileTransaction {
    /// Test-only accessor: call rollback_partial_commit directly with an explicit set of
    /// committed paths.  Lets unit tests verify rollback behavior without having to trigger
    /// an actual Phase 2 rename failure.
    pub fn test_rollback_partial_commit(&self, committed: &[PathBuf]) -> Result<()> {
        self.rollback_partial_commit(committed)
    }
}
