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
pub mod navigation;
pub mod refactoring;
pub mod search;
pub mod symbols; // Symbol overview tools
pub mod trace_call_path; // Cross-language call path tracing
pub mod workspace;

// Re-export all tools for external use
pub use edit_lines::EditLinesTool; // Surgical line editing (insert/replace/delete)
pub use editing::EditingTransaction; // Shared transaction infrastructure
pub use exploration::FindLogicTool;
pub use fuzzy_replace::FuzzyReplaceTool; // DMP fuzzy matching
pub use navigation::{FastGotoTool, FastRefsTool};
pub use refactoring::SmartRefactorTool;
pub use search::FastSearchTool;
pub use symbols::GetSymbolsTool;
pub use trace_call_path::TraceCallPathTool;
pub use workspace::ManageWorkspaceTool;

// Re-export shared types
pub use shared::{
    OptimizedResponse, BLACKLISTED_DIRECTORIES, BLACKLISTED_EXTENSIONS, KNOWN_CODE_EXTENSIONS,
};

// Generate the JulieTools enum with tool_box! macro (gradually add tools as they are migrated)
use rust_mcp_sdk::tool_box;

// Generates the JulieTools enum with all tool variants
tool_box!(
    JulieTools,
    [
        // Search & Navigation tools
        FastSearchTool,
        FastGotoTool,
        FastRefsTool,
        GetSymbolsTool,
        TraceCallPathTool,
        FindLogicTool,
        // Editing tools
        EditLinesTool,
        FuzzyReplaceTool,
        // Semantic refactoring tool
        SmartRefactorTool,
        // Workspace management
        ManageWorkspaceTool,
    ]
);
