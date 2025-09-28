use super::ManageWorkspaceTool;
use crate::handler::JulieServerHandler;
use crate::workspace::registry::WorkspaceType;
use crate::workspace::registry_service::WorkspaceRegistryService;
use anyhow::Result;
use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use tracing::{info, warn};

impl ManageWorkspaceTool {
    /// Handle add command - add reference workspace
    pub(crate) async fn handle_add_command(
        &self,
        handler: &JulieServerHandler,
        path: &str,
        name: Option<String>,
    ) -> Result<CallToolResult> {
        info!("➕ Adding reference workspace: {}", path);

        // Get primary workspace for registry service
        let primary_workspace = match handler.get_workspace().await? {
            Some(ws) => ws,
            None => {
                let message = "❌ No primary workspace found. Please run 'index' command first.";
                return Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]));
            }
        };

        let registry_service = WorkspaceRegistryService::new(primary_workspace.root.clone());

        // Register the reference workspace
        match registry_service
            .register_workspace(path.to_string(), WorkspaceType::Reference)
            .await
        {
            Ok(entry) => {
                let display_name = name.unwrap_or_else(|| entry.display_name.clone());

                // TODO: Index the reference workspace (Phase 4)
                // For now, just register it in the registry

                let message = format!(
                    "✅ Added reference workspace!\n\
                    📝 ID: {}\n\
                    📁 Path: {}\n\
                    🏷️ Name: {}\n\
                    ⏰ Expires: {} days\n\
                    💡 Use 'refresh {}' to index its content",
                    entry.id,
                    entry.original_path,
                    display_name,
                    entry
                        .expires_at
                        .map(|exp| {
                            let days = (exp - entry.created_at) / (24 * 60 * 60);
                            format!("{}", days)
                        })
                        .unwrap_or("never".to_string()),
                    entry.id
                );
                Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]))
            }
            Err(e) => {
                let message = format!("❌ Failed to add workspace: {}", e);
                Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]))
            }
        }
    }

    /// Handle remove command - remove workspace by ID
    pub(crate) async fn handle_remove_command(
        &self,
        handler: &JulieServerHandler,
        workspace_id: &str,
    ) -> Result<CallToolResult> {
        info!("🗑️ Removing workspace: {}", workspace_id);

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

        // First check if workspace exists and clean up database data
        if let Ok(Some(_workspace_entry)) = registry_service.get_workspace(workspace_id).await {
            // Clean up database data before removing from registry
            if let Some(db) = &primary_workspace.db {
                let db_lock = db.lock().await;
                match db_lock.delete_workspace_data(workspace_id) {
                    Ok(stats) => {
                        info!("Cleaned database data for workspace {}: {} symbols, {} files, {} relationships",
                              workspace_id, stats.symbols_deleted, stats.files_deleted, stats.relationships_deleted);
                    }
                    Err(e) => {
                        warn!(
                            "Failed to clean database data for workspace {}: {}",
                            workspace_id, e
                        );
                    }
                }
            }

            // Remove from registry
            match registry_service.unregister_workspace(workspace_id).await {
                Ok(true) => {
                    let message = format!(
                        "✅ **Workspace Removed Successfully**\n\
                        🗑️ Workspace: {}\n\
                        📊 Database data cleaned up\n\
                        💡 All associated symbols, files, and relationships have been removed.",
                        workspace_id
                    );
                    Ok(CallToolResult::text_content(vec![TextContent::from(
                        message,
                    )]))
                }
                Ok(false) => {
                    let message = format!("⚠️ Workspace not found in registry: {}", workspace_id);
                    Ok(CallToolResult::text_content(vec![TextContent::from(
                        message,
                    )]))
                }
                Err(e) => {
                    let message = format!("❌ Failed to remove workspace from registry: {}", e);
                    Ok(CallToolResult::text_content(vec![TextContent::from(
                        message,
                    )]))
                }
            }
        } else {
            let message = format!("⚠️ Workspace not found: {}", workspace_id);
            Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]))
        }
    }

    /// Handle list command - show all workspaces
    pub(crate) async fn handle_list_command(
        &self,
        handler: &JulieServerHandler,
    ) -> Result<CallToolResult> {
        info!("📋 Listing all workspaces");

        let primary_workspace = match handler.get_workspace().await? {
            Some(ws) => ws,
            None => {
                let message = "❌ No primary workspace found. Use 'index' command to create one.";
                return Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]));
            }
        };

        let registry_service = WorkspaceRegistryService::new(primary_workspace.root.clone());

        match registry_service.get_all_workspaces().await {
            Ok(workspaces) => {
                if workspaces.is_empty() {
                    let message = "📭 No workspaces registered.";
                    return Ok(CallToolResult::text_content(vec![TextContent::from(
                        message,
                    )]));
                }

                let mut output = String::from("📋 Registered Workspaces:\n\n");

                for workspace in workspaces {
                    let status = if workspace.is_expired() {
                        "⏰ EXPIRED"
                    } else if !workspace.path_exists() {
                        "❌ MISSING"
                    } else {
                        "✅ ACTIVE"
                    };

                    let expires = match workspace.expires_at {
                        Some(exp_time) => {
                            let now = crate::workspace::registry::current_timestamp();
                            if exp_time > now {
                                let days_left = (exp_time - now) / (24 * 60 * 60);
                                format!("in {} days", days_left)
                            } else {
                                "expired".to_string()
                            }
                        }
                        None => "never".to_string(),
                    };

                    output.push_str(&format!(
                        "🏷️ **{}** ({})\n\
                        📁 Path: {}\n\
                        🔍 Type: {:?}\n\
                        📊 Documents: {} | Size: {:.1} KB\n\
                        ⏰ Expires: {}\n\
                        📅 Status: {}\n\n",
                        workspace.display_name,
                        workspace.id,
                        workspace.original_path,
                        workspace.workspace_type,
                        workspace.document_count,
                        workspace.index_size_bytes as f64 / 1024.0,
                        expires,
                        status
                    ));
                }

                Ok(CallToolResult::text_content(vec![TextContent::from(
                    output,
                )]))
            }
            Err(e) => {
                let message = format!("❌ Failed to list workspaces: {}", e);
                Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]))
            }
        }
    }

    /// Handle clean command - clean expired/orphaned workspaces
    pub(crate) async fn handle_clean_command(
        &self,
        handler: &JulieServerHandler,
        expired_only: bool,
    ) -> Result<CallToolResult> {
        info!("🧹 Cleaning workspaces (expired_only: {})", expired_only);

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

        if expired_only {
            // Only clean expired workspaces with full database cleanup
            match registry_service
                .cleanup_expired_workspaces_with_data(primary_workspace.db.as_ref())
                .await
            {
                Ok(report) => {
                    let message = if report.workspaces_removed.is_empty() {
                        "✨ No expired workspaces to clean.".to_string()
                    } else {
                        format!(
                            "✅ Cleaned {} expired workspace(s):\n{}\n\n\
                            📊 Database cleanup:\n\
                            • {} symbols deleted\n\
                            • {} files deleted\n\
                            • {} relationships deleted",
                            report.workspaces_removed.len(),
                            report
                                .workspaces_removed
                                .iter()
                                .map(|id| format!("  - {}", id))
                                .collect::<Vec<_>>()
                                .join("\n"),
                            report.total_symbols_deleted,
                            report.total_files_deleted,
                            report.total_relationships_deleted
                        )
                    };
                    Ok(CallToolResult::text_content(vec![TextContent::from(
                        message,
                    )]))
                }
                Err(e) => {
                    let message = format!("❌ Failed to clean expired workspaces: {}", e);
                    Ok(CallToolResult::text_content(vec![TextContent::from(
                        message,
                    )]))
                }
            }
        } else {
            // Comprehensive cleanup: TTL + Size Limits + Orphans
            match registry_service
                .comprehensive_cleanup(primary_workspace.db.as_ref())
                .await
            {
                Ok(report) => {
                    let ttl_count = report.ttl_cleanup.workspaces_removed.len();
                    let size_count = report.size_cleanup.workspaces_removed.len();
                    let orphan_count = report.orphaned_cleaned.len();
                    let total_symbols = report.ttl_cleanup.total_symbols_deleted
                        + report.size_cleanup.total_symbols_deleted;
                    let total_files = report.ttl_cleanup.total_files_deleted
                        + report.size_cleanup.total_files_deleted;

                    let mut message_parts = Vec::new();

                    if ttl_count > 0 {
                        message_parts
                            .push(format!("⏰ TTL Cleanup: {} expired workspaces", ttl_count));
                    }

                    if size_count > 0 {
                        message_parts.push(format!(
                            "💾 Size Cleanup: {} workspaces (LRU eviction)",
                            size_count
                        ));
                    }

                    if orphan_count > 0 {
                        message_parts.push(format!(
                            "🗑️ Orphan Cleanup: {} abandoned indexes",
                            orphan_count
                        ));
                    }

                    let message = if message_parts.is_empty() {
                        "✨ No cleanup needed. All workspaces are healthy!".to_string()
                    } else {
                        format!(
                            "🧹 **Comprehensive Cleanup Complete**\n\n{}\n\n\
                            📊 **Database Impact:**\n\
                            • {} symbols deleted\n\
                            • {} files deleted\n\
                            • {} relationships deleted\n\n\
                            💡 Cleanup helps maintain optimal performance and storage usage.",
                            message_parts.join("\n"),
                            total_symbols,
                            total_files,
                            report.ttl_cleanup.total_relationships_deleted
                                + report.size_cleanup.total_relationships_deleted
                        )
                    };

                    Ok(CallToolResult::text_content(vec![TextContent::from(
                        message,
                    )]))
                }
                Err(e) => {
                    let message = format!("❌ Failed to perform comprehensive cleanup: {}", e);
                    Ok(CallToolResult::text_content(vec![TextContent::from(
                        message,
                    )]))
                }
            }
        }
    }

    /// Handle refresh command - re-index workspace
    pub(crate) async fn handle_refresh_command(
        &self,
        handler: &JulieServerHandler,
        workspace_id: &str,
    ) -> Result<CallToolResult> {
        info!("🔄 Refreshing workspace: {}", workspace_id);

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

        // Get workspace info
        match registry_service.get_workspace(workspace_id).await? {
            Some(workspace_entry) => {
                // Update last accessed time
                registry_service.update_last_accessed(workspace_id).await?;

                // Actually re-index the workspace content
                let workspace_path = std::path::PathBuf::from(&workspace_entry.original_path);

                info!(
                    "🔄 Starting re-indexing of workspace: {}",
                    workspace_entry.display_name
                );

                match self
                    .index_workspace_files(handler, &workspace_path, true)
                    .await
                {
                    Ok((symbol_count, file_count, relationship_count)) => {
                        let message = format!(
                            "✅ **Workspace Refresh Complete!**\n\
                            🏷️ Workspace: {}\n\
                            📁 Path: {}\n\
                            📊 Results:\n\
                            • {} files indexed\n\
                            • {} symbols extracted\n\
                            • {} relationships found\n\
                            ⚡ Content is now up-to-date and searchable!",
                            workspace_entry.display_name,
                            workspace_entry.original_path,
                            file_count,
                            symbol_count,
                            relationship_count
                        );
                        Ok(CallToolResult::text_content(vec![TextContent::from(
                            message,
                        )]))
                    }
                    Err(e) => {
                        let message = format!(
                            "❌ **Workspace Refresh Failed**\n\
                            🏷️ Workspace: {}\n\
                            📁 Path: {}\n\
                            💥 Error: {}\n\
                            💡 Check that the path exists and contains readable files",
                            workspace_entry.display_name, workspace_entry.original_path, e
                        );
                        Ok(CallToolResult::text_content(vec![TextContent::from(
                            message,
                        )]))
                    }
                }
            }
            None => {
                let message = format!("❌ Workspace not found: {}", workspace_id);
                Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]))
            }
        }
    }

    /// Handle stats command - show workspace statistics
    pub(crate) async fn handle_stats_command(
        &self,
        handler: &JulieServerHandler,
        workspace_id: Option<String>,
    ) -> Result<CallToolResult> {
        info!("📊 Showing workspace statistics");

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

        match workspace_id {
            Some(id) => {
                // Show stats for specific workspace
                match registry_service.get_workspace(&id).await? {
                    Some(workspace) => {
                        let message = format!(
                            "📊 Workspace Statistics: {}\n\n\
                            🏷️ **{}** ({})\n\
                            📁 Path: {}\n\
                            🔍 Type: {:?}\n\
                            📊 Documents: {}\n\
                            💾 Index Size: {:.2} MB\n\
                            📅 Created: {} (timestamp)\n\
                            🕐 Last Accessed: {} (timestamp)\n\
                            ⏰ Expires: {}",
                            workspace.display_name,
                            workspace.display_name,
                            workspace.id,
                            workspace.original_path,
                            workspace.workspace_type,
                            workspace.document_count,
                            workspace.index_size_bytes as f64 / (1024.0 * 1024.0),
                            workspace.created_at,
                            workspace.last_accessed,
                            workspace
                                .expires_at
                                .map(|t| t.to_string())
                                .unwrap_or("never".to_string())
                        );
                        Ok(CallToolResult::text_content(vec![TextContent::from(
                            message,
                        )]))
                    }
                    None => {
                        let message = format!("❌ Workspace not found: {}", id);
                        Ok(CallToolResult::text_content(vec![TextContent::from(
                            message,
                        )]))
                    }
                }
            }
            None => {
                // Show overall statistics
                let registry = registry_service.load_registry().await?;

                let message = format!(
                    "📊 Overall Workspace Statistics\n\n\
                    🏗️ **Registry Status**\n\
                    📦 Total Workspaces: {}\n\
                    👑 Primary Workspace: {}\n\
                    📚 Reference Workspaces: {}\n\
                    🗑️ Orphaned Indexes: {}\n\n\
                    💾 **Storage Usage**\n\
                    📊 Total Documents: {}\n\
                    💽 Total Index Size: {:.2} MB\n\
                    📅 Last Updated: {} (timestamp)\n\n\
                    ⚙️ **Configuration**\n\
                    ⏰ Default TTL: {} days\n\
                    📏 Max Size Limit: {} MB\n\
                    🧹 Auto Cleanup: {}",
                    registry.statistics.total_workspaces,
                    if registry.primary_workspace.is_some() {
                        "Yes"
                    } else {
                        "No"
                    },
                    registry.reference_workspaces.len(),
                    registry.statistics.total_orphans,
                    registry.statistics.total_documents,
                    registry.statistics.total_index_size_bytes as f64 / (1024.0 * 1024.0),
                    registry.last_updated,
                    registry.config.default_ttl_seconds / (24 * 60 * 60), // Convert to days
                    registry.config.max_total_size_bytes / (1024 * 1024), // Convert to MB
                    if registry.config.auto_cleanup_enabled {
                        "Enabled"
                    } else {
                        "Disabled"
                    }
                );
                Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]))
            }
        }
    }
}
