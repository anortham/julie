// Centralized Health Check System
//
// This module provides a unified health checking mechanism used by all tools
// to ensure consistent behavior across the entire Julie system.

use crate::handler::JulieServerHandler;
use anyhow::Result;
use std::sync::{Arc, Mutex};
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

pub(crate) enum PrimaryWorkspaceHealth {
    ColdStart,
    Ready {
        database: Option<Arc<Mutex<crate::database::SymbolDatabase>>>,
        search_index_ready: bool,
    },
}

impl HealthChecker {
    pub(crate) async fn primary_workspace_health(
        handler: &JulieServerHandler,
    ) -> Result<PrimaryWorkspaceHealth> {
        match handler.primary_workspace_snapshot().await {
            Ok(snapshot) => {
                let search_index_ready = handler
                    .get_search_index_for_workspace(&snapshot.binding.workspace_id)
                    .await?
                    .is_some();

                Ok(PrimaryWorkspaceHealth::Ready {
                    database: Some(snapshot.database),
                    search_index_ready,
                })
            }
            Err(err) => {
                let target_workspace_id = match handler.require_primary_workspace_identity() {
                    Ok(id) => id,
                    Err(identity_err) => {
                        if handler.is_primary_workspace_swap_in_progress() {
                            return Err(identity_err);
                        }

                        return Ok(PrimaryWorkspaceHealth::ColdStart);
                    }
                };

                if handler.is_primary_workspace_swap_in_progress() {
                    return Err(err);
                }

                let database = match handler
                    .get_database_for_workspace(&target_workspace_id)
                    .await
                {
                    Ok(db) => Some(db),
                    Err(_) => None,
                };

                let search_index_ready = handler
                    .get_search_index_for_workspace(&target_workspace_id)
                    .await?
                    .is_some();

                Ok(PrimaryWorkspaceHealth::Ready {
                    database,
                    search_index_ready,
                })
            }
        }
    }

    /// Get comprehensive system readiness status
    ///
    /// This is the SINGLE SOURCE OF TRUTH for system health across all tools
    pub async fn check_system_readiness(
        handler: &JulieServerHandler,
        workspace_id: Option<&str>,
    ) -> Result<SystemStatus> {
        if workspace_id.is_some_and(|id| id != "primary") {
            let target_workspace_id = workspace_id.unwrap().to_string();
            return Self::check_workspace_store_readiness(handler, &target_workspace_id).await;
        }

        match Self::primary_workspace_health(handler).await? {
            PrimaryWorkspaceHealth::ColdStart => Ok(SystemStatus::NotReady),
            PrimaryWorkspaceHealth::Ready {
                database,
                search_index_ready,
            } => {
                let Some(db) = database else {
                    debug!("❌ Health check failed: primary workspace database is unavailable");
                    return Ok(SystemStatus::NotReady);
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

                if search_index_ready {
                    Ok(SystemStatus::FullyReady { symbol_count })
                } else {
                    Ok(SystemStatus::SqliteOnly { symbol_count })
                }
            }
        }
    }

    async fn check_workspace_store_readiness(
        handler: &JulieServerHandler,
        workspace_id: &str,
    ) -> Result<SystemStatus> {
        let db = match handler.get_database_for_workspace(workspace_id).await {
            Ok(db) => db,
            Err(_) => return Ok(SystemStatus::NotReady),
        };

        let symbol_count = match db.try_lock() {
            Ok(db_lock) => db_lock.get_symbol_count_for_workspace().unwrap_or(0),
            Err(_busy) => {
                debug!("Symbol database busy during readiness check; assuming data present");
                1
            }
        };

        if symbol_count == 0 {
            return Ok(SystemStatus::NotReady);
        }

        let has_search_index = handler
            .get_search_index_for_workspace(workspace_id)
            .await?
            .is_some();

        if has_search_index {
            Ok(SystemStatus::FullyReady { symbol_count })
        } else {
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

    /// Get a user-friendly status message
    pub async fn get_status_message(handler: &JulieServerHandler) -> Result<String> {
        let readiness = Self::check_system_readiness(handler, None).await?;

        match readiness {
            SystemStatus::NotReady => {
                Ok("❌ System not ready. Run 'manage_workspace index' to initialize.".to_string())
            }
            SystemStatus::SqliteOnly { symbol_count } => Ok(format!(
                "🟡 Partially ready: {} symbols available in SQLite, Tantivy search unavailable",
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
        let (database, search_index_ready) = match Self::primary_workspace_health(handler).await? {
            PrimaryWorkspaceHealth::ColdStart => {
                return Ok("❌ No workspace found".to_string());
            }
            PrimaryWorkspaceHealth::Ready {
                database,
                search_index_ready,
            } => (database, search_index_ready),
        };

        let mut report = String::new();

        // Database status
        if let Some(db) = database.as_ref() {
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

        // Search status
        if search_index_ready {
            report.push_str("✅ Tantivy search ready\n");
        } else {
            report.push_str("❌ Tantivy search index not initialized\n");
        }

        Ok(report)
    }
}
