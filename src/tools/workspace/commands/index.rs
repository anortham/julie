use super::ManageWorkspaceTool;
use crate::handler::JulieServerHandler;
use crate::workspace::registry::WorkspaceType;
use crate::workspace::registry_service::WorkspaceRegistryService;
use anyhow::Result;
use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use tracing::{debug, error, info};

impl ManageWorkspaceTool {
    /// Handle index command - index primary workspace
    pub(crate) async fn handle_index_command(
        &self,
        handler: &JulieServerHandler,
        path: Option<String>,
        force: bool,
    ) -> Result<CallToolResult> {
        info!("📚 Starting workspace indexing...");

        let workspace_path = self.resolve_workspace_path(path)?;
        let force_reindex = force;

        info!("🎯 Resolved workspace path: {}", workspace_path.display());

        // Clear existing state if force reindexing
        if force_reindex {
            info!("🔄 Force reindex requested - clearing existing state");
            handler.symbols.write().await.clear();
            handler.relationships.write().await.clear();
            *handler.is_indexed.write().await = false;
        }

        // Initialize or load workspace in handler (with force if requested)
        handler
            .initialize_workspace_with_force(
                Some(workspace_path.to_string_lossy().to_string()),
                force_reindex,
            )
            .await?;

        // Check if already indexed and not forcing reindex
        if !force_reindex {
            let is_indexed = *handler.is_indexed.read().await;
            if is_indexed {
                let symbol_count = handler.symbols.read().await.len();
                let message = format!(
                    "✅ Workspace already indexed!\n\
                    📊 Found {} symbols\n\
                    💡 Use force: true to re-index",
                    symbol_count
                );
                return Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]));
            }
        }

        // Perform indexing
        match self
            .index_workspace_files(handler, &workspace_path, force_reindex)
            .await
        {
            Ok((symbol_count, file_count, relationship_count)) => {
                // Mark as indexed
                *handler.is_indexed.write().await = true;

                // Register as primary workspace
                if let Some(workspace) = handler.get_workspace().await? {
                    let registry_service = WorkspaceRegistryService::new(workspace.root.clone());
                    let workspace_path_str = workspace.root.to_string_lossy().to_string();

                    match registry_service
                        .register_workspace(workspace_path_str, WorkspaceType::Primary)
                        .await
                    {
                        Ok(entry) => {
                            info!("✅ Registered primary workspace: {}", entry.id);
                        }
                        Err(e) => {
                            debug!("Primary workspace registration: {}", e);
                        }
                    }
                }

                let message = format!(
                    "🎉 Workspace indexing complete!\n\
                    📁 Indexed {} files\n\
                    🔍 Extracted {} symbols\n\
                    🔗 Found {} relationships\n\
                    ⚡ Ready for search and navigation!",
                    file_count, symbol_count, relationship_count
                );
                Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]))
            }
            Err(e) => {
                error!("Failed to index workspace: {}", e);
                let message = format!(
                    "❌ Workspace indexing failed: {}\n\
                    💡 Check that the path exists and contains source files",
                    e
                );
                Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]))
            }
        }
    }
}
