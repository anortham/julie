//! Shared constants and types — relocated to `julie_core::shared`.
//!
//! All items re-exported from `julie_core` so existing `julie_core::shared::*`
//! import sites compile unchanged.
pub use julie_core::shared::{
    BLACKLISTED_DIRECTORIES, BLACKLISTED_EXTENSIONS, BLACKLISTED_FILENAMES, NOISE_CALLEE_NAMES,
    OptimizedResponse,
};

/// Trailer for a paged result: the exact call that returns the next page.
pub fn next_line<T: serde::Serialize>(tool: &str, args: &T, next_offset: usize) -> String {
    let mut args = serde_json::to_value(args).expect("tool arguments must serialize");
    args.as_object_mut()
        .expect("tool arguments must serialize as an object")
        .insert("offset".to_string(), serde_json::json!(next_offset));
    format!(
        "next: {tool} {}",
        serde_json::to_string(&args).expect("tool arguments must serialize as JSON")
    )
}

pub fn resolved_workspace(
    handler: &dyn julie_context::ToolContext,
    target: &julie_context::WorkspaceTarget,
) -> anyhow::Result<String> {
    match target {
        julie_context::WorkspaceTarget::Primary => handler.require_primary_workspace_identity(),
        julie_context::WorkspaceTarget::Target(id) => Ok(id.clone()),
    }
}
