//! File change handlers. Store writes live in the watcher queue processor.

pub mod created_modified;
pub mod delete_rename;

pub use created_modified::handle_file_created_or_modified_static;
pub use delete_rename::handle_file_deleted_static;
pub(crate) use delete_rename::handle_file_renamed_static;

use julie_core::indexing_state::IndexingRepairReason;

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
}
