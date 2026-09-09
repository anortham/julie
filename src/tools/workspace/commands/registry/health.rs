use super::ManageWorkspaceTool;
use crate::handler::JulieServerHandler;
use crate::health::{HealthChecker, PrimaryWorkspaceHealth};
use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use anyhow::Result;
use tracing::info;

impl ManageWorkspaceTool {
    /// Handle health command with the shared health snapshot model.
    pub(crate) async fn handle_health_command(
        &self,
        handler: &JulieServerHandler,
        detailed: bool,
    ) -> Result<CallToolResult> {
        info!(
            "Performing comprehensive system health check (detailed: {})",
            detailed
        );

        if matches!(
            HealthChecker::primary_workspace_health(handler).await?,
            PrimaryWorkspaceHealth::ColdStart
        ) {
            let message =
                "No workspace initialized. Run manage_workspace(operation=\"index\") first.";
            return Ok(CallToolResult::text_content(vec![Content::text(message)]));
        }

        let snapshot = HealthChecker::system_snapshot(handler).await?;
        let report = snapshot.render_report(detailed);
        let structured = serde_json::to_value(&snapshot)?;

        let mut result = CallToolResult::text_content(vec![Content::text(report)]);
        result.structured_content = Some(structured);
        Ok(result)
    }
}
