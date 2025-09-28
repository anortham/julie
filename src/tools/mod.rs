// Julie MCP Tools - Modularized for maintainability and readability
// Each tool category is in its own file to avoid the monolithic tools.rs blocker

// Shared infrastructure used by all tools
pub mod shared;

// Tool modules organized by functionality
pub mod editing;
pub mod exploration;
pub mod navigation;
pub mod refactoring;
pub mod search;
pub mod workspace;

// Re-export all tools for external use
pub use editing::{FastEditTool, LineEditTool};
pub use exploration::{FastExploreTool, FindLogicTool};
pub use navigation::{FastGotoTool, FastRefsTool};
pub use refactoring::SmartRefactorTool;
pub use search::FastSearchTool;
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
        // Core tools - all migrated
        FastSearchTool,
        FastGotoTool,
        FastRefsTool,
        FastExploreTool,
        FindLogicTool,
        // Editing tools
        FastEditTool,
        LineEditTool,
        // Refactoring tools
        SmartRefactorTool,
        // Workspace management
        ManageWorkspaceTool,
    ]
);
