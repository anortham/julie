//! File change handlers for incremental indexing operations
//!
//! This module implements the core logic for handling Create, Modify, Delete,
//! and Rename operations on indexed files.

pub mod created_modified;
pub mod delete_rename;

pub use created_modified::handle_file_created_or_modified_static;
pub use delete_rename::handle_file_deleted_static;
pub(crate) use delete_rename::handle_file_renamed_static;

use julie_core::database::SymbolDatabase;
use julie_core::indexing_state::IndexingRepairReason;
use std::sync::Arc;
use tracing::warn;

/// Test-only injection: when set, `handle_file_created_or_modified_static` returns
/// `Err` after the SQLite commit and file-hash update but before the Tantivy apply,
/// reproducing a transient post-commit failure. Reset on read so it fires once.
#[cfg(test)]
pub(crate) static FAIL_AFTER_COMMIT_FOR_TEST: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileIndexOutcome {
    pub tantivy_ok: bool,
    pub repair_reason: Option<IndexingRepairReason>,
}

impl FileIndexOutcome {
    pub(crate) fn clean() -> Self {
        Self {
            tantivy_ok: true,
            repair_reason: None,
        }
    }

    pub(crate) fn repair_needed(tantivy_ok: bool, repair_reason: IndexingRepairReason) -> Self {
        Self {
            tantivy_ok,
            repair_reason: Some(repair_reason),
        }
    }
}

pub(crate) fn persist_repair_state(
    db: &Arc<std::sync::Mutex<SymbolDatabase>>,
    relative_path: &str,
    reason: IndexingRepairReason,
    detail: Option<&str>,
) {
    let db_lock = match db.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            warn!(
                "Database mutex poisoned during repair-state update, recovering: {}",
                poisoned
            );
            poisoned.into_inner()
        }
    };

    if let Err(err) = db_lock.record_indexing_repair(relative_path, reason.as_str(), detail) {
        warn!(
            "Failed to persist repair state for {} ({}): {}",
            relative_path, reason, err
        );
    }
}
