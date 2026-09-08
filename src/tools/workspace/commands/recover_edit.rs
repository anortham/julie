//! Handler for manage_workspace operation: recover_edit.

use crate::handler::JulieServerHandler;
use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use crate::workspace_runtime::source_edit::{
    RecoveryAction, SourceEditCoordinator, SourceEditError,
};
use anyhow::{Result, anyhow};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

pub(crate) async fn handle_recover_edit_command(
    handler: &JulieServerHandler,
    edit_id: &str,
    recovery_action: &str,
) -> Result<CallToolResult> {
    if edit_id.is_empty()
        || edit_id.contains('/')
        || edit_id.contains('\\')
        || edit_id.contains("..")
    {
        return Err(anyhow!(
            "Invalid edit_id '{edit_id}': must be an opaque identifier without path separators"
        ));
    }

    let action = RecoveryAction::parse(recovery_action)
        .map_err(|e| anyhow!("Invalid recovery_action '{recovery_action}': {e}"))?;

    let root = handler.require_primary_workspace_root()?;
    let coordinator = SourceEditCoordinator::new(root)?;

    let deadline = Instant::now() + Duration::from_secs(30);
    let cancellation = CancellationToken::new();

    match coordinator
        .recover(edit_id, action, deadline, &cancellation)
        .await
    {
        Ok(disposition) => {
            let summary = format!(
                "Edit recovery ({}) completed successfully for edit_id '{}'. Applied: {} file(s).",
                recovery_action,
                edit_id,
                disposition.applied_paths.len()
            );
            Ok(CallToolResult::text_content(vec![Content::text(summary)]))
        }
        Err(SourceEditError::RecoveryConflict {
            edit_id,
            disposition,
        }) => {
            let failure = SourceEditError::RecoveryConflict {
                edit_id,
                disposition,
            }
            .to_request_failure();
            Err(anyhow!(failure))
        }
        Err(e) => Err(anyhow!(e.to_request_failure())),
    }
}
