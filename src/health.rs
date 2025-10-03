// Centralized Health Check System
//
// This module provides a unified health checking mechanism used by all tools
// to ensure consistent behavior across the entire Julie system.

use crate::handler::JulieServerHandler;
use anyhow::Result;
use tracing::debug;

/// System readiness levels for graceful degradation
#[derive(Debug, Clone)]
pub enum SystemReadiness {
    /// No workspace or database available
    NotReady,
    /// Only SQLite database is ready (fallback mode)
    SqliteOnly { symbol_count: i64 },
    /// Some systems ready (partial functionality)
    PartiallyReady {
        tantivy_ready: bool,
        embeddings_ready: bool,
        symbol_count: i64,
    },
    /// All systems operational
    FullyReady { symbol_count: i64 },
}

/// Centralized health checker used by all tools
pub struct HealthChecker;

impl HealthChecker {
    /// Get comprehensive system readiness status
    ///
    /// This is the SINGLE SOURCE OF TRUTH for system health across all tools
    pub async fn check_system_readiness(handler: &JulieServerHandler) -> Result<SystemReadiness> {
        // Step 1: Check if workspace and database exist
        let workspace = match handler.get_workspace().await? {
            Some(ws) => ws,
            None => return Ok(SystemReadiness::NotReady),
        };

        let db = match &workspace.db {
            Some(db_arc) => db_arc,
            None => return Ok(SystemReadiness::NotReady),
        };

        // Step 2: Get the actual primary workspace ID from registry
        let registry_service = crate::workspace::registry_service::WorkspaceRegistryService::new(
            workspace.root.clone(),
        );
        let primary_workspace_id = match registry_service.get_primary_workspace_id().await? {
            Some(id) => id,
            None => return Ok(SystemReadiness::NotReady),
        };

        // Step 3: Get symbol count from database using actual workspace ID
        let symbol_count = match db.try_lock() {
            Ok(db_lock) => db_lock
                .get_symbol_count_for_workspace(&primary_workspace_id)
                .unwrap_or(0),
            Err(_busy) => {
                debug!(
                    "Symbol database busy during readiness check; assuming symbols available"
                );
                1
            }
        };

        if symbol_count == 0 {
            return Ok(SystemReadiness::NotReady);
        }

        // Compute workspace ID for per-workspace paths
        use crate::workspace::registry;
        let workspace_id = registry::generate_workspace_id(
            workspace.root.to_str()
                .ok_or_else(|| anyhow::anyhow!("Invalid workspace path"))?
        )?;

        // Step 4: Check Tantivy search engine status
        let tantivy_ready = workspace.search.is_some()
            && workspace.workspace_index_path(&workspace_id).exists();

        // Step 5: Check embedding system status
        let embeddings_ready = workspace.embeddings.is_some()
            && Self::has_embedding_files(&workspace.workspace_vectors_path(&workspace_id));

        // Step 6: Determine overall readiness level
        match (tantivy_ready, embeddings_ready) {
            (false, false) => Ok(SystemReadiness::SqliteOnly { symbol_count }),
            (true, true) => Ok(SystemReadiness::FullyReady { symbol_count }),
            _ => Ok(SystemReadiness::PartiallyReady {
                tantivy_ready,
                embeddings_ready,
                symbol_count,
            }),
        }
    }

    /// Quick check: Is the system ready for basic operations?
    pub async fn is_ready_for_search(handler: &JulieServerHandler) -> Result<bool> {
        match Self::check_system_readiness(handler).await? {
            SystemReadiness::NotReady => Ok(false),
            _ => Ok(true), // SQLite or better is sufficient for search
        }
    }

    /// Quick check: Is Tantivy search available for optimal performance?
    pub async fn is_tantivy_ready(handler: &JulieServerHandler) -> Result<bool> {
        match Self::check_system_readiness(handler).await? {
            SystemReadiness::PartiallyReady { tantivy_ready, .. } => Ok(tantivy_ready),
            SystemReadiness::FullyReady { .. } => Ok(true),
            _ => Ok(false),
        }
    }

    /// Quick check: Are embeddings available for semantic search?
    pub async fn are_embeddings_ready(handler: &JulieServerHandler) -> Result<bool> {
        match Self::check_system_readiness(handler).await? {
            SystemReadiness::PartiallyReady {
                embeddings_ready, ..
            } => Ok(embeddings_ready),
            SystemReadiness::FullyReady { .. } => Ok(true),
            _ => Ok(false),
        }
    }

    /// Get a user-friendly status message
    pub async fn get_status_message(handler: &JulieServerHandler) -> Result<String> {
        let readiness = Self::check_system_readiness(handler).await?;

        match readiness {
            SystemReadiness::NotReady => {
                Ok("❌ System not ready. Run 'manage_workspace index' to initialize.".to_string())
            }
            SystemReadiness::SqliteOnly { symbol_count } => Ok(format!(
                "🔄 Basic mode: {} symbols available via database search (Tantivy building)",
                symbol_count
            )),
            SystemReadiness::PartiallyReady {
                tantivy_ready,
                embeddings_ready,
                symbol_count,
            } => {
                let tantivy_status = if tantivy_ready { "✅" } else { "🔄" };
                let embedding_status = if embeddings_ready { "✅" } else { "🔄" };
                Ok(format!(
                    "🟡 Partially ready: {} symbols | Tantivy: {} | Embeddings: {}",
                    symbol_count, tantivy_status, embedding_status
                ))
            }
            SystemReadiness::FullyReady { symbol_count } => Ok(format!(
                "🟢 Fully operational: {} symbols with lightning-fast search!",
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
            let db_lock = db.lock().await;
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
        let workspace_id_result = registry::generate_workspace_id(
            workspace.root.to_str().unwrap_or("")
        );

        // Tantivy status
        if let Ok(workspace_id) = &workspace_id_result {
            let tantivy_path = workspace.workspace_index_path(workspace_id);
            if tantivy_path.exists() {
                report.push_str("✅ Tantivy search index ready\n");
            } else {
                report.push_str("🔄 Tantivy search index building\n");
            }
        } else {
            report.push_str("❌ Could not determine workspace ID\n");
        }

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
