use anyhow::Result;
use tracing::debug;

use super::{
    CanonicalStoreHealth, DataPlaneHealth, HealthLevel, PrimaryWorkspaceHealth,
    ProjectionFreshness, ProjectionState, indexing_health, overall_from_levels,
    search_projection_health_for_workspace,
};

pub(crate) fn build_data_plane(primary: &PrimaryWorkspaceHealth) -> Result<DataPlaneHealth> {
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
            let search_projection = crate::health::SearchProjectionHealth {
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
            let indexing = crate::health::IndexingHealth {
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
                            detail: "SQLite database is busy; counts are temporarily unavailable"
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
                    detail: "No SQLite database is connected for the primary workspace".to_string(),
                },
            };

            let search_projection = match state.database.as_ref() {
                Some(database) => match database.try_lock() {
                    Ok(db_lock) => search_projection_health_for_workspace(
                        &state.binding.workspace_id,
                        &db_lock,
                        canonical_store.symbol_count,
                        state.search_index_ready,
                    )
                    .unwrap_or_else(|err| crate::health::SearchProjectionHealth {
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
                    Err(_busy) => crate::health::SearchProjectionHealth {
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
                None => crate::health::SearchProjectionHealth {
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
