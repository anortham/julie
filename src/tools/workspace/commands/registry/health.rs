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

        let report = HealthChecker::system_snapshot(handler)
            .await?
            .render_report(detailed);

        Ok(CallToolResult::text_content(vec![Content::text(report)]))
    }
}
