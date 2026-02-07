// Julie MCP Tools - Modularized for maintainability and readability

// Shared infrastructure used by all tools
pub mod shared;

// Tool modules organized by functionality
pub mod deep_dive; // Progressive-depth symbol investigation
pub mod editing; // EditingTransaction infrastructure (shared by refactoring tools)
pub mod memory; // Memory system (checkpoint/recall)
pub mod navigation;
pub mod refactoring;
pub mod search;
pub mod symbols; // Symbol overview tools
pub mod workspace;

// Re-export all tools for external use
pub use deep_dive::DeepDiveTool;
pub use memory::{CheckpointTool, PlanTool, RecallTool}; // Memory system (checkpoint/recall/plans)
pub use navigation::FastRefsTool;
pub use refactoring::RenameSymbolTool;
pub use search::FastSearchTool;
pub use symbols::GetSymbolsTool;
pub use workspace::ManageWorkspaceTool;

// Re-export shared types and helpers
pub use shared::{
    BLACKLISTED_DIRECTORIES, BLACKLISTED_EXTENSIONS,
    KNOWN_CODE_EXTENSIONS, OptimizedResponse,
};

// Note: Tool registration now handled by rmcp #[tool_router] macro in handler.rs
