use super::ManageWorkspaceTool;
use crate::handler::JulieServerHandler;
use crate::workspace::registry_service::WorkspaceRegistryService;
use anyhow::Result;
use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use tracing::info;

impl ManageWorkspaceTool {
    /// Handle set TTL command - configure expiration
    pub(crate) async fn handle_set_ttl_command(
        &self,
        handler: &JulieServerHandler,
        days: u32,
    ) -> Result<CallToolResult> {
        info!("⏰ Setting TTL to {} days", days);

        let primary_workspace = match handler.get_workspace().await? {
            Some(ws) => ws,
            None => {
                let message = "❌ No primary workspace found.";
                return Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]));
            }
        };

        let registry_service = WorkspaceRegistryService::new(primary_workspace.root.clone());
        let mut registry = registry_service.load_registry().await?;

        // Update TTL configuration
        registry.config.default_ttl_seconds = days as u64 * 24 * 60 * 60; // Convert days to seconds

        registry_service.save_registry(registry).await?;

        let message = format!(
            "✅ TTL updated to {} days\n\
            💡 This affects new reference workspaces only.\n\
            🔄 Existing workspaces keep their current expiration dates.",
            days
        );
        Ok(CallToolResult::text_content(vec![TextContent::from(
            message,
        )]))
    }

    /// Handle set limit command - configure storage limits
    pub(crate) async fn handle_set_limit_command(
        &self,
        handler: &JulieServerHandler,
        max_size_mb: u64,
    ) -> Result<CallToolResult> {
        info!("💾 Setting storage limit to {} MB", max_size_mb);

        let primary_workspace = match handler.get_workspace().await? {
            Some(ws) => ws,
            None => {
                let message = "❌ No primary workspace found.";
                return Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]));
            }
        };

        let registry_service = WorkspaceRegistryService::new(primary_workspace.root.clone());
        let mut registry = registry_service.load_registry().await?;

        // Update size limit configuration
        registry.config.max_total_size_bytes = max_size_mb * 1024 * 1024; // Convert MB to bytes

        // Capture current usage before moving registry
        let current_usage_mb =
            registry.statistics.total_index_size_bytes as f64 / (1024.0 * 1024.0);

        registry_service.save_registry(registry).await?;

        let message = format!(
            "✅ Storage limit updated to {} MB\n\
            💡 Current usage: {:.2} MB\n\
            🧹 Auto-cleanup will enforce this limit.",
            max_size_mb, current_usage_mb
        );
        Ok(CallToolResult::text_content(vec![TextContent::from(
            message,
        )]))
    }
}
