use crate::handler::JulieServerHandler;
use crate::handler::session_workspace::PrimaryWorkspaceBinding;
use anyhow::Result;

use super::evaluation::{overall_from_planes, readiness_from_data_plane};
use super::{
    ControlPlaneHealth, DaemonLifecycleState, HealthLevel, ProjectionFreshness, RuntimePlaneHealth,
    SystemHealthSnapshot, SystemStatus, WatcherState, build_data_plane, project_embedding_runtime,
};

/// Centralized health checker used by all tools.
pub struct HealthChecker;

#[derive(Clone)]
pub(crate) struct PrimaryWorkspaceState {
    pub binding: PrimaryWorkspaceBinding,
    pub search_index_ready: bool,
    pub indexing_runtime: Option<crate::tools::workspace::indexing::state::IndexingRuntimeSnapshot>,
}

pub(crate) enum PrimaryWorkspaceHealth {
    ColdStart,
    Ready(PrimaryWorkspaceState),
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

                Ok(PrimaryWorkspaceHealth::Ready(PrimaryWorkspaceState {
                    binding: snapshot.binding,
                    search_index_ready,
                    indexing_runtime: snapshot.indexing_runtime.as_ref().map(|runtime| {
                        runtime
                            .read()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .snapshot()
                    }),
                }))
            }
            Err(err) => {
                let binding = match handler.require_primary_workspace_binding() {
                    Ok(binding) => binding,
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

                let search_index_ready = handler
                    .get_search_index_for_workspace(&binding.workspace_id)
                    .await?
                    .is_some();

                Ok(PrimaryWorkspaceHealth::Ready(PrimaryWorkspaceState {
                    binding,
                    search_index_ready,
                    indexing_runtime: None,
                }))
            }
        }
    }

    pub async fn system_snapshot(handler: &JulieServerHandler) -> Result<SystemHealthSnapshot> {
        let primary = Self::primary_workspace_health(handler).await?;
        let control_plane = Self::build_control_plane(handler, &primary).await?;
        let data_plane = build_data_plane(handler, &primary).await?;
        let runtime_plane = Self::build_runtime_plane(handler).await?;
        let readiness = readiness_from_data_plane(&data_plane);
        let overall = overall_from_planes(
            control_plane.level,
            data_plane.level,
            runtime_plane.level,
            handler.embedding_service.is_some(),
        );

        Ok(SystemHealthSnapshot {
            overall,
            readiness,
            control_plane,
            data_plane,
            runtime_plane,
        })
    }

    /// Get comprehensive system readiness status.
    ///
    /// This is the single source of truth for search gating across tools.
    pub async fn check_system_readiness(
        handler: &JulieServerHandler,
        workspace_id: Option<&str>,
    ) -> Result<SystemStatus> {
        if workspace_id.is_some_and(|id| id != "primary") {
            let target_workspace_id = workspace_id.unwrap().to_string();
            return Self::check_workspace_store_readiness(handler, &target_workspace_id).await;
        }

        Ok(Self::system_snapshot(handler).await?.readiness)
    }

    async fn check_workspace_store_readiness(
        handler: &JulieServerHandler,
        workspace_id: &str,
    ) -> Result<SystemStatus> {
        // Pooled DB: read-only, no mutation gate required. The pool waits async
        // when no connection is immediately available, so no busy-fallback is
        // needed (the old Arc<Mutex<>> path treated contention as "data present"
        // — that heuristic doesn't translate to pool semantics).
        let pooled_db = match handler
            .get_pooled_database_for_workspace(workspace_id)
            .await
        {
            Ok(db) => db,
            Err(_) => return Ok(SystemStatus::NotReady),
        };

        let symbol_count = pooled_db.get_symbol_count_for_workspace().unwrap_or(0);

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

    /// Quick check: Is the system ready for basic search operations?
    pub async fn is_ready_for_search(handler: &JulieServerHandler) -> Result<bool> {
        match Self::check_system_readiness(handler, None).await? {
            SystemStatus::NotReady => Ok(false),
            _ => Ok(true),
        }
    }

    /// Get a user-facing status message.
    pub async fn get_status_message(handler: &JulieServerHandler) -> Result<String> {
        let snapshot = Self::system_snapshot(handler).await?;
        let projection = &snapshot.data_plane.search_projection;

        match snapshot.readiness {
            SystemStatus::NotReady => {
                Ok("❌ System not ready. Run 'manage_workspace index' to initialize.".to_string())
            }
            SystemStatus::SqliteOnly { symbol_count } => match projection.freshness {
                ProjectionFreshness::Lagging => Ok(format!(
                    "🟡 Partially ready: {} symbols in SQLite, Tantivy projection lagging at revision {}/{}",
                    symbol_count,
                    projection
                        .projected_revision
                        .map(|revision| revision.to_string())
                        .unwrap_or_else(|| "unknown".to_string()),
                    projection
                        .canonical_revision
                        .map(|revision| revision.to_string())
                        .unwrap_or_else(|| "unknown".to_string()),
                )),
                ProjectionFreshness::RebuildRequired => Ok(format!(
                    "🟡 Partially ready: {} symbols in SQLite, Tantivy projection repair required: {}",
                    symbol_count, projection.detail
                )),
                _ => Ok(format!(
                    "🟡 Partially ready: {} symbols in SQLite, Tantivy projection missing",
                    symbol_count
                )),
            },
            SystemStatus::FullyReady { symbol_count }
                if projection.freshness == ProjectionFreshness::Lagging =>
            {
                Ok(format!(
                    "🟡 Search-ready but lagging: {} symbols, Tantivy at revision {}/{}",
                    symbol_count,
                    projection
                        .projected_revision
                        .map(|revision| revision.to_string())
                        .unwrap_or_else(|| "unknown".to_string()),
                    projection
                        .canonical_revision
                        .map(|revision| revision.to_string())
                        .unwrap_or_else(|| "unknown".to_string()),
                ))
            }
            SystemStatus::FullyReady { symbol_count }
                if projection.freshness == ProjectionFreshness::RebuildRequired =>
            {
                Ok(format!(
                    "🟡 Search-ready but projection repair needed: {} symbols, {}",
                    symbol_count, projection.detail
                ))
            }
            SystemStatus::FullyReady { symbol_count } => {
                if snapshot.overall == HealthLevel::Ready {
                    Ok(format!(
                        "🟢 Fully operational: {} symbols with Tantivy search",
                        symbol_count
                    ))
                } else {
                    Ok(format!(
                        "🟡 Search-ready with degraded runtime: {} symbols with Tantivy search",
                        symbol_count
                    ))
                }
            }
        }
    }

    /// Generate detailed health report for diagnostics.
    pub async fn get_detailed_health_report(handler: &JulieServerHandler) -> Result<String> {
        match Self::primary_workspace_health(handler).await? {
            PrimaryWorkspaceHealth::ColdStart => Ok("❌ No workspace found".to_string()),
            PrimaryWorkspaceHealth::Ready(_) => {
                Ok(Self::system_snapshot(handler).await?.render_report(true))
            }
        }
    }

    async fn build_control_plane(
        handler: &JulieServerHandler,
        primary: &PrimaryWorkspaceHealth,
    ) -> Result<ControlPlaneHealth> {
        let daemon_state = if handler.daemon_db.is_some() {
            DaemonLifecycleState::Serving
        } else {
            DaemonLifecycleState::Direct
        };

        let primary_workspace_id = match primary {
            PrimaryWorkspaceHealth::ColdStart => None,
            PrimaryWorkspaceHealth::Ready(state) => Some(state.binding.workspace_id.clone()),
        };

        let (watcher_state, watcher_ref_count, watcher_grace_active) =
            if primary_workspace_id.is_some() {
                let workspace_guard = handler.workspace.read().await;
                let watcher_state = workspace_guard
                    .as_ref()
                    .and_then(|workspace| workspace.watcher.as_ref())
                    .map(|_| WatcherState::Local)
                    .unwrap_or(WatcherState::Unavailable);
                (watcher_state, None, false)
            } else {
                (WatcherState::Unavailable, None, false)
            };

        let level = if matches!(watcher_state, WatcherState::Unavailable)
            && primary_workspace_id.is_some()
        {
            HealthLevel::Degraded
        } else {
            HealthLevel::Ready
        };

        let detail = match daemon_state {
            DaemonLifecycleState::Direct => "Direct stdio session".to_string(),
            DaemonLifecycleState::Serving => "Shared daemon serving workspace sessions".to_string(),
        };

        Ok(ControlPlaneHealth {
            level,
            daemon_state,
            primary_workspace_id,
            watcher_state,
            watcher_ref_count,
            watcher_grace_active,
            detail,
        })
    }

    async fn build_runtime_plane(handler: &JulieServerHandler) -> Result<RuntimePlaneHealth> {
        let runtime_status = handler.embedding_runtime_status().await;
        let embedding_provider = handler.embedding_provider().await;
        let service_configured = handler.embedding_service.is_some();
        let service_settling = handler
            .embedding_service
            .as_ref()
            .is_some_and(|service| !service.is_settled());
        let embeddings = project_embedding_runtime(
            runtime_status,
            embedding_provider.as_deref(),
            service_configured,
            service_settling,
        );

        Ok(RuntimePlaneHealth {
            level: embeddings.level,
            embeddings,
        })
    }
}
