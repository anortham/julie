use super::ManageWorkspaceTool;
use crate::dashboard::standalone::{DashboardLaunchOptions, launch_dashboard};
use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use anyhow::Result;
use tracing::info;

impl ManageWorkspaceTool {
    pub(crate) async fn handle_dashboard_command(&self) -> Result<CallToolResult> {
        info!("Launching standalone dashboard from manage_workspace");

        let launch = launch_dashboard(DashboardLaunchOptions::default()).await?;
        let mut message = format!(
            "Dashboard Launched\nURL: {}\n\nThe dashboard is running in this Julie session and will stop when the session exits.",
            launch.url
        );

        if let Some(error) = launch.browser_error {
            message.push_str(&format!("\n\nBrowser open failed: {error}"));
        }

        Ok(CallToolResult::text_content(vec![Content::text(message)]))
    }
}
