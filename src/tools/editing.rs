//! Transactional Editing Infrastructure
//!
//! This module provides atomic file operation primitives used by all editing tools:
//! - EditingTransaction: Single-file atomic operations (temp file + rename)
//! - MultiFileTransaction: Multi-file all-or-nothing transactions
//!
//! These primitives ensure file safety across all editing tools in Julie.

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
    files: HashMap<PathBuf, String>, // file_path -> original_content
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

        // Read original content if file exists
        let original_content = if path.exists() {
            fs::read_to_string(&path)?
        } else {
            String::new()
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
        // Phase 0: Pre-flight validation - check if we can write to all target files
        for file_path in self.pending_content.keys() {
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
        for (file_path, content) in &self.pending_content {
            let temp_name = format!("{}.tmp.{}", file_path.display(), self.session_id);
            let temp_path = file_path.with_file_name(temp_name);

            fs::write(&temp_path, content)?;
            self.temp_files.push(temp_path);
        }

        // Phase 2: Atomic rename all temp files (commit point)
        for (i, (file_path, _)) in self.pending_content.iter().enumerate() {
            let temp_path = &self.temp_files[i];

            if let Err(e) = fs::rename(temp_path, file_path) {
                // If any rename fails, roll back all previous renames
                self.rollback_partial_commit(i)?;
                return Err(e.into());
            }
        }

        debug!(
            "Multi-file transaction committed: {} files",
            self.pending_content.len()
        );
        Ok(())
    }

    /// Rollback partial commit (used internally)
    fn rollback_partial_commit(&self, committed_count: usize) -> Result<()> {
        // Restore files that were successfully renamed
        for (i, (file_path, _)) in self.pending_content.iter().enumerate() {
            if i < committed_count {
                let original_content = &self.files[file_path];
                if let Err(e) = fs::write(file_path, original_content) {
                    warn!(
                        "Failed to restore file during rollback: {:?}: {}",
                        file_path, e
                    );
                }
            }
        }

        // Clean up remaining temp files
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
