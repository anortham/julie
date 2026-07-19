// Julie MCP Tools — shim layer
//
// The 9 handler-free tool dirs have been extracted into the `julie-tools` crate
// (Phase 2b, T2b.5b). This file re-exports them so all `crate::tools::*` paths
// in handler code, tests, and the rest of the top crate continue to resolve.
//
// Modules that remain in the top crate (handler-bound or too coupled to daemon):
pub mod metrics; // Internal metrics formatting and session tracking
pub mod workspace; // Workspace management and indexing

// The 9 extracted tool modules — re-exported from julie_tools so sub-paths work:
//   crate::tools::search::FastSearchTool  →  julie_tools::search::FastSearchTool
//   crate::tools::navigation::resolution::WorkspaceTarget  →  (and so on)
pub use julie_tools::deep_dive;
pub use julie_tools::editing;
pub use julie_tools::get_context;
pub use julie_tools::impact;
pub use julie_tools::navigation;
pub use julie_tools::patterns;
pub use julie_tools::refactoring;
pub use julie_tools::search;
pub use julie_tools::shared;
pub use julie_tools::spillover;
pub use julie_tools::symbols;

// Re-export all tools for external use (backward compat)
pub use deep_dive::{DeepDiveDepth, DeepDiveTool};
pub use get_context::GetContextTool;
pub use impact::BlastRadiusTool;
pub use navigation::{CallPathTool, FastRefsTool};
pub use patterns::{PatternsFormat, PatternsGroupBy, PatternsOperation, PatternsTool};
pub use refactoring::RenameSymbolTool;
pub use search::FastSearchTool;
pub use spillover::SpilloverGetTool;
pub use symbols::GetSymbolsTool;
pub use workspace::ManageWorkspaceTool;

// Re-export shared types and helpers
pub use shared::{
    BLACKLISTED_DIRECTORIES, BLACKLISTED_EXTENSIONS, BLACKLISTED_FILENAMES, OptimizedResponse,
};

// Note: Tool registration handled by rmcp #[tool_router] macro in handler.rs.
