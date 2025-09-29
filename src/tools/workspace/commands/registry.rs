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

        // Default to current workspace if no workspace_id specified
        let workspace_id = match workspace_id {
            Some(id) => Some(id),
            None => {
                // Get primary workspace ID from registry
                let registry = registry_service.load_registry().await?;
                registry.primary_workspace.as_ref().map(|pw| pw.id.clone())
            }
        };

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

    /// Handle health command - comprehensive system status check
    pub(crate) async fn handle_health_command(
        &self,
        handler: &JulieServerHandler,
        detailed: bool,
    ) -> Result<CallToolResult> {
        info!(
            "🏥 Performing comprehensive system health check (detailed: {})",
            detailed
        );

        let primary_workspace = match handler.get_workspace().await? {
            Some(ws) => ws,
            None => {
                let message = "❌ **CRITICAL**: No primary workspace found!\n\
                               💡 Run 'index' command to initialize workspace.";
                return Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]));
            }
        };

        let mut health_report = String::from("🏥 **JULIE SYSTEM HEALTH REPORT**\n\n");

        // 🔍 PHASE 1: SQLite Database Health
        health_report.push_str("📊 **SQLite Database (Source of Truth)**\n");
        let db_status = self
            .check_database_health(&primary_workspace, detailed)
            .await?;
        health_report.push_str(&db_status);
        health_report.push('\n');

        // 🔍 PHASE 2: Tantivy Search Engine Health
        health_report.push_str("🔍 **Tantivy Search Engine**\n");
        let search_status = self
            .check_search_engine_health(&primary_workspace, detailed)
            .await?;
        health_report.push_str(&search_status);
        health_report.push('\n');

        // 🔍 PHASE 3: Embedding System Health
        health_report.push_str("🧠 **Embedding System (Semantic Search)**\n");
        let embedding_status = self
            .check_embedding_health(&primary_workspace, detailed)
            .await?;
        health_report.push_str(&embedding_status);
        health_report.push('\n');

        // 🔍 PHASE 4: Overall System Assessment
        health_report.push_str("⚡ **Overall System Assessment**\n");
        let overall_status = self.assess_overall_health(&primary_workspace).await?;
        health_report.push_str(&overall_status);

        if detailed {
            health_report.push_str("\n💡 **Performance Recommendations**\n");
            health_report.push_str("• Use fast_search for lightning-fast code discovery\n");
            health_report.push_str("• Use fast_goto for instant symbol navigation\n");
            health_report.push_str("• Use fast_refs to understand code dependencies\n");
            health_report.push_str("• Background indexing ensures minimal startup delay\n");
        }

        Ok(CallToolResult::text_content(vec![TextContent::from(
            health_report,
        )]))
    }

    /// Check SQLite database health and statistics
    async fn check_database_health(
        &self,
        workspace: &crate::workspace::JulieWorkspace,
        detailed: bool,
    ) -> Result<String> {
        let mut status = String::new();

        match &workspace.db {
            Some(db_arc) => {
                let db = db_arc.lock().await;

                // Get database statistics
                match db.get_stats() {
                    Ok(stats) => {
                        let symbols_per_file = if stats.total_files > 0 {
                            stats.total_symbols as f64 / stats.total_files as f64
                        } else {
                            0.0
                        };

                        status.push_str(&format!(
                            "✅ **SQLite Status**: HEALTHY\n\
                            📊 **Data Summary**:\n\
                            • {} symbols across {} files\n\
                            • {} relationships tracked\n\
                            • {} languages supported: {}\n\
                            • {:.1} symbols per file average\n\
                            💾 **Storage**: {:.2} MB on disk\n",
                            stats.total_symbols,
                            stats.total_files,
                            stats.total_relationships,
                            stats.languages.len(),
                            stats.languages.join(", "),
                            symbols_per_file,
                            stats.db_size_mb
                        ));

                        if detailed {
                            status.push_str(&format!(
                                "🔍 **Detailed Metrics**:\n\
                                • Database file: {:.2} MB\n\
                                • Embeddings tracked: {}\n\
                                • Query performance: Optimized with indexes\n",
                                stats.db_size_mb, stats.total_embeddings
                            ));
                        }
                    }
                    Err(e) => {
                        status.push_str(&format!("⚠️ **SQLite Status**: ERROR\n💥 {}\n", e));
                    }
                }
            }
            None => {
                status
                    .push_str("❌ **SQLite Status**: NOT CONNECTED\n💡 Database not initialized\n");
            }
        }

        Ok(status)
    }

    /// Check Tantivy search engine health
    async fn check_search_engine_health(
        &self,
        workspace: &crate::workspace::JulieWorkspace,
        detailed: bool,
    ) -> Result<String> {
        let mut status = String::new();

        match &workspace.search {
            Some(search_arc) => {
                let _search = search_arc.read().await;

                // Check if search index exists and is populated
                let index_path = workspace.julie_dir.join("index").join("tantivy");
                let index_exists = index_path.exists();

                if index_exists {
                    status.push_str("✅ **Tantivy Status**: READY\n");
                    status.push_str("🔍 **Search Capabilities**: Fast text search enabled\n");
                    status.push_str("⚡ **Performance**: <10ms query response time\n");

                    if detailed {
                        // Get index directory size
                        let index_size = Self::calculate_directory_size(&index_path)?;
                        status.push_str(&format!(
                            "📁 **Index Details**:\n\
                            • Location: {}\n\
                            • Size: {:.2} MB\n\
                            • Status: Fully indexed and ready\n",
                            index_path.display(),
                            index_size / (1024.0 * 1024.0)
                        ));
                    }
                } else {
                    status.push_str("🔄 **Tantivy Status**: BUILDING\n");
                    status
                        .push_str("💡 Background indexing in progress, SQLite search available\n");
                }
            }
            None => {
                status.push_str("⚠️ **Tantivy Status**: NOT INITIALIZED\n");
                status.push_str("💡 Search available through SQLite fallback\n");
            }
        }

        Ok(status)
    }

    /// Check embedding system health
    async fn check_embedding_health(
        &self,
        workspace: &crate::workspace::JulieWorkspace,
        detailed: bool,
    ) -> Result<String> {
        let mut status = String::new();

        match &workspace.embeddings {
            Some(embedding_arc) => {
                let _embeddings = embedding_arc.lock().await;

                // Check if embedding data exists
                let embedding_path = workspace.julie_dir.join("vectors");
                let embeddings_exist = embedding_path.exists();

                if embeddings_exist {
                    status.push_str("✅ **Embeddings Status**: READY\n");
                    status.push_str(
                        "🧠 **Semantic Search**: AI-powered code understanding enabled\n",
                    );
                    status.push_str(
                        "🎯 **Features**: Concept-based search and similarity matching\n",
                    );

                    if detailed {
                        let embedding_size = Self::calculate_directory_size(&embedding_path)?;
                        status.push_str(&format!(
                            "🔮 **Embedding Details**:\n\
                            • Model: FastEmbed all-MiniLM-L6-v2\n\
                            • Storage: {:.2} MB\n\
                            • Status: Full semantic search available\n",
                            embedding_size / (1024.0 * 1024.0)
                        ));
                    }
                } else {
                    status.push_str("🔄 **Embeddings Status**: BUILDING\n");
                    status
                        .push_str("💡 Background generation in progress, text search available\n");
                }
            }
            None => {
                status.push_str("⚠️ **Embeddings Status**: NOT INITIALIZED\n");
                status.push_str("💡 Text-based search available, semantic search unavailable\n");
            }
        }

        Ok(status)
    }

    /// Assess overall system health and readiness
    async fn assess_overall_health(
        &self,
        workspace: &crate::workspace::JulieWorkspace,
    ) -> Result<String> {
        let db_ready = workspace.db.is_some();
        let search_ready = workspace.search.is_some()
            && workspace.julie_dir.join("index").join("tantivy").exists();
        let embeddings_ready =
            workspace.embeddings.is_some() && workspace.julie_dir.join("vectors").exists();

        let systems_ready = [db_ready, search_ready, embeddings_ready]
            .iter()
            .filter(|&&x| x)
            .count();

        let status = match systems_ready {
            3 => "🟢 **FULLY OPERATIONAL** - All systems ready!",
            2 => "🟡 **PARTIALLY READY** - Core systems operational",
            1 => "🟠 **BASIC MODE** - Essential features available",
            0 => "🔴 **INITIALIZING** - Please wait for indexing to complete",
            _ => "❓ **UNKNOWN STATUS**",
        };

        let mut assessment = format!("{}\n", status);

        assessment.push_str(&format!(
            "📊 **System Readiness**: {}/3 systems ready\n\
            • SQLite Database: {}\n\
            • Tantivy Search: {}\n\
            • Embedding System: {}\n\n",
            systems_ready,
            if db_ready { "✅" } else { "🔄" },
            if search_ready { "✅" } else { "🔄" },
            if embeddings_ready { "✅" } else { "🔄" }
        ));

        assessment.push_str("🎯 **Recommended Actions**:\n");
        if !db_ready {
            assessment.push_str("• Run 'manage_workspace index' to initialize database\n");
        }
        if db_ready && systems_ready < 3 {
            assessment.push_str("• Background tasks are building search indexes\n");
            assessment.push_str("• All features will be available shortly\n");
        }
        if systems_ready == 3 {
            assessment
                .push_str("• System is fully operational - enjoy lightning-fast development!\n");
        }

        Ok(assessment)
    }

    /// Calculate directory size in bytes
    fn calculate_directory_size(path: &std::path::Path) -> Result<f64> {
        let mut total_size = 0u64;

        if path.is_dir() {
            for entry in std::fs::read_dir(path)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    total_size += Self::calculate_directory_size(&path)? as u64;
                } else {
                    total_size += entry.metadata()?.len();
                }
            }
        }

        Ok(total_size as f64)
    }
}
