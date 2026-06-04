//! Indexing state types — relocated to `julie_core::indexing_state`.
//!
//! All items re-exported so existing `crate::tools::workspace::indexing::state::*`
//! import sites compile unchanged.
pub use julie_core::indexing_state::{
    IndexedFileDisposition, IndexingBatchState, IndexingOperation, IndexingRepairReason,
    IndexingRuntimeSnapshot, IndexingRuntimeState, IndexingStage, SharedIndexingRuntime,
};

