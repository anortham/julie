// Centralized Health Check System
//
// This module provides a unified health checking mechanism used by all tools
// to ensure consistent behavior across the entire Julie system.

use crate::handler::JulieServerHandler;
use anyhow::{anyhow, Result};
use tracing::debug;

/// System readiness levels for graceful degradation
#[derive(Debug, Clone)]
pub enum SystemReadiness {
    /// No workspace or database available
    NotReady,
    /// Only SQLite database is ready (FTS5 search available)
    SqliteOnly { symbol_count: i64 },
    /// All systems operational (SQLite FTS5 + Embeddings)
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
    ) -> Result<SystemReadiness> {
        // Step 1: Check if workspace and database exist
        let workspace = match handler.get_workspace().await? {
            Some(ws) => ws,
            None => return Ok(SystemReadiness::NotReady),
        };

        let registry_service = crate::workspace::registry_service::WorkspaceRegistryService::new(
            workspace.root.clone(),
        );

        let primary_workspace_id = registry_service
            .get_primary_workspace_id()
            .await?
            .unwrap_or_else(|| "primary".to_string());

        let target_workspace_id = workspace_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| primary_workspace_id.clone());

        if target_workspace_id == primary_workspace_id {
            let db = match &workspace.db {
                Some(db_arc) => db_arc,
                None => return Ok(SystemReadiness::NotReady),
            };

            let symbol_count = match db.try_lock() {
                Ok(db_lock) => db_lock
                    .get_symbol_count_for_workspace(&target_workspace_id)
                    .unwrap_or(0),
                Err(_busy) => {
                    debug!(
                        "Primary symbol database busy during readiness check; assuming data present"
                    );
                    1
                }
            };

            if symbol_count == 0 {
                return Ok(SystemReadiness::NotReady);
            }

            let embeddings_ready = workspace.embeddings.is_some()
                && Self::has_embedding_files(
                    &workspace.workspace_vectors_path(&target_workspace_id),
                );

            if embeddings_ready {
                Ok(SystemReadiness::FullyReady { symbol_count })
            } else {
                Ok(SystemReadiness::SqliteOnly { symbol_count })
            }
        } else {
            let ref_db_path = workspace.workspace_db_path(&target_workspace_id);
            if !ref_db_path.exists() {
                return Ok(SystemReadiness::NotReady);
            }

            let symbol_count = tokio::task::spawn_blocking(move || -> Result<i64> {
                let ref_db = crate::database::SymbolDatabase::new(&ref_db_path)?;
                ref_db.get_symbol_count_for_workspace(&target_workspace_id)
            })
            .await
            .map_err(|e| anyhow!("Failed to open reference workspace database: {}", e))??;

            if symbol_count == 0 {
                return Ok(SystemReadiness::NotReady);
            }

            Ok(SystemReadiness::SqliteOnly { symbol_count })
        }
    }

    /// Quick check: Is the system ready for basic operations?
    pub async fn is_ready_for_search(handler: &JulieServerHandler) -> Result<bool> {
        match Self::check_system_readiness(handler, None).await? {
            SystemReadiness::NotReady => Ok(false),
            _ => Ok(true), // SQLite or better is sufficient for search
        }
    }

    /// Quick check: Are embeddings available for semantic search?
    pub async fn are_embeddings_ready(handler: &JulieServerHandler) -> Result<bool> {
        match Self::check_system_readiness(handler, None).await? {
            SystemReadiness::FullyReady { .. } => Ok(true),
            _ => Ok(false),
        }
    }

    /// Get a user-friendly status message
    pub async fn get_status_message(handler: &JulieServerHandler) -> Result<String> {
        let readiness = Self::check_system_readiness(handler, None).await?;

        match readiness {
            SystemReadiness::NotReady => {
                Ok("❌ System not ready. Run 'manage_workspace index' to initialize.".to_string())
            }
            SystemReadiness::SqliteOnly { symbol_count } => Ok(format!(
                "🟢 Ready: {} symbols available via SQLite FTS5 search (<5ms)",
                symbol_count
            )),
            SystemReadiness::FullyReady { symbol_count } => Ok(format!(
                "🟢 Fully operational: {} symbols with semantic search enabled!",
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
            let db_lock = db.lock().unwrap();
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

        // SQLite FTS5 search status (always available if database exists)
        report.push_str("✅ SQLite FTS5 search ready (<5ms queries)\n");

        // Embeddings status
        if let Ok(workspace_id) = &workspace_id_result {
            let embeddings_path = workspace.workspace_vectors_path(workspace_id);
            if Self::has_embedding_files(&embeddings_path) {
                report.push_str("✅ Embedding vectors ready\n");
            } else {
                report.push_str("🔄 Embedding vectors building\n");
            }
        } else {
            report.push_str("❌ Could not determine workspace ID\n");
        }

        Ok(report)
    }

    /// Check if a directory contains actual embedding files (not just empty directory)
    fn has_embedding_files(path: &std::path::Path) -> bool {
        if !path.exists() || !path.is_dir() {
            return false;
        }

        // Check if directory has any files (not just subdirectories)
        match std::fs::read_dir(path) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    if entry.path().is_file() {
                        return true; // Found at least one file
                    }
                }
                false // Directory exists but no files
            }
            Err(_) => false, // Can't read directory
        }
    }
}
