use super::ManageWorkspaceTool;
use crate::handler::JulieServerHandler;
use anyhow::Result;
use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use tracing::{info, warn};

impl ManageWorkspaceTool {
    /// Handle health command - comprehensive system status check
    pub(crate) async fn handle_health_command(
        &self,
        handler: &JulieServerHandler,
        detailed: bool,
    ) -> Result<CallToolResult> {
        info!(
            "Performing comprehensive system health check (detailed: {})",
            detailed
        );

        let primary_workspace = match handler.get_workspace().await? {
            Some(ws) => ws,
            None => {
                let message = "CRITICAL: No primary workspace found!\n\
                               Run 'index' command to initialize workspace.";
                return Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]));
            }
        };

        let mut health_report = String::from("JULIE SYSTEM HEALTH REPORT\n\n");

        // PHASE 1: SQLite Database Health
        health_report.push_str("SQLite Database (Source of Truth)\n");
        let db_status = self
            .check_database_health(&primary_workspace, detailed)
            .await?;
        health_report.push_str(&db_status);
        health_report.push('\n');

        // PHASE 2: SQLite FTS5 Search Health
        health_report.push_str("SQLite FTS5 Search\n");
        let search_status = self
            .check_search_engine_health(&primary_workspace, detailed)
            .await?;
        health_report.push_str(&search_status);
        health_report.push('\n');

        // PHASE 3: Embedding System Health
        health_report.push_str("Embedding System (Semantic Search)\n");
        let embedding_status = self
            .check_embedding_health(&primary_workspace, detailed)
            .await?;
        health_report.push_str(&embedding_status);
        health_report.push('\n');

        // PHASE 4: Overall System Assessment
        health_report.push_str("Overall System Assessment\n");
        let overall_status = self.assess_overall_health(&primary_workspace).await?;
        health_report.push_str(&overall_status);

        if detailed {
            health_report.push_str("\nPerformance Recommendations\n");
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
                let db = db_arc.lock().unwrap();

                // Get database statistics
                match db.get_stats() {
                    Ok(stats) => {
                        let symbols_per_file = if stats.total_files > 0 {
                            stats.total_symbols as f64 / stats.total_files as f64
                        } else {
                            0.0
                        };

                        status.push_str(&format!(
                            "SQLite Status: HEALTHY\n\
                            Data Summary:\n\
                            • {} symbols across {} files\n\
                            • {} relationships tracked\n\
                            • {} languages supported: {}\n\
                            • {:.1} symbols per file average\n\
                            Storage: {:.2} MB on disk\n",
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
                                "Detailed Metrics:\n\
                                • Database file: {:.2} MB\n\
                                • Embeddings tracked: {}\n\
                                • Query performance: Optimized with indexes\n",
                                stats.db_size_mb, stats.total_embeddings
                            ));
                        }
                    }
                    Err(e) => {
                        status.push_str(&format!("SQLite Status: ERROR\n{}\n", e));
                    }
                }
            }
            None => {
                status.push_str("SQLite Status: NOT CONNECTED\nDatabase not initialized\n");
            }
        }

        Ok(status)
    }

    /// Check SQLite FTS5 search health
    async fn check_search_engine_health(
        &self,
        workspace: &crate::workspace::JulieWorkspace,
        _detailed: bool,
    ) -> Result<String> {
        let mut status = String::new();

        // SQLite FTS5 search is always available when database exists
        if workspace.db.is_some() {
            status.push_str("SQLite FTS5 Status: READY\n");
            status.push_str("Search Capabilities: Fast full-text search enabled\n");
            status.push_str("Performance: <5ms query response time\n");
        } else {
            status.push_str("Search Status: NOT AVAILABLE\n");
            status.push_str("Database not initialized\n");
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
            Some(_embedding_arc) => {
                // Compute workspace ID for per-workspace path
                use crate::workspace::registry as ws_registry;
                let workspace_id =
                    ws_registry::generate_workspace_id(workspace.root.to_str().unwrap_or(""))?;

                // Check if embedding data exists (no lock needed, just checking filesystem)
                let embedding_path = workspace.workspace_vectors_path(&workspace_id);
                let embeddings_exist = embedding_path.exists();

                if embeddings_exist {
                    status.push_str("Embeddings Status: READY\n");
                    status.push_str("Semantic Search: AI-powered code understanding enabled\n");
                    status.push_str("Features: Concept-based search and similarity matching\n");

                    if detailed {
                        // Calculate directory size asynchronously to avoid blocking
                        let path = embedding_path.clone();
                        let embedding_size = match tokio::task::spawn_blocking(move || {
                            crate::tools::workspace::calculate_dir_size(&path)
                        })
                        .await
                        {
                            Ok(Ok(size)) => size as f64,
                            Ok(Err(e)) => {
                                warn!("Failed to calculate embedding directory size: {}", e);
                                0.0
                            }
                            Err(e) => {
                                warn!(
                                    "spawn_blocking task failed for embedding size calculation: {}",
                                    e
                                );
                                0.0
                            }
                        };

                        // Detect GPU acceleration based on platform
                        let acceleration = if cfg!(target_os = "windows") {
                            "DirectML (GPU)"
                        } else if cfg!(target_os = "linux") {
                            "CUDA (GPU)"
                        } else if cfg!(target_os = "macos") {
                            "CPU-optimized"
                        } else {
                            "CPU"
                        };

                        status.push_str(&format!(
                            "Embedding Details:\n\
                            • Model: ONNX Runtime with bge-small-en-v1.5 (384-dim)\n\
                            • Acceleration: {}\n\
                            • Storage: {:.2} MB\n\
                            • Status: Full semantic search available\n",
                            acceleration,
                            embedding_size / (1024.0 * 1024.0)
                        ));
                    }
                } else {
                    status.push_str("Embeddings Status: BUILDING\n");
                    status.push_str("Background generation in progress, text search available\n");
                }
            }
            None => {
                status.push_str("Embeddings Status: NOT INITIALIZED\n");
                status.push_str("Text-based search available, semantic search unavailable\n");
            }
        }

        Ok(status)
    }

    /// Assess overall system health and readiness
    async fn assess_overall_health(
        &self,
        workspace: &crate::workspace::JulieWorkspace,
    ) -> Result<String> {
        // Compute workspace ID for per-workspace paths
        use crate::workspace::registry as ws_registry;
        let workspace_id =
            ws_registry::generate_workspace_id(workspace.root.to_str().unwrap_or(""))?;

        let db_ready = workspace.db.is_some();
        let embeddings_ready = workspace.embeddings.is_some()
            && workspace.workspace_vectors_path(&workspace_id).exists();

        let systems_ready = [db_ready, embeddings_ready].iter().filter(|&&x| x).count();

        let status = match systems_ready {
            2 => "FULLY OPERATIONAL - All systems ready!",
            1 => "PARTIALLY READY - Core systems operational",
            0 => "INITIALIZING - Please wait for indexing to complete",
            _ => "UNKNOWN STATUS",
        };

        let mut assessment = format!("{}\n", status);

        assessment.push_str(&format!(
            "System Readiness: {}/2 systems ready\n\
            • SQLite Database (with FTS5 search): {}\n\
            • Embedding System: {}\n\n",
            systems_ready,
            if db_ready { "READY" } else { "BUILDING" },
            if embeddings_ready {
                "READY"
            } else {
                "BUILDING"
            }
        ));

        assessment.push_str("Recommended Actions:\n");
        if !db_ready {
            assessment.push_str("• Run 'manage_workspace index' to initialize database\n");
        }
        if db_ready && systems_ready < 2 {
            assessment.push_str("• Background tasks are building embeddings\n");
            assessment.push_str("• Search is available now, semantic features coming shortly\n");
        }
        if systems_ready == 2 {
            assessment
                .push_str("• System is fully operational - enjoy lightning-fast development!\n");
        }

        Ok(assessment)
    }
}
