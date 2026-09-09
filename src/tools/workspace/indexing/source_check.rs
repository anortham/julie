//! src/tools/workspace/indexing/source_check.rs
//! Pre-commit source hash verification, batch partitioning, and dirty queue requeueing.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tracing::warn;

use crate::indexing_core::batch::ExtractedBatch;
use julie_core::database::FileInfo;
use julie_core::workspace::projection_stamp::SourceCheckState;

/// Result of pre-commit source verification and batch partitioning.
pub(crate) struct SourcePartitionResult {
    /// Batch containing only verified intact files.
    pub(crate) intact_batch: ExtractedBatch,
    /// Absolute paths of files detected as modified or deleted on disk during extraction.
    pub(crate) requeue_paths: Vec<PathBuf>,
    /// State describing the check outcome.
    pub(crate) state: SourceCheckState,
    /// Number of files verified intact.
    #[allow(dead_code)]
    pub(crate) verified_count: usize,
}

#[allow(dead_code)]
pub(crate) fn verify_source_hashes_before_commit(
    workspace_root: &Path,
    file_infos: &[FileInfo],
) -> SourceCheckState {
    let mut verified = 0;
    for file_info in file_infos {
        let abs_path = workspace_root.join(&file_info.path);
        if let Ok(content) = std::fs::read(&abs_path) {
            let disk_hash = hex::encode(blake3::hash(&content).as_bytes());
            if disk_hash != file_info.hash {
                return SourceCheckState::HashMismatch;
            }
            verified += 1;
        } else {
            return SourceCheckState::HashMismatch;
        }
    }
    SourceCheckState::Verified {
        files_checked: verified,
    }
}

pub(crate) fn filter_mismatched_batch_files(
    workspace_root: &Path,
    mut batch: ExtractedBatch,
) -> (ExtractedBatch, Vec<PathBuf>) {
    let mut intact_paths = HashSet::new();
    let mut mismatched_paths = HashSet::new();
    let mut requeue_paths = Vec::new();

    for file_info in &batch.all_file_infos {
        let abs_path = workspace_root.join(&file_info.path);
        match std::fs::read(&abs_path) {
            Ok(content) => {
                let disk_hash = hex::encode(blake3::hash(&content).as_bytes());
                if disk_hash == file_info.hash {
                    intact_paths.insert(file_info.path.clone());
                } else {
                    warn!(
                        path = %file_info.path,
                        expected_hash = %file_info.hash,
                        disk_hash = %disk_hash,
                        "Pre-commit hash mismatch: file was modified during extraction; excluding and requeueing"
                    );
                    mismatched_paths.insert(file_info.path.clone());
                    requeue_paths.push(abs_path);
                }
            }
            Err(e) => {
                warn!(
                    path = %file_info.path,
                    error = %e,
                    "Pre-commit disk read failed: file was deleted during extraction; excluding and requeueing"
                );
                mismatched_paths.insert(file_info.path.clone());
                requeue_paths.push(abs_path);
            }
        }
    }

    if !mismatched_paths.is_empty() {
        batch.retain_files(&intact_paths);
    }

    (batch, requeue_paths)
}

pub(crate) fn verify_and_partition_batch(
    workspace_root: &Path,
    batch: ExtractedBatch,
) -> SourcePartitionResult {
    let (intact_batch, requeue_paths) = filter_mismatched_batch_files(workspace_root, batch);
    let verified_count = intact_batch.all_file_infos.len();
    let state = if requeue_paths.is_empty() {
        SourceCheckState::Verified {
            files_checked: verified_count,
        }
    } else {
        SourceCheckState::RecheckRequeued {
            requeued_count: requeue_paths.len(),
        }
    };

    SourcePartitionResult {
        intact_batch,
        requeue_paths,
        state,
        verified_count,
    }
}
