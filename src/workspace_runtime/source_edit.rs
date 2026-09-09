//! Dedicated Coordinator for Follower & Leader Source Edits.
//!
//! Enforces OS-level advisory locking at `<canonical-root>/.julie/locks/source-edit.lock`,
//! bounded source reads, hash precondition validation, and durable atomic journaling.

pub use super::edit_journal::{EditDisposition, RecoveryAction};
use super::edit_journal::{EditJournal, JournalFileEntry, JournalFileState, JournalState};
pub use super::source_edit_ops::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio_util::sync::CancellationToken;

pub struct SourceEditCoordinator {
    canonical_root: PathBuf,
    config: SourceEditConfig,
}

impl SourceEditCoordinator {
    pub fn new(canonical_root: PathBuf) -> Result<Self, SourceEditError> {
        Self::with_config(canonical_root, SourceEditConfig::default())
    }

    pub fn with_config(
        canonical_root: PathBuf,
        config: SourceEditConfig,
    ) -> Result<Self, SourceEditError> {
        let canonical_root = canonical_root.canonicalize().unwrap_or(canonical_root);
        Ok(Self {
            canonical_root,
            config,
        })
    }

    pub fn canonical_root(&self) -> &Path {
        &self.canonical_root
    }

    pub fn lock_path(&self) -> PathBuf {
        self.canonical_root
            .join(".julie")
            .join("locks")
            .join("source-edit.lock")
    }

    pub fn journals_dir(&self) -> PathBuf {
        self.canonical_root.join(".julie").join("edit-journals")
    }

    pub fn resolve_path(&self, rel_path: &Path) -> Result<PathBuf, SourceEditError> {
        julie_core::file_utils::secure_path_resolution(
            &rel_path.to_string_lossy(),
            &self.canonical_root,
        )
        .map_err(|e| SourceEditError::InvalidArguments(e.to_string()))
    }

    pub async fn preview(
        &self,
        file_path: &Path,
        old_text: &str,
        new_text: &str,
    ) -> Result<EditPreview, SourceEditError> {
        let full_path = self.resolve_path(file_path)?;
        let original_bytes =
            read_bounded_source(&full_path, self.config.max_source_bytes, None, None)?;
        let original_content = String::from_utf8(original_bytes).map_err(|e| {
            SourceEditError::InvalidArguments(format!("File is not valid UTF-8: {e}"))
        })?;
        let before_hash = blake3::hash(original_content.as_bytes())
            .to_hex()
            .to_string();

        let modified_content = julie_tools::editing::edit_file::apply_edit(
            &original_content,
            old_text,
            new_text,
            "first",
        )
        .map_err(|e| SourceEditError::Conflict {
            path: file_path.to_path_buf(),
            expected_hash: before_hash.clone(),
            actual_hash: format!("match failed: {e}"),
        })?;

        let diff = julie_tools::editing::validation::format_unified_diff(
            &original_content,
            &modified_content,
            &file_path.to_string_lossy(),
        );

        Ok(EditPreview {
            file_path: file_path.to_path_buf(),
            old_text: old_text.to_string(),
            new_text: new_text.to_string(),
            diff,
            before_hash,
            after_bytes: modified_content.into_bytes(),
        })
    }

    pub fn read_source_bounded(
        &self,
        path: &Path,
        deadline: Instant,
        cancellation: &CancellationToken,
    ) -> Result<Vec<u8>, SourceEditError> {
        let full_path = self.resolve_path(path)?;
        read_bounded_source(
            &full_path,
            self.config.max_source_bytes,
            Some(deadline),
            Some(cancellation),
        )
    }

    pub async fn apply(
        &self,
        edit_id: &str,
        changes: &[PreparedSourceChange],
        deadline: Instant,
        cancellation: &CancellationToken,
    ) -> Result<EditDisposition, SourceEditError> {
        if cancellation.is_cancelled() {
            return Err(SourceEditError::Cancelled);
        }
        let _guard = acquire_source_edit_lock(&self.lock_path(), deadline, cancellation).await?;

        for change in changes {
            let full_path = self.resolve_path(&change.path)?;
            if let Ok(metadata) = fs::metadata(&full_path) {
                if metadata.permissions().readonly() {
                    return Err(SourceEditError::Io(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        format!("Target file '{}' is read-only", change.path.display()),
                    )));
                }
            }
            let current = read_bounded_source(
                &full_path,
                self.config.max_source_bytes,
                Some(deadline),
                Some(cancellation),
            )?;
            let current_hash = blake3::hash(&current).to_hex().to_string();
            if current_hash != change.before_hash {
                return Err(SourceEditError::Conflict {
                    path: change.path.clone(),
                    expected_hash: change.before_hash.clone(),
                    actual_hash: current_hash,
                });
            }
        }

        validate_ast_changes(changes, |p| self.resolve_path(p), deadline)?;

        let journal_path = self.journals_dir().join(format!("{edit_id}.json"));
        let journal_files = changes
            .iter()
            .map(|c| {
                let before_str = c
                    .before_bytes
                    .as_ref()
                    .and_then(|b| std::str::from_utf8(b).ok().map(|s| s.to_string()));
                let after_str = std::str::from_utf8(&c.after_bytes)
                    .ok()
                    .map(|s| s.to_string());
                JournalFileEntry {
                    path: c.path.clone(),
                    before_hash: c.before_hash.clone(),
                    after_hash: blake3::hash(&c.after_bytes).to_hex().to_string(),
                    before_bytes: before_str.clone(),
                    after_bytes: after_str.clone(),
                    before_bytes_hex: if before_str.is_none() {
                        c.before_bytes.as_ref().map(hex::encode)
                    } else {
                        None
                    },
                    after_bytes_hex: if after_str.is_none() {
                        Some(hex::encode(&c.after_bytes))
                    } else {
                        None
                    },
                    state: JournalFileState::Pending,
                }
            })
            .collect();
        let mut journal = EditJournal::new_empty(edit_id.to_string(), journal_files);
        journal.state = JournalState::Applying;
        journal.save(&journal_path)?;

        let mut applied_paths = Vec::new();

        for change in changes {
            if cancellation.is_cancelled() {
                return Err(SourceEditError::Cancelled);
            }
            if Instant::now() >= deadline {
                return Err(SourceEditError::DeadlineExceeded);
            }

            let full_path = self.resolve_path(&change.path)?;
            let parent = full_path.parent().unwrap_or(&self.canonical_root);
            fs::create_dir_all(parent)?;
            let temp_file = parent.join(format!(".tmp_edit_{}_{}", edit_id, applied_paths.len()));

            let existing_perms = fs::metadata(&full_path).ok().map(|m| m.permissions());
            if let Some(ref perms) = existing_perms {
                if perms.readonly() {
                    return Err(SourceEditError::Io(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        format!("Target file '{}' is read-only", change.path.display()),
                    )));
                }
            }
            fs::write(&temp_file, &change.after_bytes)?;
            if let Some(perms) = existing_perms {
                let _ = fs::set_permissions(&temp_file, perms);
            }
            fs::rename(&temp_file, &full_path)?;

            applied_paths.push(change.path.clone());
            let _ =
                journal.record_file_state(&change.path, JournalFileState::Applied, &journal_path);
        }

        journal.state = JournalState::Applied;
        let _ = journal.save(&journal_path);

        Ok(EditDisposition {
            edit_id: edit_id.to_string(),
            recovery_action: None,
            applied_paths,
            pending_paths: Vec::new(),
            conflicted_paths: Vec::new(),
            index_refresh_pending: false,
        })
    }

    pub async fn recover(
        &self,
        edit_id: &str,
        action: RecoveryAction,
        deadline: Instant,
        cancellation: &CancellationToken,
    ) -> Result<EditDisposition, SourceEditError> {
        if cancellation.is_cancelled() {
            return Err(SourceEditError::Cancelled);
        }
        let _guard = acquire_source_edit_lock(&self.lock_path(), deadline, cancellation).await?;
        let journal_path = self.journals_dir().join(format!("{edit_id}.json"));
        if !journal_path.exists() {
            return Err(SourceEditError::InvalidArguments(format!(
                "Journal '{edit_id}' not found"
            )));
        }
        let mut journal = EditJournal::load(&journal_path)?;

        if let Some(active) = &journal.active_recovery_action {
            if *active != action {
                return Err(SourceEditError::RecoveryActionConflict {
                    edit_id: edit_id.to_string(),
                    active: active.as_str().to_string(),
                    requested: action.as_str().to_string(),
                });
            }
        } else {
            journal.active_recovery_action = Some(action);
            journal.save(&journal_path)?;
        }

        if (action == RecoveryAction::Resume && journal.state == JournalState::Applied)
            || (action == RecoveryAction::Rollback && journal.state == JournalState::RolledBack)
        {
            let applied_paths = journal.files.iter().map(|f| f.path.clone()).collect();
            return Ok(EditDisposition {
                edit_id: edit_id.to_string(),
                recovery_action: Some(action),
                applied_paths,
                pending_paths: Vec::new(),
                conflicted_paths: Vec::new(),
                index_refresh_pending: false,
            });
        }

        let mut applied_paths = Vec::new();
        let pending_paths = Vec::new();
        let mut conflicted_paths = Vec::new();
        let mut write_ops = Vec::new();

        for file in &journal.files {
            let full_path = self.resolve_path(&file.path)?;
            let current = read_bounded_source(
                &full_path,
                self.config.max_source_bytes,
                Some(deadline),
                Some(cancellation),
            )?;
            let current_hash = blake3::hash(&current).to_hex().to_string();

            match action {
                RecoveryAction::Resume => {
                    if current_hash == file.after_hash {
                        applied_paths.push(file.path.clone());
                    } else if current_hash == file.before_hash {
                        let target_bytes = if let Some(ref text) = file.after_bytes {
                            text.as_bytes().to_vec()
                        } else if let Some(ref hex_str) = file.after_bytes_hex {
                            hex::decode(hex_str).map_err(|e| {
                                SourceEditError::InvalidArguments(format!(
                                    "Invalid hex payload: {e}"
                                ))
                            })?
                        } else {
                            return Err(SourceEditError::InvalidArguments(
                                "Missing after payload in journal".into(),
                            ));
                        };
                        let target_hash = blake3::hash(&target_bytes).to_hex().to_string();
                        if target_hash != file.after_hash {
                            return Err(SourceEditError::InvalidArguments(format!(
                                "Corrupted after payload for {}: expected hash {}, got {}",
                                file.path.display(),
                                file.after_hash,
                                target_hash
                            )));
                        }
                        write_ops.push((file.path.clone(), full_path, target_bytes));
                    } else {
                        conflicted_paths.push(file.path.clone());
                    }
                }
                RecoveryAction::Rollback => {
                    if current_hash == file.before_hash {
                        applied_paths.push(file.path.clone());
                    } else if current_hash == file.after_hash {
                        let target_bytes = if let Some(ref text) = file.before_bytes {
                            text.as_bytes().to_vec()
                        } else if let Some(ref hex_str) = file.before_bytes_hex {
                            hex::decode(hex_str).map_err(|e| {
                                SourceEditError::InvalidArguments(format!(
                                    "Invalid hex payload: {e}"
                                ))
                            })?
                        } else {
                            return Err(SourceEditError::InvalidArguments(
                                "Missing before payload in journal".into(),
                            ));
                        };
                        let target_hash = blake3::hash(&target_bytes).to_hex().to_string();
                        if target_hash != file.before_hash {
                            return Err(SourceEditError::InvalidArguments(format!(
                                "Corrupted before payload for {}: expected hash {}, got {}",
                                file.path.display(),
                                file.before_hash,
                                target_hash
                            )));
                        }
                        write_ops.push((file.path.clone(), full_path, target_bytes));
                    } else {
                        conflicted_paths.push(file.path.clone());
                    }
                }
            }
        }

        if !conflicted_paths.is_empty() {
            for path in &conflicted_paths {
                let _ =
                    journal.record_file_state(path, JournalFileState::Conflicted, &journal_path);
            }
            let disposition = EditDisposition {
                edit_id: edit_id.to_string(),
                recovery_action: Some(action),
                applied_paths,
                pending_paths,
                conflicted_paths,
                index_refresh_pending: false,
            };
            return Err(SourceEditError::RecoveryConflict {
                edit_id: edit_id.to_string(),
                disposition: Box::new(disposition),
            });
        }

        for (rel_path, full_path, target_bytes) in write_ops {
            let parent = full_path.parent().unwrap_or(&self.canonical_root);
            let temp_file =
                parent.join(format!(".tmp_recover_{}_{}", edit_id, applied_paths.len()));
            let existing_perms = fs::metadata(&full_path).ok().map(|m| m.permissions());
            fs::write(&temp_file, &target_bytes)?;
            if let Some(perms) = existing_perms {
                let _ = fs::set_permissions(&temp_file, perms);
            }
            fs::rename(&temp_file, &full_path)?;
            applied_paths.push(rel_path.clone());
            let st = if action == RecoveryAction::Resume {
                JournalFileState::Applied
            } else {
                JournalFileState::RolledBack
            };
            let _ = journal.record_file_state(&rel_path, st, &journal_path);
        }

        journal.state = if action == RecoveryAction::Resume {
            JournalState::Applied
        } else {
            JournalState::RolledBack
        };
        let _ = journal.save(&journal_path);

        let disposition = EditDisposition {
            edit_id: edit_id.to_string(),
            recovery_action: Some(action),
            applied_paths,
            pending_paths,
            conflicted_paths,
            index_refresh_pending: false,
        };
        let _ = journal.record_terminal_receipt(disposition.clone(), &journal_path);

        Ok(disposition)
    }
}
