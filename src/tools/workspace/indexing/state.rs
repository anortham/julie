//! Indexing state types — relocated to `julie_core::indexing_state`.
//!
//! All items re-exported so existing `crate::tools::workspace::indexing::state::*`
//! import sites compile unchanged.
pub use julie_core::indexing_state::{
    IndexedFileDisposition, IndexingBatchState, IndexingOperation, IndexingRepairReason,
    IndexingRuntimeSnapshot, IndexingStage, SharedIndexingRuntime,
};
// Test-only since Phase 3d.2b removed the WorkspacePool consumer; re-exported so
// the `…::indexing::state::IndexingRuntimeState` test paths still resolve.
#[cfg(test)]
pub use julie_core::indexing_state::IndexingRuntimeState;

