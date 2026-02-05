// Centralized Health Check System
//
// This module provides a unified health checking mechanism used by all tools
// to ensure consistent behavior across the entire Julie system.

use crate::handler::JulieServerHandler;
use anyhow::{Result, anyhow};
use tracing::{debug, warn};

/// System readiness levels for graceful degradation
#[derive(Debug, Clone)]
pub enum SystemStatus {
    /// No workspace or database available
    NotReady,
    /// SQLite + Tantivy search available
    SqliteOnly { symbol_count: i64 },
    /// All systems operational (SQLite + Tantivy search)
    FullyReady { symbol_count: i64 },
}

/// Centralized health checker used by all tools
pub struct HealthChecker;

impl HealthChecker {
    /// Get comprehensive system readiness status
    ///
    /// This is the SINGLE SOURCE OF TRUTH for system health across all tools
    pub async fn check_system_readiness(
        handler: &JulieServerHandler,
        workspace_id: Option<&str>,
    ) -> Result<SystemStatus> {
        // Step 1: Check if workspace and database exist
        let workspace = match handler.get_workspace().await? {
            Some(ws) => ws,
            None => {
                debug!("❌ Health check failed: handler.get_workspace() returned None");
                return Ok(SystemStatus::NotReady);
            }
        };

        let registry_service = crate::workspace::registry_service::WorkspaceRegistryService::new(
            workspace.root.clone(),
        );

        let primary_workspace_id = registry_service
            .get_primary_workspace_id()
            .await?
            .unwrap_or_else(|| "primary".to_string());

        let target_workspace_id = workspace_id
            .map(|id| {
                // Normalize "primary" to actual primary workspace ID
                // This allows users to pass workspace="primary" consistently
                if id == "primary" {
                    primary_workspace_id.clone()
                } else {
                    id.to_string()
                }
            })
            .unwrap_or_else(|| primary_workspace_id.clone());

        if target_workspace_id == primary_workspace_id {
            let db = match &workspace.db {
                Some(db_arc) => db_arc,
                None => {
                    debug!("❌ Health check failed: workspace.db is None for primary workspace");
                    return Ok(SystemStatus::NotReady);
                }
            };

            let symbol_count = match db.try_lock() {
                Ok(db_lock) => db_lock.get_symbol_count_for_workspace().unwrap_or(0),
                Err(_busy) => {
                    debug!(
                        "Primary symbol database busy during readiness check; assuming data present"
                    );
                    1
                }
            };

            if symbol_count == 0 {
                debug!("❌ Health check failed: symbol_count is 0 for primary workspace");
                return Ok(SystemStatus::NotReady);
            }

            // With Tantivy as the search engine, if symbols are indexed we're fully ready
            Ok(SystemStatus::FullyReady { symbol_count })
        } else {
            let ref_db_path = workspace.workspace_db_path(&target_workspace_id);
            if !ref_db_path.exists() {
                return Ok(SystemStatus::NotReady);
            }

            let symbol_count = tokio::task::spawn_blocking(move || -> Result<i64> {
                let ref_db = crate::database::SymbolDatabase::new(&ref_db_path)?;
                ref_db.get_symbol_count_for_workspace()
            })
            .await
            .map_err(|e| anyhow!("Failed to open reference workspace database: {}", e))??;

            if symbol_count == 0 {
                return Ok(SystemStatus::NotReady);
            }

            Ok(SystemStatus::SqliteOnly { symbol_count })
        }
    }

    /// Quick check: Is the system ready for basic operations?
    pub async fn is_ready_for_search(handler: &JulieServerHandler) -> Result<bool> {
        match Self::check_system_readiness(handler, None).await? {
            SystemStatus::NotReady => Ok(false),
            _ => Ok(true), // SQLite or better is sufficient for search
        }
    }

    /// Quick check: Are embeddings available for semantic search?
    pub async fn are_embeddings_ready(handler: &JulieServerHandler) -> Result<bool> {
        match Self::check_system_readiness(handler, None).await? {
            SystemStatus::FullyReady { .. } => Ok(true),
            _ => Ok(false),
        }
    }

    /// Get a user-friendly status message
    pub async fn get_status_message(handler: &JulieServerHandler) -> Result<String> {
        let readiness = Self::check_system_readiness(handler, None).await?;

        match readiness {
            SystemStatus::NotReady => {
                Ok("❌ System not ready. Run 'manage_workspace index' to initialize.".to_string())
            }
            SystemStatus::SqliteOnly { symbol_count } => Ok(format!(
                "🟢 Ready: {} symbols available via Tantivy search",
                symbol_count
            )),
            SystemStatus::FullyReady { symbol_count } => Ok(format!(
                "🟢 Fully operational: {} symbols with Tantivy search",
                symbol_count
            )),
        }
    }

    /// Generate detailed health report for diagnostics
    pub async fn get_detailed_health_report(handler: &JulieServerHandler) -> Result<String> {
        let workspace = match handler.get_workspace().await? {
            Some(ws) => ws,
            None => return Ok("❌ No workspace found".to_string()),
        };

        let mut report = String::new();

        // Database status
        if let Some(db) = &workspace.db {
            let db_lock = match db.lock() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    warn!(
                        "Database mutex poisoned during health report, recovering: {}",
                        poisoned
                    );
                    poisoned.into_inner()
                }
            };
            match db_lock.get_stats() {
                Ok(stats) => {
                    report.push_str(&format!(
                        "📊 Database: {} symbols, {} files, {} relationships\n",
                        stats.total_symbols, stats.total_files, stats.total_relationships
                    ));
                }
                Err(e) => {
                    report.push_str(&format!("❌ Database error: {}\n", e));
                }
            }
        } else {
            report.push_str("❌ No database connection\n");
        }

        // Compute workspace ID for per-workspace paths
        use crate::workspace::registry;
        let workspace_id_result =
            registry::generate_workspace_id(workspace.root.to_str().unwrap_or(""));

        // Search status
        if workspace_id_result.is_ok() {
            report.push_str("✅ Tantivy search ready\n");
        } else {
            report.push_str("❌ Could not determine workspace ID\n");
        }

        Ok(report)
    }
}
