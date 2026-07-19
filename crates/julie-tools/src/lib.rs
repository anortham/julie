//! `julie-tools`: handler-free MCP tool implementations.
//!
//! This crate contains the 9 read/edit tool directories extracted from the
//! top-level `julie` crate as part of the Phase-2b relink-cure refactor.
//! All entry points take `&dyn julie_context::ToolContext`; no `JulieServerHandler`
//! reference exists in this crate.

pub mod deep_dive;
pub mod editing;
pub mod get_context;
pub mod impact;
pub mod navigation;
pub mod patterns;
pub mod refactoring;
pub mod search;
pub mod shared;
pub mod spillover;
pub mod symbols;

// Re-export the public tool types so the top-crate shim can re-export them.
pub use deep_dive::{DeepDiveDepth, DeepDiveTool};
pub use editing::EditingTransaction;
pub use get_context::GetContextTool;
pub use impact::BlastRadiusTool;
pub use navigation::{CallPathTool, FastRefsTool};
pub use patterns::{PatternsFormat, PatternsGroupBy, PatternsOperation, PatternsTool};
pub use refactoring::RenameSymbolTool;
pub use search::FastSearchTool;
pub use spillover::SpilloverGetTool;
pub use symbols::GetSymbolsTool;

pub use shared::{
    BLACKLISTED_DIRECTORIES, BLACKLISTED_EXTENSIONS, BLACKLISTED_FILENAMES, OptimizedResponse,
};

#[cfg(test)]
mod tests;
