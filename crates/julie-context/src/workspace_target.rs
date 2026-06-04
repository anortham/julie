/// Workspace targeting for tool operations.
///
/// Replaces the previous `Option<String>` / `Option<Vec<String>>` return types
/// from workspace resolution with an explicit enum that all tool callers match on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceTarget {
    /// Use the primary workspace (handler.get_workspace().db)
    Primary,
    /// Use a specific non-primary workspace by ID
    Target(String),
}
