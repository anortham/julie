use anyhow::Result;

use crate::handler::JulieServerHandler;

use super::{
    CanonicalStoreHealth, DataPlaneHealth, HealthLevel, PrimaryWorkspaceHealth,
    ProjectionFreshness, ProjectionState, indexing_health, overall_from_levels,
    search_projection_health_for_workspace,
};

pub(crate) async fn build_data_plane(
    handler: &JulieServerHandler,
    primary: &PrimaryWorkspaceHealth,
) -> Result<DataPlaneHealth> {
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
            let workspace_id = state.binding.workspace_id.as_str();

            let pooled_db = handler
                .get_pooled_database_for_workspace(workspace_id)
                .await
                .ok();

            let canonical_store = match pooled_db.as_ref() {
                Some(db) => match db.get_stats() {
                    Ok(stats) => {
                        // Detect the phantom-fd state: SQLite reports symbols but the
                        // on-disk file is gone (size 0). This happens when the index
                        // directory is removed while the daemon holds the SQLite fd
                        // open — reads keep working but the data is unrecoverable.
                        let phantom_fd = stats.total_symbols > 0 && stats.db_size_mb == 0.0;
                        let level = if phantom_fd {
                            HealthLevel::Unavailable
                        } else if stats.total_symbols > 0 {
                            HealthLevel::Ready
                        } else {
                            HealthLevel::Unavailable
                        };
                        let detail = if phantom_fd {
                            format!(
                                "ON-DISK STATE MISSING: SQLite reports {} symbols but db file size is 0 MB. \
                                 Index directory was removed while daemon was running. \
                                 Restart daemon and force-reindex to recover.",
                                stats.total_symbols
                            )
                        } else if stats.total_symbols > 0 {
                            format!(
                                "{} symbols across {} files",
                                stats.total_symbols, stats.total_files
                            )
                        } else {
                            "SQLite opened but has no indexed symbols".to_string()
                        };
                        CanonicalStoreHealth {
                            level,
                            symbol_count: stats.total_symbols,
                            file_count: stats.total_files,
                            relationship_count: stats.total_relationships,
                            embedding_count: stats.embedding_count,
                            db_size_mb: stats.db_size_mb,
                            languages: stats.languages,
                            detail,
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

            let search_projection = match pooled_db.as_ref() {
                Some(db) => search_projection_health_for_workspace(
                    workspace_id,
                    db,
                    canonical_store.symbol_count,
                    state.search_index_ready,
                )
                .unwrap_or_else(|err| crate::health::SearchProjectionHealth {
                    level: HealthLevel::Unavailable,
                    state: ProjectionState::Missing,
                    freshness: ProjectionFreshness::Unavailable,
                    workspace_id: Some(workspace_id.to_string()),
                    canonical_revision: None,
                    projected_revision: None,
                    revision_lag: None,
                    repair_needed: false,
                    detail: format!("Failed to read projection state: {}", err),
                }),
                None => crate::health::SearchProjectionHealth {
                    level: HealthLevel::Unavailable,
                    state: ProjectionState::Missing,
                    freshness: ProjectionFreshness::Unavailable,
                    workspace_id: Some(workspace_id.to_string()),
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
