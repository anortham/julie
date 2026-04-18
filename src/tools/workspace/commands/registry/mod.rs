// Workspace registry command implementations split into focused modules for maintainability.
// Each module contains related command handlers with <= 500 line limit per CLAUDE.md
//
// Module breakdown:
// - register_remove: workspace registration and deletion
// - cleanup: shared prune logic for manual and automatic workspace cleanup
// - list_clean: workspace listing and cleanup operations
// - refresh_stats: workspace re-indexing and statistics
// - health: comprehensive system health checks

pub use super::ManageWorkspaceTool;

// Split command implementations into logical modules
pub(crate) mod cleanup;
mod health;
mod list_clean;
mod open;
mod refresh_stats;
mod register_remove;
