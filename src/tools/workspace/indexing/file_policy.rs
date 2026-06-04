//! File policy — relocated to `julie_core::file_policy`.
//!
//! Items re-exported so existing `crate::tools::workspace::indexing::file_policy::*`
//! import sites in the top crate compile unchanged. `should_watch_path` /
//! `should_process_deleted_path` are intentionally not re-exported here: their only
//! consumer (`src/watcher/filtering.rs`) now imports them directly from
//! `julie_core::file_policy` (Phase 2 PR 2a severance — watcher must not name
//! `crate::tools` ahead of its move to julie-runtime in 2c).
pub use julie_core::file_policy::{
    ExtractionMode, allows_blacklisted_extension, detect_language_for_indexing,
    detect_language_for_indexing_with_content, determine_extraction_mode,
    should_index_path_candidate, supported_extensions_for_indexing,
};
