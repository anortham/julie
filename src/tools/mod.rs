// Julie MCP Tools - Modularized for maintainability and readability

// Shared infrastructure used by all tools
pub mod shared;

// Tool modules organized by functionality
pub mod deep_dive; // Progressive-depth symbol investigation
pub mod editing; // EditingTransaction infrastructure (shared by refactoring tools)
pub mod get_context; // Token-budgeted code context subgraph
pub mod impact; // Change impact analysis and blast radius reporting
pub mod metrics; // Internal metrics formatting and session tracking
pub mod navigation;
pub mod refactoring;
pub mod search;
pub mod spillover;
pub mod symbols; // Symbol overview tools
pub mod workspace; // Workspace management and indexing

// Re-export all tools for external use
pub use deep_dive::{DeepDiveDepth, DeepDiveTool};
pub use get_context::GetContextTool;
pub use impact::BlastRadiusTool;
pub use navigation::{CallPathTool, FastRefsTool};
pub use refactoring::RenameSymbolTool;
pub use search::FastSearchTool;
pub use spillover::SpilloverGetTool;
pub use symbols::GetSymbolsTool;
pub use workspace::ManageWorkspaceTool;

// Re-export shared types and helpers
pub use shared::{
    BLACKLISTED_DIRECTORIES, BLACKLISTED_EXTENSIONS, BLACKLISTED_FILENAMES, KNOWN_CODE_EXTENSIONS,
    OptimizedResponse,
};

// Note: Tool registration now handled by rmcp #[tool_router] macro in handler.rs
