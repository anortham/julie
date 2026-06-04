//! Symbol extraction for workspace indexing.
//!
//! `ManageWorkspaceTool::extract_symbols_static` was the only item here; it was a
//! thin wrapper over `crate::extractors::extract_canonical` and is now called
//! directly at the `indexing_core::extraction::process_file_with_parser` call site,
//! severing the `indexing_core → ManageWorkspaceTool` dependency edge.
