// Julie MCP Tools - Modularized for maintainability and readability
// Each tool category is in its own file to avoid the monolithic tools.rs blocker

// Shared infrastructure used by all tools
pub mod shared;

// Tool modules organized by functionality
pub mod ast_symbol_finder; // AST-aware symbol finding using tree-sitter
pub mod edit_lines; // Surgical line editing tool (insert/replace/delete)
pub mod editing; // EditingTransaction infrastructure (shared by all editing tools)
pub mod exploration;
pub mod fuzzy_replace; // DMP fuzzy matching tool
mod fuzzy_replace_algorithm; // Fuzzy replace algorithm (extracted from fuzzy_replace.rs)
pub mod memory; // Memory system (checkpoint/recall)
pub mod navigation;
pub mod refactoring;
pub mod search;
pub mod symbols; // Symbol overview tools
pub mod trace_call_path; // Cross-language call path tracing
pub mod workspace;

// Re-export all tools for external use
pub use edit_lines::EditLinesTool; // Surgical line editing (insert/replace/delete)
pub use editing::EditingTransaction; // Shared transaction infrastructure
pub use exploration::{FastExploreTool, FindLogicTool}; // Multi-mode exploration (NEW: FastExploreTool)
pub use fuzzy_replace::FuzzyReplaceTool; // DMP fuzzy matching
pub use memory::{CheckpointTool, PlanTool, RecallTool}; // Memory system (checkpoint/recall/plans)
pub use navigation::{FastGotoTool, FastRefsTool};
pub use refactoring::{EditSymbolTool, RenameSymbolTool};
// SmartRefactorTool is internal only - not exposed to MCP clients
pub use search::FastSearchTool;
pub use symbols::GetSymbolsTool;
pub use trace_call_path::TraceCallPathTool;
pub use workspace::ManageWorkspaceTool;

// Re-export shared types and helpers
pub use shared::{
    BLACKLISTED_DIRECTORIES, BLACKLISTED_EXTENSIONS,
    KNOWN_CODE_EXTENSIONS, OptimizedResponse,
};

// Note: Tool registration now handled by rmcp #[tool_router] macro in handler.rs
// The old tool_box! macro from rust-mcp-sdk has been removed
