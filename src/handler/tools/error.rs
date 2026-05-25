//! Shared error helpers for handler tool wrappers.
//!
//! Tool wrappers in `src/handler/tools/*.rs` map `anyhow::Error` from the
//! inner tool implementation into `rmcp::ErrorData`. Classifying workspace
//! parameter failures as `invalid_params` (vs the default `internal_error`)
//! lets MCP clients tell the difference between user mistakes (bad
//! `workspace_id`) and genuine server errors.

use rmcp::ErrorData as McpError;

use crate::tools::navigation::resolution::workspace_resolution_failure_kind;

/// Map a tool failure to the appropriate `McpError`.
///
/// * Workspace-parameter failures (unknown workspace, not ready, swap in
///   progress, auto-activation failed — anything that downcasts to
///   `WorkspaceResolutionFailure`) become `invalid_params`.
/// * Everything else becomes `internal_error`.
///
/// The error message is prefixed with `"<tool_name> failed: "` so callers can
/// pass the same string to both metrics and the `McpError`.
pub(crate) fn classify_tool_failure(tool_name: &str, err: &anyhow::Error) -> McpError {
    let message = format!("{} failed: {}", tool_name, err);
    if workspace_resolution_failure_kind(err).is_some() {
        McpError::invalid_params(message, None)
    } else {
        McpError::internal_error(message, None)
    }
}
