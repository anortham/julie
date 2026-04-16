use crate::handler::JulieServerHandler;
use crate::handler::session_workspace::PrimaryWorkspaceBinding;
use crate::{
    database::ProjectionStatus as DbProjectionStatus, search::projection::TANTIVY_PROJECTION_NAME,
};
use anyhow::Result;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use tracing::debug;

use super::evaluation::{overall_from_levels, overall_from_planes, readiness_from_data_plane};
use super::{
    CanonicalStoreHealth, ControlPlaneHealth, DaemonLifecycleState, DataPlaneHealth, HealthLevel,
    IndexingHealth, ProjectionFreshness, ProjectionState, RuntimePlaneHealth,
    SearchProjectionHealth, SystemHealthSnapshot, SystemStatus, WatcherState,
    project_embedding_runtime,
};

/// Centralized health checker used by all tools.
pub struct HealthChecker;

#[derive(Clone)]
pub(crate) struct PrimaryWorkspaceState {
    pub binding: PrimaryWorkspaceBinding,
    pub database: Option<Arc<Mutex<crate::database::SymbolDatabase>>>,
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
                    database: Some(snapshot.database),
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

                let database = match handler
                    .get_database_for_workspace(&binding.workspace_id)
                    .await
                {
                    Ok(db) => Some(db),
                    Err(_) => None,
                };

                let search_index_ready = handler
                    .get_search_index_for_workspace(&binding.workspace_id)
                    .await?
                    .is_some();

                Ok(PrimaryWorkspaceHealth::Ready(PrimaryWorkspaceState {
                    binding,
                    database,
                    search_index_ready,
                    indexing_runtime: None,
                }))
            }
        }
    }

    pub async fn system_snapshot(handler: &JulieServerHandler) -> Result<SystemHealthSnapshot> {
        let primary = Self::primary_workspace_health(handler).await?;
        let control_plane = Self::build_control_plane(handler, &primary).await?;
        let data_plane = Self::build_data_plane(&primary)?;
        let runtime_plane = Self::build_runtime_plane(handler).await?;
        let readiness = readiness_from_data_plane(&data_plane);
        let overall =
            overall_from_planes(control_plane.level, data_plane.level, runtime_plane.level);

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
            SystemStatus::SqliteOnly { symbol_count } => Ok(format!(
                "🟡 Partially ready: {} symbols in SQLite, Tantivy projection missing",
                symbol_count
            )),
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
        let daemon_state = match handler.restart_pending.as_ref() {
            Some(flag) if flag.load(Ordering::Relaxed) => DaemonLifecycleState::RestartPending,
            Some(_) => DaemonLifecycleState::Serving,
            None => DaemonLifecycleState::Direct,
        };

        let primary_workspace_id = match primary {
            PrimaryWorkspaceHealth::ColdStart => None,
            PrimaryWorkspaceHealth::Ready(state) => Some(state.binding.workspace_id.clone()),
        };

        let (watcher_state, watcher_ref_count, watcher_grace_active) =
            if let Some(workspace_id) = primary_workspace_id.as_deref() {
                if let Some(pool) = handler.watcher_pool.as_ref() {
                    let ref_count = pool.ref_count(workspace_id).await;
                    let grace_active = pool.has_grace_deadline(workspace_id).await;
                    let watcher_state = if ref_count > 0 {
                        WatcherState::SharedActive
                    } else if grace_active {
                        WatcherState::SharedGrace
                    } else {
                        WatcherState::SharedIdle
                    };
                    (watcher_state, Some(ref_count), grace_active)
                } else {
                    let workspace_guard = handler.workspace.read().await;
                    let watcher_state = workspace_guard
                        .as_ref()
                        .and_then(|workspace| workspace.watcher.as_ref())
                        .map(|_| WatcherState::Local)
                        .unwrap_or(WatcherState::Unavailable);
                    (watcher_state, None, false)
                }
            } else {
                (WatcherState::Unavailable, None, false)
            };

        let level = if daemon_state == DaemonLifecycleState::RestartPending
            || matches!(watcher_state, WatcherState::Unavailable) && primary_workspace_id.is_some()
        {
            HealthLevel::Degraded
        } else {
            HealthLevel::Ready
        };

        let detail = match daemon_state {
            DaemonLifecycleState::Direct => "Direct stdio session".to_string(),
            DaemonLifecycleState::Serving => "Shared daemon serving workspace sessions".to_string(),
            DaemonLifecycleState::RestartPending => {
                "Daemon restart is pending after a binary refresh".to_string()
            }
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

    fn build_data_plane(primary: &PrimaryWorkspaceHealth) -> Result<DataPlaneHealth> {
        match primary {
            PrimaryWorkspaceHealth::ColdStart => {
                let canonical_store = CanonicalStoreHealth {
                    level: HealthLevel::Unavailable,
                    symbol_count: 0,
                    file_count: 0,
                    relationship_count: 0,
                    embedding_count: 0,
                    db_size_mb: 0.0,
                    languages: Vec::new(),
                    detail: "No primary workspace is indexed".to_string(),
                };
                let search_projection = SearchProjectionHealth {
                    level: HealthLevel::Unavailable,
                    state: ProjectionState::Missing,
                    freshness: ProjectionFreshness::Unavailable,
                    workspace_id: None,
                    canonical_revision: None,
                    projected_revision: None,
                    revision_lag: None,
                    repair_needed: false,
                    detail: "No Tantivy projection exists because no primary workspace is bound"
                        .to_string(),
                };
                let indexing = IndexingHealth {
                    level: HealthLevel::Unavailable,
                    active_operation: None,
                    stage: None,
                    catchup_active: false,
                    watcher_paused: false,
                    watcher_rescan_pending: false,
                    dirty_projection_count: 0,
                    repair_needed: false,
                    repair_issue_count: 0,
                    repair_reasons: Vec::new(),
                    detail: "No indexing runtime is attached because no primary workspace is bound"
                        .to_string(),
                };
                Ok(DataPlaneHealth {
                    level: HealthLevel::Unavailable,
                    canonical_store,
                    search_projection,
                    indexing,
                })
            }
            PrimaryWorkspaceHealth::Ready(state) => {
                let canonical_store = match state.database.as_ref() {
                    Some(database) => match database.try_lock() {
                        Ok(db_lock) => match db_lock.get_stats() {
                            Ok(stats) => {
                                let level = if stats.total_symbols > 0 {
                                    HealthLevel::Ready
                                } else {
                                    HealthLevel::Unavailable
                                };
                                CanonicalStoreHealth {
                                    level,
                                    symbol_count: stats.total_symbols,
                                    file_count: stats.total_files,
                                    relationship_count: stats.total_relationships,
                                    embedding_count: stats.embedding_count,
                                    db_size_mb: stats.db_size_mb,
                                    languages: stats.languages,
                                    detail: if stats.total_symbols > 0 {
                                        format!(
                                            "{} symbols across {} files",
                                            stats.total_symbols, stats.total_files
                                        )
                                    } else {
                                        "SQLite opened but has no indexed symbols".to_string()
                                    },
                                }
                            }
                            Err(err) => CanonicalStoreHealth {
                                level: HealthLevel::Unavailable,
                                symbol_count: 0,
                                file_count: 0,
                                relationship_count: 0,
                                embedding_count: 0,
                                db_size_mb: 0.0,
                                languages: Vec::new(),
                                detail: format!("Failed to read SQLite stats: {}", err),
                            },
                        },
                        Err(_busy) => {
                            debug!(
                                "Primary symbol database busy during health snapshot; assuming data present"
                            );
                            CanonicalStoreHealth {
                                level: HealthLevel::Degraded,
                                symbol_count: 1,
                                file_count: 0,
                                relationship_count: 0,
                                embedding_count: 0,
                                db_size_mb: 0.0,
                                languages: Vec::new(),
                                detail:
                                    "SQLite database is busy; counts are temporarily unavailable"
                                        .to_string(),
                            }
                        }
                    },
                    None => CanonicalStoreHealth {
                        level: HealthLevel::Unavailable,
                        symbol_count: 0,
                        file_count: 0,
                        relationship_count: 0,
                        embedding_count: 0,
                        db_size_mb: 0.0,
                        languages: Vec::new(),
                        detail: "No SQLite database is connected for the primary workspace"
                            .to_string(),
                    },
                };

                let search_projection = match state.database.as_ref() {
                    Some(database) => match database.try_lock() {
                        Ok(db_lock) => Self::search_projection_health_for_workspace(
                            &state.binding.workspace_id,
                            &db_lock,
                            canonical_store.symbol_count,
                            state.search_index_ready,
                        )
                        .unwrap_or_else(|err| SearchProjectionHealth {
                            level: HealthLevel::Unavailable,
                            state: ProjectionState::Missing,
                            freshness: ProjectionFreshness::Unavailable,
                            workspace_id: Some(state.binding.workspace_id.clone()),
                            canonical_revision: None,
                            projected_revision: None,
                            revision_lag: None,
                            repair_needed: false,
                            detail: format!("Failed to read projection state: {}", err),
                        }),
                        Err(_busy) => SearchProjectionHealth {
                            level: HealthLevel::Degraded,
                            state: if state.search_index_ready {
                                ProjectionState::Ready
                            } else {
                                ProjectionState::Missing
                            },
                            freshness: if state.search_index_ready {
                                ProjectionFreshness::Lagging
                            } else {
                                ProjectionFreshness::RebuildRequired
                            },
                            workspace_id: Some(state.binding.workspace_id.clone()),
                            canonical_revision: None,
                            projected_revision: None,
                            revision_lag: None,
                            repair_needed: true,
                            detail:
                                "SQLite database is busy; projection freshness is temporarily unavailable"
                                    .to_string(),
                        },
                    },
                    None => SearchProjectionHealth {
                        level: HealthLevel::Unavailable,
                        state: ProjectionState::Missing,
                        freshness: ProjectionFreshness::Unavailable,
                        workspace_id: Some(state.binding.workspace_id.clone()),
                        canonical_revision: None,
                        projected_revision: None,
                        revision_lag: None,
                        repair_needed: false,
                        detail: "No SQLite database is connected for the primary workspace"
                            .to_string(),
                    },
                };

                let indexing = indexing_health(state.indexing_runtime.as_ref());

                Ok(DataPlaneHealth {
                    level: overall_from_levels(&[
                        canonical_store.level,
                        search_projection.level,
                        indexing.level,
                    ]),
                    canonical_store,
                    search_projection,
                    indexing,
                })
            }
        }
    }

    pub(crate) fn search_projection_health_for_workspace(
        workspace_id: &str,
        db: &crate::database::SymbolDatabase,
        symbol_count: i64,
        search_index_ready: bool,
    ) -> Result<SearchProjectionHealth> {
        if symbol_count <= 0 {
            return Ok(SearchProjectionHealth {
                level: HealthLevel::Unavailable,
                state: ProjectionState::Missing,
                freshness: ProjectionFreshness::Unavailable,
                workspace_id: Some(workspace_id.to_string()),
                canonical_revision: None,
                projected_revision: None,
                revision_lag: None,
                repair_needed: false,
                detail: "Tantivy projection is not usable because SQLite has no indexed symbols"
                    .to_string(),
            });
        }

        let canonical_revision = db.get_current_canonical_revision(workspace_id)?;
        if canonical_revision.is_none() {
            return Ok(SearchProjectionHealth {
                level: HealthLevel::Degraded,
                state: if search_index_ready {
                    ProjectionState::Ready
                } else {
                    ProjectionState::Missing
                },
                freshness: ProjectionFreshness::RebuildRequired,
                workspace_id: Some(workspace_id.to_string()),
                canonical_revision: None,
                projected_revision: None,
                revision_lag: None,
                repair_needed: true,
                detail: format!(
                    "Canonical revision metadata is missing for workspace {workspace_id}; projection repair is required"
                ),
            });
        }

        let projection = db.get_projection_state(TANTIVY_PROJECTION_NAME, workspace_id)?;
        let projected_revision = projection.as_ref().and_then(projected_revision_from_state);
        let revision_lag = canonical_revision
            .zip(projected_revision)
            .map(|(canonical, projected)| canonical.saturating_sub(projected));

        let state = if search_index_ready {
            ProjectionState::Ready
        } else {
            ProjectionState::Missing
        };

        let freshness = if !search_index_ready {
            ProjectionFreshness::RebuildRequired
        } else {
            match projection.as_ref().map(|state| state.status) {
                Some(DbProjectionStatus::Ready) if projected_revision == canonical_revision => {
                    ProjectionFreshness::Current
                }
                Some(DbProjectionStatus::Ready) if revision_lag.unwrap_or(0) > 0 => {
                    ProjectionFreshness::Lagging
                }
                Some(DbProjectionStatus::Building) if revision_lag.unwrap_or(0) > 0 => {
                    ProjectionFreshness::Lagging
                }
                Some(DbProjectionStatus::Ready | DbProjectionStatus::Building)
                | Some(DbProjectionStatus::Missing | DbProjectionStatus::Stale)
                | None => ProjectionFreshness::RebuildRequired,
            }
        };

        let repair_needed = matches!(
            freshness,
            ProjectionFreshness::Lagging | ProjectionFreshness::RebuildRequired
        );

        let level = match freshness {
            ProjectionFreshness::Current if state == ProjectionState::Ready => HealthLevel::Ready,
            ProjectionFreshness::Unavailable => HealthLevel::Unavailable,
            _ => HealthLevel::Degraded,
        };

        let detail = projection_detail(
            workspace_id,
            state,
            freshness,
            canonical_revision,
            projected_revision,
            projection
                .as_ref()
                .and_then(|state| state.detail.as_deref()),
        );

        Ok(SearchProjectionHealth {
            level,
            state,
            freshness,
            workspace_id: Some(workspace_id.to_string()),
            canonical_revision,
            projected_revision,
            revision_lag,
            repair_needed,
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

fn projected_revision_from_state(state: &crate::database::ProjectionState) -> Option<i64> {
    state.projected_revision.or_else(|| {
        if state.status == DbProjectionStatus::Ready {
            state.canonical_revision
        } else {
            None
        }
    })
}

fn projection_detail(
    workspace_id: &str,
    state: ProjectionState,
    freshness: ProjectionFreshness,
    canonical_revision: Option<i64>,
    projected_revision: Option<i64>,
    state_detail: Option<&str>,
) -> String {
    match (state, freshness, canonical_revision, projected_revision) {
        (_, ProjectionFreshness::Unavailable, _, _) => {
            "Tantivy projection is not usable because SQLite has no indexed symbols".to_string()
        }
        (ProjectionState::Missing, _, _, Some(projected)) => format!(
            "Tantivy handle is unavailable for workspace {workspace_id}; last recorded projection revision {projected}"
        ),
        (ProjectionState::Missing, _, Some(canonical), None) => format!(
            "Tantivy handle is unavailable for workspace {workspace_id}; canonical revision {canonical} needs rebuild"
        ),
        (
            ProjectionState::Ready,
            ProjectionFreshness::Current,
            Some(canonical),
            Some(projected),
        ) => {
            format!(
                "Tantivy projection is current for workspace {workspace_id} at revision {projected}/{canonical}"
            )
        }
        (
            ProjectionState::Ready,
            ProjectionFreshness::Lagging,
            Some(canonical),
            Some(projected),
        ) => {
            format!(
                "Tantivy projection for workspace {workspace_id} is lagging at revision {projected}/{canonical}"
            )
        }
        (
            ProjectionState::Ready,
            ProjectionFreshness::RebuildRequired,
            Some(canonical),
            Some(projected),
        ) => match state_detail {
            Some(detail) if !detail.is_empty() => format!(
                "Projection repair needed for workspace {workspace_id}; serving revision {projected}/{canonical}: {detail}"
            ),
            _ => format!(
                "Projection repair needed for workspace {workspace_id}; serving revision {projected}/{canonical}"
            ),
        },
        (ProjectionState::Ready, ProjectionFreshness::RebuildRequired, Some(canonical), None) => {
            match state_detail {
                Some(detail) if !detail.is_empty() => format!(
                    "Projection revision metadata is incomplete for workspace {workspace_id}; canonical revision {canonical}: {detail}"
                ),
                _ => format!(
                    "Projection revision metadata is incomplete for workspace {workspace_id}; canonical revision {canonical}"
                ),
            }
        }
        (_, _, Some(canonical), None) => {
            format!(
                "Tantivy projection detail unavailable for workspace {workspace_id}; canonical revision {canonical}"
            )
        }
        _ => "Tantivy projection state is unavailable".to_string(),
    }
}

fn indexing_health(
    snapshot: Option<&crate::tools::workspace::indexing::state::IndexingRuntimeSnapshot>,
) -> IndexingHealth {
    let Some(snapshot) = snapshot else {
        return IndexingHealth {
            level: HealthLevel::Ready,
            active_operation: None,
            stage: None,
            catchup_active: false,
            watcher_paused: false,
            watcher_rescan_pending: false,
            dirty_projection_count: 0,
            repair_needed: false,
            repair_issue_count: 0,
            repair_reasons: Vec::new(),
            detail: "Indexing idle".to_string(),
        };
    };

    let repair_reasons = snapshot
        .repair_reasons
        .iter()
        .map(|reason| reason.as_str().to_string())
        .collect::<Vec<_>>();
    let repair_needed = snapshot.repair_needed();
    let repair_issue_count = snapshot.repair_issue_count();

    let level = if repair_needed
        || snapshot.dirty_projection_count > 0
        || snapshot.watcher_paused
        || snapshot.watcher_rescan_pending
        || snapshot.catchup_active
        || snapshot.active_operation.is_some()
    {
        HealthLevel::Degraded
    } else {
        HealthLevel::Ready
    };

    let detail = if snapshot.active_operation.is_none()
        && !snapshot.catchup_active
        && !snapshot.watcher_paused
        && !snapshot.watcher_rescan_pending
        && snapshot.dirty_projection_count == 0
        && !repair_needed
    {
        "Indexing idle".to_string()
    } else {
        let mut parts = Vec::new();
        if let Some(operation) = snapshot.active_operation {
            parts.push(format!("operation {}", operation.as_str()));
        }
        if let Some(stage) = snapshot.stage {
            parts.push(format!("stage {}", stage.as_str()));
        }
        if snapshot.catchup_active {
            parts.push("catch-up active".to_string());
        }
        if snapshot.watcher_paused {
            parts.push("watcher paused".to_string());
        }
        if snapshot.watcher_rescan_pending {
            parts.push("watcher rescan pending".to_string());
        }
        if snapshot.dirty_projection_count > 0 {
            parts.push(format!(
                "{} dirty projection entries",
                snapshot.dirty_projection_count
            ));
        }
        if repair_needed {
            parts.push(format!("{repair_issue_count} repair issue(s)"));
        }
        parts.join(", ")
    };

    IndexingHealth {
        level,
        active_operation: snapshot
            .active_operation
            .map(|operation| operation.as_str().to_string()),
        stage: snapshot.stage.map(|stage| stage.as_str().to_string()),
        catchup_active: snapshot.catchup_active,
        watcher_paused: snapshot.watcher_paused,
        watcher_rescan_pending: snapshot.watcher_rescan_pending,
        dirty_projection_count: snapshot.dirty_projection_count,
        repair_needed,
        repair_issue_count,
        repair_reasons,
        detail,
    }
}
