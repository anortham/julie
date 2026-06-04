//! File policy — relocated to `julie_core::file_policy`.
//!
//! All items re-exported so existing `crate::tools::workspace::indexing::file_policy::*`
//! import sites compile unchanged.
pub use julie_core::file_policy::{
    ExtractionMode, allows_blacklisted_extension, detect_language_for_indexing,
    detect_language_for_indexing_with_content, determine_extraction_mode,
    should_index_path_candidate, should_process_deleted_path, should_watch_path,
    supported_extensions_for_indexing,
};
