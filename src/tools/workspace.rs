use anyhow::Result;
use rust_mcp_sdk::macros::mcp_tool;
use rust_mcp_sdk::macros::JsonSchema;
use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, warn};

use super::shared::{BLACKLISTED_DIRECTORIES, BLACKLISTED_EXTENSIONS};
use crate::extractors::Symbol;
use crate::handler::JulieServerHandler;
use crate::workspace::registry::WorkspaceType;
use crate::workspace::registry_service::WorkspaceRegistryService;

//******************//
// Workspace Management Commands //
//******************//

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum WorkspaceCommand {
    /// Index primary workspace or current directory
    Index {
        /// Path to workspace (defaults to current directory)
        path: Option<String>,
        /// Force complete re-indexing even if cache exists
        force: bool,
    },
    /// Add reference workspace for cross-project search
    Add {
        /// Path to the workspace to add
        path: String,
        /// Optional display name for the workspace
        name: Option<String>,
    },
    /// Remove specific workspace by ID
    Remove {
        /// Workspace ID to remove
        workspace_id: String,
    },
    /// List all registered workspaces with status
    List,
    /// Clean up expired or orphaned workspaces
    Clean {
        /// Only clean expired workspaces, not orphaned ones
        expired_only: bool,
    },
    /// Re-index specific workspace
    Refresh {
        /// Workspace ID to refresh
        workspace_id: String,
    },
    /// Show workspace statistics
    Stats {
        /// Optional specific workspace ID (defaults to all)
        workspace_id: Option<String>,
    },
    /// Set TTL for reference workspaces
    SetTtl {
        /// Number of days before reference workspaces expire
        days: u32,
    },
    /// Set storage size limit
    SetLimit {
        /// Maximum total index size in MB
        max_size_mb: u64,
    },
}

#[mcp_tool(
    name = "manage_workspace",
    description = "🏗️ UNIFIED WORKSPACE MANAGEMENT - Index, add, remove, and manage multiple project workspaces\n\nCommon operations:\n• Index workspace: Use 'index' command to enable fast search capabilities\n• Force reindex: Use 'index' with force=true to rebuild from scratch\n• Multi-workspace: Use 'add' to include reference workspaces for cross-project search\n• Maintenance: Use 'clean' to remove expired workspaces and optimize storage\n\nMust provide command as JSON object with command type + parameters (see examples in parameter docs)",
    title = "Manage Julie Workspaces",
    idempotent_hint = false,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = false,
    meta = r#"{"priority": "high", "category": "workspace"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ManageWorkspaceTool {
    /// Workspace management command to execute.
    ///
    /// Examples:
    /// - Index current directory: {"command": "index", "force": false, "path": null}
    /// - Force reindex workspace: {"command": "index", "force": true, "path": null}
    /// - Index specific path: {"command": "index", "force": false, "path": "/path/to/workspace"}
    /// - Add reference workspace: {"command": "add", "path": "/path/to/other/project", "name": "Optional Display Name"}
    /// - List all workspaces: {"command": "list"}
    /// - Remove workspace: {"command": "remove", "workspace_id": "workspace-id-here"}
    /// - Clean expired workspaces: {"command": "clean", "expired_only": true}
    /// - Show statistics: {"command": "stats", "workspace_id": null}
    /// - Set TTL: {"command": "set_ttl", "days": 30}
    /// - Set storage limit: {"command": "set_limit", "max_size_mb": 1024}
    ///
    /// Note: The command field uses a tagged enum structure where the command type and parameters
    /// are combined in a single JSON object with the command type as the "command" field.
    pub command: WorkspaceCommand,
}

impl ManageWorkspaceTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        info!("🏗️ Managing workspace with command: {:?}", self.command);

        match &self.command {
            WorkspaceCommand::Index { path, force } => {
                self.handle_index_command(handler, path.clone(), *force)
                    .await
            }
            WorkspaceCommand::Add { path, name } => {
                self.handle_add_command(handler, path, name.clone()).await
            }
            WorkspaceCommand::Remove { workspace_id } => {
                self.handle_remove_command(handler, workspace_id).await
            }
            WorkspaceCommand::List => self.handle_list_command(handler).await,
            WorkspaceCommand::Clean { expired_only } => {
                self.handle_clean_command(handler, *expired_only).await
            }
            WorkspaceCommand::Refresh { workspace_id } => {
                self.handle_refresh_command(handler, workspace_id).await
            }
            WorkspaceCommand::Stats { workspace_id } => {
                self.handle_stats_command(handler, workspace_id.clone())
                    .await
            }
            WorkspaceCommand::SetTtl { days } => self.handle_set_ttl_command(handler, *days).await,
            WorkspaceCommand::SetLimit { max_size_mb } => {
                self.handle_set_limit_command(handler, *max_size_mb).await
            }
        }
    }

    /// Handle index command - index primary workspace
    async fn handle_index_command(
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

    /// Handle add command - add reference workspace
    async fn handle_add_command(
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
    async fn handle_remove_command(
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
    async fn handle_list_command(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
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
    async fn handle_clean_command(
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
    async fn handle_refresh_command(
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

                // TODO: Implement actual re-indexing logic (Phase 4)
                let message = format!(
                    "🔄 Workspace refresh queued: {}\n\
                    📁 Path: {}\n\
                    💡 Full re-indexing will be implemented in Phase 4",
                    workspace_entry.display_name, workspace_entry.original_path
                );
                Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]))
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
    async fn handle_stats_command(
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

    /// Handle set TTL command - configure expiration
    async fn handle_set_ttl_command(
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
    async fn handle_set_limit_command(
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

    // ============================================================
    // INDEXING METHODS (moved from IndexWorkspaceTool)
    // ============================================================

    /// Resolve workspace path with proper root detection
    fn resolve_workspace_path(&self, workspace_path: Option<String>) -> Result<PathBuf> {
        let target_path = match workspace_path {
            Some(path) => {
                let expanded_path = shellexpand::tilde(&path).to_string();
                PathBuf::from(expanded_path)
            }
            None => std::env::current_dir()?,
        };

        // Ensure path exists
        if !target_path.exists() {
            return Err(anyhow::anyhow!(
                "Path does not exist: {}",
                target_path.display()
            ));
        }

        // If it's a file, get its directory
        let workspace_candidate = if target_path.is_file() {
            target_path
                .parent()
                .ok_or_else(|| anyhow::anyhow!("Cannot determine parent directory"))?
                .to_path_buf()
        } else {
            target_path
        };

        // Find the actual workspace root
        self.find_workspace_root(&workspace_candidate)
    }

    /// Find workspace root by looking for common workspace markers
    fn find_workspace_root(&self, start_path: &Path) -> Result<PathBuf> {
        let workspace_markers = [
            ".git",
            ".julie",
            ".vscode",
            "Cargo.toml",
            "package.json",
            ".project",
        ];

        let mut current_path = start_path.to_path_buf();

        // Walk up the directory tree looking for workspace markers
        loop {
            for marker in &workspace_markers {
                let marker_path = current_path.join(marker);
                if marker_path.exists() {
                    info!(
                        "🎯 Found workspace marker '{}' at: {}",
                        marker,
                        current_path.display()
                    );
                    return Ok(current_path);
                }
            }

            match current_path.parent() {
                Some(parent) => current_path = parent.to_path_buf(),
                None => break,
            }
        }

        // No markers found, use the original path as workspace root
        info!(
            "🎯 No workspace markers found, using directory as root: {}",
            start_path.display()
        );
        Ok(start_path.to_path_buf())
    }

    async fn index_workspace_files(
        &self,
        handler: &JulieServerHandler,
        workspace_path: &Path,
        force_reindex: bool,
    ) -> Result<(usize, usize, usize)> {
        info!("🔍 Scanning workspace: {}", workspace_path.display());

        // Clear existing data if force reindex
        if force_reindex {
            handler.symbols.write().await.clear();
            handler.relationships.write().await.clear();
        }

        let mut total_files = 0;

        // Use blacklist-based file discovery
        let files_to_index = self.discover_indexable_files(workspace_path)?;

        info!(
            "📊 Found {} files to index after filtering",
            files_to_index.len()
        );

        for file_path in files_to_index {
            match self.process_file(handler, &file_path).await {
                Ok(_) => {
                    total_files += 1;
                    if total_files % 50 == 0 {
                        debug!("📈 Processed {} files so far...", total_files);
                    }
                }
                Err(e) => {
                    warn!("Failed to process file {:?}: {}", file_path, e);
                }
            }
        }

        // Get final counts
        let total_symbols = handler.symbols.read().await.len();
        let total_relationships = handler.relationships.read().await.len();

        // CRITICAL FIX: Feed symbols to SearchEngine for fast indexed search
        if total_symbols > 0 {
            info!(
                "⚡ Populating SearchEngine with {} symbols...",
                total_symbols
            );
            let symbols = handler.symbols.read().await;
            let symbol_vec: Vec<Symbol> = symbols.clone();
            drop(symbols); // Release the read lock

            let search_engine = handler.active_search_engine().await;
            let mut search_engine = search_engine.write().await;

            // Index all symbols in SearchEngine
            search_engine.index_symbols(symbol_vec).await.map_err(|e| {
                error!("Failed to populate SearchEngine: {}", e);
                anyhow::anyhow!("SearchEngine indexing failed: {}", e)
            })?;

            // Commit to make symbols searchable
            search_engine.commit().await.map_err(|e| {
                error!("Failed to commit SearchEngine: {}", e);
                anyhow::anyhow!("SearchEngine commit failed: {}", e)
            })?;

            info!("🚀 SearchEngine populated and committed - searches will now be fast!");
        }

        info!(
            "✅ Indexing complete: {} files, {} symbols, {} relationships",
            total_files, total_symbols, total_relationships
        );

        Ok((total_symbols, total_files, total_relationships))
    }

    /// Discover all indexable files using blacklist approach
    fn discover_indexable_files(&self, workspace_path: &Path) -> Result<Vec<PathBuf>> {
        let mut indexable_files = Vec::new();
        let blacklisted_dirs: HashSet<&str> = BLACKLISTED_DIRECTORIES.iter().copied().collect();
        let blacklisted_exts: HashSet<&str> = BLACKLISTED_EXTENSIONS.iter().copied().collect();
        let max_file_size = 1024 * 1024; // 1MB limit for files

        debug!(
            "🔍 Starting recursive file discovery from: {}",
            workspace_path.display()
        );

        self.walk_directory_recursive(
            workspace_path,
            &blacklisted_dirs,
            &blacklisted_exts,
            max_file_size,
            &mut indexable_files,
        )?;

        debug!("📊 File discovery summary:");
        debug!("  - Total indexable files: {}", indexable_files.len());

        Ok(indexable_files)
    }

    /// Recursively walk directory tree, excluding blacklisted paths
    fn walk_directory_recursive(
        &self,
        dir_path: &Path,
        blacklisted_dirs: &HashSet<&str>,
        blacklisted_exts: &HashSet<&str>,
        max_file_size: u64,
        indexable_files: &mut Vec<PathBuf>,
    ) -> Result<()> {
        let entries = fs::read_dir(dir_path)
            .map_err(|e| anyhow::anyhow!("Failed to read directory {:?}: {}", dir_path, e))?;

        for entry in entries {
            let entry =
                entry.map_err(|e| anyhow::anyhow!("Failed to read directory entry: {}", e))?;
            let path = entry.path();
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            // Skip hidden files/directories that start with . (except known code files)
            if file_name.starts_with('.') && !self.is_known_dotfile(&path) {
                continue;
            }

            if path.is_dir() {
                // Check if directory should be blacklisted
                if blacklisted_dirs.contains(file_name) {
                    debug!("⏭️  Skipping blacklisted directory: {}", path.display());
                    continue;
                }

                // Recursively process subdirectory
                self.walk_directory_recursive(
                    &path,
                    blacklisted_dirs,
                    blacklisted_exts,
                    max_file_size,
                    indexable_files,
                )?;
            } else if path.is_file() {
                // Check file extension and size
                if self.should_index_file(&path, blacklisted_exts, max_file_size)? {
                    indexable_files.push(path);
                }
            }
        }

        Ok(())
    }

    /// Check if a file should be indexed based on blacklist and size limits
    fn should_index_file(
        &self,
        file_path: &Path,
        blacklisted_exts: &HashSet<&str>,
        max_file_size: u64,
    ) -> Result<bool> {
        // Get file extension
        let extension = file_path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| format!(".{}", ext.to_lowercase()))
            .unwrap_or_default();

        // Skip blacklisted extensions
        if blacklisted_exts.contains(extension.as_str()) {
            return Ok(false);
        }

        // Check file size
        let metadata = fs::metadata(file_path)
            .map_err(|e| anyhow::anyhow!("Failed to get metadata for {:?}: {}", file_path, e))?;

        if metadata.len() > max_file_size {
            debug!(
                "⏭️  Skipping large file ({} bytes): {}",
                metadata.len(),
                file_path.display()
            );
            return Ok(false);
        }

        // If no extension, check if it's likely a text file by reading first few bytes
        if extension.is_empty() {
            return Ok(self.is_likely_text_file(file_path)?);
        }

        // Index any non-blacklisted file
        Ok(true)
    }

    /// Check if a dotfile is a known configuration file that should be indexed
    fn is_known_dotfile(&self, path: &Path) -> bool {
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        matches!(
            file_name,
            ".gitignore"
                | ".gitattributes"
                | ".editorconfig"
                | ".eslintrc"
                | ".prettierrc"
                | ".babelrc"
                | ".tsconfig"
                | ".jsconfig"
                | ".cargo"
                | ".env"
                | ".npmrc"
        )
    }

    /// Heuristic to determine if a file without extension is likely a text file
    fn is_likely_text_file(&self, file_path: &Path) -> Result<bool> {
        // Read first 512 bytes to check for binary content
        let mut file = fs::File::open(file_path)
            .map_err(|e| anyhow::anyhow!("Failed to open file {:?}: {}", file_path, e))?;

        let mut buffer = [0; 512];
        let bytes_read = std::io::Read::read(&mut file, &mut buffer)
            .map_err(|e| anyhow::anyhow!("Failed to read file {:?}: {}", file_path, e))?;

        if bytes_read == 0 {
            return Ok(false); // Empty file
        }

        // Check for null bytes (common in binary files)
        let has_null_bytes = buffer[..bytes_read].contains(&0);
        if has_null_bytes {
            return Ok(false);
        }

        // Check if most bytes are printable ASCII/UTF-8
        let printable_count = buffer[..bytes_read]
            .iter()
            .filter(|&&b| b >= 32 && b <= 126 || b == 9 || b == 10 || b == 13 || b >= 128)
            .count();

        let text_ratio = printable_count as f64 / bytes_read as f64;
        Ok(text_ratio > 0.8) // At least 80% printable characters
    }

    async fn process_file(&self, handler: &JulieServerHandler, file_path: &Path) -> Result<()> {
        debug!("Processing file: {:?}", file_path);

        // Read file content
        let content = fs::read_to_string(file_path)
            .map_err(|e| anyhow::anyhow!("Failed to read file {:?}: {}", file_path, e))?;

        // Skip empty files
        if content.trim().is_empty() {
            return Ok(());
        }

        // Determine language and extract symbols
        let language = self.detect_language(file_path);
        let file_path_str = file_path.to_string_lossy().to_string();

        self.extract_symbols_for_language(handler, &file_path_str, &content, &language)
            .await
    }

    /// Extract symbols using the appropriate extractor for the detected language
    async fn extract_symbols_for_language(
        &self,
        handler: &JulieServerHandler,
        file_path: &str,
        content: &str,
        language: &str,
    ) -> Result<()> {
        // Only process languages that we have both tree-sitter support and extractors for
        match language {
            "rust" | "typescript" | "javascript" | "python" => {
                self.extract_symbols_with_parser(handler, file_path, content, language)
                    .await
            }
            _ => {
                // For unsupported languages, just skip extraction but log it
                debug!(
                    "No extractor available for language: {} (file: {})",
                    language, file_path
                );
                Ok(())
            }
        }
    }

    /// Extract symbols using the appropriate extractor - specific implementation per language
    async fn extract_symbols_with_parser(
        &self,
        handler: &JulieServerHandler,
        file_path: &str,
        content: &str,
        language: &str,
    ) -> Result<()> {
        // Create parser for the language
        let mut parser = tree_sitter::Parser::new();
        let tree_sitter_language = self.get_tree_sitter_language(language)?;

        parser.set_language(&tree_sitter_language).map_err(|e| {
            anyhow::anyhow!("Failed to set parser language for {}: {}", language, e)
        })?;

        // Parse the file
        let tree = parser
            .parse(content, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse file: {}", file_path))?;

        // Extract symbols and relationships using language-specific extractor
        let (symbols, relationships) = match language {
            "rust" => {
                let mut extractor = crate::extractors::rust::RustExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                let symbols = extractor.extract_symbols(&tree);
                let relationships = extractor.extract_relationships(&tree, &symbols);
                (symbols, relationships)
            }
            "typescript" => {
                let mut extractor = crate::extractors::typescript::TypeScriptExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                let symbols = extractor.extract_symbols(&tree);
                let relationships = extractor.extract_relationships(&tree, &symbols);
                (symbols, relationships)
            }
            "javascript" => {
                let mut extractor = crate::extractors::javascript::JavaScriptExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                let symbols = extractor.extract_symbols(&tree);
                let relationships = extractor.extract_relationships(&tree, &symbols);
                (symbols, relationships)
            }
            "python" => {
                let mut extractor = crate::extractors::python::PythonExtractor::new(
                    file_path.to_string(),
                    content.to_string(),
                );
                let symbols = extractor.extract_symbols(&tree);
                let relationships = extractor.extract_relationships(&tree, &symbols);
                (symbols, relationships)
            }
            _ => {
                debug!(
                    "Language '{}' supported for parsing but no extractor available",
                    language
                );
                (Vec::new(), Vec::new())
            }
        };

        debug!(
            "📊 Extracted {} symbols and {} relationships from {}",
            symbols.len(),
            relationships.len(),
            file_path
        );

        // Store in persistent database and search index if workspace is available
        if let Some(workspace) = handler.get_workspace().await? {
            if let Some(db) = &workspace.db {
                let db_lock = db.lock().await;

                let workspace_id = "primary";

                // Calculate and store file hash for change detection
                let _file_hash = crate::database::calculate_file_hash(file_path)?;
                let file_info = crate::database::create_file_info(file_path, language)?;
                db_lock.store_file_info(&file_info, workspace_id)?;

                // Store symbols in database
                if let Err(e) = db_lock.store_symbols(&symbols, workspace_id) {
                    warn!("Failed to store symbols in database: {}", e);
                }

                // Store relationships in database
                if let Err(e) = db_lock.store_relationships(&relationships, workspace_id) {
                    warn!("Failed to store relationships in database: {}", e);
                }

                debug!(
                    "✅ Stored {} symbols and {} relationships in database",
                    symbols.len(),
                    relationships.len()
                );
            }

            // Also add symbols to search index for fast retrieval
            if let Some(search_index) = &workspace.search {
                let mut search_lock = search_index.write().await;
                if let Err(e) = search_lock.index_symbols(symbols.clone()).await {
                    warn!("Failed to index symbols in search engine: {}", e);
                } else {
                    debug!("✅ Indexed {} symbols in Tantivy search", symbols.len());
                }
            }
        }

        // Store results in handler (compatibility)
        {
            let mut symbol_storage = handler.symbols.write().await;
            symbol_storage.extend(symbols);
        }

        {
            let mut relationship_storage = handler.relationships.write().await;
            relationship_storage.extend(relationships);
        }

        Ok(())
    }

    /// Get the appropriate tree-sitter language for a detected language
    fn get_tree_sitter_language(&self, language: &str) -> Result<tree_sitter::Language> {
        match language {
            "rust" => Ok(tree_sitter_rust::LANGUAGE.into()),
            "typescript" => Ok(tree_sitter_typescript::LANGUAGE_TSX.into()),
            "javascript" => Ok(tree_sitter_javascript::LANGUAGE.into()),
            "python" => Ok(tree_sitter_python::LANGUAGE.into()),
            _ => Err(anyhow::anyhow!(
                "No tree-sitter language available for: {}",
                language
            )),
        }
    }

    /// Detect programming language from file extension
    fn detect_language(&self, file_path: &Path) -> String {
        let extension = file_path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");

        let file_name = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");

        // Match by extension first
        match extension.to_lowercase().as_str() {
            // Rust
            "rs" => "rust".to_string(),

            // TypeScript/JavaScript
            "ts" | "mts" | "cts" => "typescript".to_string(),
            "tsx" => "typescript".to_string(),
            "js" | "mjs" | "cjs" => "javascript".to_string(),
            "jsx" => "javascript".to_string(),

            // Python
            "py" | "pyi" | "pyw" => "python".to_string(),

            // Java
            "java" => "java".to_string(),

            // C#
            "cs" => "csharp".to_string(),

            // PHP
            "php" | "phtml" | "php3" | "php4" | "php5" => "php".to_string(),

            // Ruby
            "rb" | "rbw" => "ruby".to_string(),

            // Swift
            "swift" => "swift".to_string(),

            // Kotlin
            "kt" | "kts" => "kotlin".to_string(),

            // Go
            "go" => "go".to_string(),

            // C
            "c" => "c".to_string(),

            // C++
            "cpp" | "cc" | "cxx" | "c++" | "hpp" | "hh" | "hxx" | "h++" => "cpp".to_string(),
            "h" => {
                // Could be C or C++ header, default to C
                if file_path.to_string_lossy().contains("cpp")
                    || file_path.to_string_lossy().contains("c++")
                {
                    "cpp".to_string()
                } else {
                    "c".to_string()
                }
            }

            // Lua
            "lua" => "lua".to_string(),

            // SQL
            "sql" | "mysql" | "pgsql" | "sqlite" => "sql".to_string(),

            // HTML
            "html" | "htm" => "html".to_string(),

            // CSS
            "css" => "css".to_string(),

            // Vue
            "vue" => "vue".to_string(),

            // Razor
            "cshtml" | "razor" => "razor".to_string(),

            // Shell scripts
            "sh" | "bash" | "zsh" | "fish" => "bash".to_string(),

            // PowerShell
            "ps1" | "psm1" | "psd1" => "powershell".to_string(),

            // GDScript
            "gd" => "gdscript".to_string(),

            // Zig
            "zig" => "zig".to_string(),

            // Dart
            "dart" => "dart".to_string(),

            // Regex patterns (special handling)
            "regex" | "regexp" => "regex".to_string(),

            // Default case - check filename
            _ => {
                // Handle files without extensions or special cases
                match file_name.to_lowercase().as_str() {
                    // Build files
                    "dockerfile" | "containerfile" => "dockerfile".to_string(),
                    "makefile" | "gnumakefile" => "makefile".to_string(),
                    "cargo.toml" | "cargo.lock" => "toml".to_string(),
                    "package.json" | "tsconfig.json" | "jsconfig.json" => "json".to_string(),

                    // Shell scripts
                    name if name.starts_with("bash")
                        || name.contains("bashrc")
                        || name.contains("bash_") =>
                    {
                        "bash".to_string()
                    }

                    // Default to unknown
                    _ => "text".to_string(),
                }
            }
        }
    }
}
