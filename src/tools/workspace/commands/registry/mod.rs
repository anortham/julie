// Workspace registry command implementations split into focused modules for maintainability.
// Each module contains related command handlers with <= 500 line limit per CLAUDE.md
//
// Module breakdown:
// - add_remove: workspace registration and deletion
// - list_clean: workspace listing and cleanup operations
// - refresh_stats: workspace re-indexing and statistics
// - health: comprehensive system health checks

pub use super::ManageWorkspaceTool;

// Split command implementations into logical modules
mod add_remove;
mod health;
mod list_clean;
mod refresh_stats;
