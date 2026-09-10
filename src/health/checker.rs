use crate::handler::JulieServerHandler;
use crate::handler::session_workspace::PrimaryWorkspaceBinding;
use crate::search::projection::TANTIVY_PROJECTION_NAME;
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
                let Ok(binding) = handler.require_primary_workspace_binding() else {
                    return Ok(PrimaryWorkspaceHealth::ColdStart);
                };
                tracing::debug!(
                    "primary snapshot unavailable, reporting binding-only health: {err}"
                );

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
        let overall =
            overall_from_planes(control_plane.level, data_plane.level, runtime_plane.level);

        let qualification = match &primary {
            PrimaryWorkspaceHealth::Ready(state) => {
                let mut record_opt =
                    crate::request_engine::semantic_qualification::load_workspace_qualification(
                        &state.binding.workspace_root,
                    );

                if let Some(ref mut record) = record_opt {
                    let pooled_db = handler
                        .get_pooled_database_for_workspace(&state.binding.workspace_id)
                        .await
                        .ok();

                    let cur_rev = pooled_db
                        .as_ref()
                        .and_then(|db| {
                            db.get_current_canonical_revision(&state.binding.workspace_id)
                                .ok()
                                .flatten()
                        })
                        .unwrap_or(-1);
                    let ready_encoder = handler
                        .checkout_store_for_workspace(
                            &state.binding.workspace_id,
                            &state.binding.workspace_root,
                        )
                        .await
                        .ok()
                        .and_then(|store| {
                            let current = store.current();
                            let vectors = current.vectors();
                            (!vectors.is_empty())
                                .then(|| vectors.encoder().map(|e| e.id.clone()))
                                .flatten()
                        });

                    let active_provider = handler.embedding_provider().await;
                    let active_id = active_provider
                        .as_ref()
                        .and_then(|p| p.encoder_identity().ok());
                    let active_device = active_provider.as_ref().map(|p| p.device_info().device);
                    let active_accel = active_provider.as_ref().and_then(|p| p.accelerated());
                    let running_sha = active_provider
                        .as_ref()
                        .and_then(|p| p.running_executable_sha());

                    let active_facts =
                        crate::request_engine::semantic_qualification::ActiveProviderFacts {
                            backend: &runtime_plane.embeddings.backend,
                            encoder_identity: active_id.as_ref(),
                            device: active_device.as_deref(),
                            accelerated: active_accel,
                            running_executable_sha: running_sha.as_deref(),
                        };

                    match crate::request_engine::semantic_qualification::validate_qualification_against_running_runtime(
                        record,
                        cur_rev,
                        ready_encoder.as_deref(),
                        &active_facts,
                    ) {
                        Ok(()) => {
                            record.qualified = true;
                            record.disqualification_reason = None;
                        }
                        Err(mismatch) => {
                            record.qualified = false;
                            record.disqualification_reason = Some(mismatch.to_string());
                        }
                    }
                }

                record_opt
            }
            _ => None,
        };

        Ok(SystemHealthSnapshot {
            overall,
            readiness,
            control_plane,
            data_plane,
            runtime_plane,
            qualification,
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
        let Ok(root) = handler.get_workspace_root_for_target(workspace_id).await else {
            return Ok(SystemStatus::NotReady);
        };
        let Ok(store) = handler
            .checkout_store_for_workspace(workspace_id, &root)
            .await
        else {
            return Ok(SystemStatus::NotReady);
        };
        let status = store.status();
        let symbol_count = status.graph.symbols as i64;
        if symbol_count == 0 {
            return Ok(SystemStatus::NotReady);
        }
        if status.tantivy == julie_index::checkout_store::TantivyState::Present {
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
        let tantivy = snapshot.data_plane.projection(TANTIVY_PROJECTION_NAME);
        let degraded_projection = snapshot.data_plane.projections.iter().find(|projection| {
            projection.name != TANTIVY_PROJECTION_NAME && projection.level != HealthLevel::Ready
        });

        match snapshot.readiness {
            SystemStatus::NotReady => {
                Ok("❌ System not ready. Run 'manage_workspace index' to initialize.".to_string())
            }
            SystemStatus::SqliteOnly { symbol_count } => match tantivy {
                Some(projection) if projection.freshness == ProjectionFreshness::Lagging => {
                    Ok(format!(
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
                    ))
                }
                Some(projection)
                    if projection.freshness == ProjectionFreshness::RebuildRequired =>
                {
                    Ok(format!(
                        "🟡 Partially ready: {} symbols in SQLite, Tantivy projection repair required: {}",
                        symbol_count, projection.detail
                    ))
                }
                _ => Ok(format!(
                    "🟡 Partially ready: {} symbols in SQLite, Tantivy projection missing",
                    symbol_count
                )),
            },
            SystemStatus::FullyReady { symbol_count } if degraded_projection.is_some() => {
                let projection = degraded_projection.expect("guarded projection");
                Ok(format!(
                    "🟡 Search-ready with degraded projection {}: {} symbols with Tantivy search; {}",
                    projection.name, symbol_count, projection.detail
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
        let embeddings = project_embedding_runtime(runtime_status, embedding_provider.as_deref());

        Ok(RuntimePlaneHealth {
            level: embeddings.level,
            embeddings,
        })
    }
}
