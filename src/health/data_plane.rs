use anyhow::Result;
use julie_index::checkout_store::{CheckoutStore, StoreStatus, TantivyState};

use crate::handler::JulieServerHandler;
use crate::search::projection::TANTIVY_PROJECTION_NAME;

use super::{
    CanonicalStoreHealth, DataPlaneHealth, HealthLevel, PrimaryWorkspaceHealth,
    ProjectionFreshness, ProjectionHealth, ProjectionState, indexing_health, overall_from_levels,
};

pub(crate) async fn build_data_plane(
    handler: &JulieServerHandler,
    primary: &PrimaryWorkspaceHealth,
) -> Result<DataPlaneHealth> {
    match primary {
        PrimaryWorkspaceHealth::ColdStart => Ok(unavailable_data_plane(
            "No primary workspace is indexed",
            "No Tantivy projection exists because no primary workspace is bound",
            "No indexing runtime is attached because no primary workspace is bound",
        )),
        PrimaryWorkspaceHealth::Ready(state) => {
            let workspace_id = state.binding.workspace_id.as_str();
            let store = handler
                .checkout_store_for_workspace(workspace_id, &state.binding.workspace_root)
                .await
                .ok();
            let (canonical_store, projections) = match store.as_ref() {
                Some(store) => {
                    let status = store.status();
                    (
                        canonical_store_from_status(store, &status),
                        vec![tantivy_from_status(workspace_id, &status)],
                    )
                }
                None => (
                    CanonicalStoreHealth {
                        level: HealthLevel::Unavailable,
                        symbol_count: 0,
                        file_count: 0,
                        relationship_count: 0,
                        embedding_count: 0,
                        db_size_mb: 0.0,
                        languages: Vec::new(),
                        detail: "Checkout store failed to open for the primary workspace"
                            .to_string(),
                    },
                    vec![unavailable_projection(
                        TANTIVY_PROJECTION_NAME,
                        Some(workspace_id),
                        "Checkout store failed to open for the primary workspace",
                    )],
                ),
            };

            let indexing = indexing_health(state.indexing_runtime.as_ref());
            let mut levels = vec![canonical_store.level, indexing.level];
            levels.extend(projections.iter().map(|projection| projection.level));

            Ok(DataPlaneHealth {
                level: overall_from_levels(&levels),
                canonical_store,
                projections,
                indexing,
            })
        }
    }
}

fn unavailable_data_plane(
    store_detail: &str,
    tantivy_detail: &str,
    indexing_detail: &str,
) -> DataPlaneHealth {
    DataPlaneHealth {
        level: HealthLevel::Unavailable,
        canonical_store: CanonicalStoreHealth {
            level: HealthLevel::Unavailable,
            symbol_count: 0,
            file_count: 0,
            relationship_count: 0,
            embedding_count: 0,
            db_size_mb: 0.0,
            languages: Vec::new(),
            detail: store_detail.to_string(),
        },
        projections: vec![unavailable_projection(
            TANTIVY_PROJECTION_NAME,
            None,
            tantivy_detail,
        )],
        indexing: crate::health::IndexingHealth {
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
            detail: indexing_detail.to_string(),
        },
    }
}

fn canonical_store_from_status(
    store: &CheckoutStore,
    status: &StoreStatus,
) -> CanonicalStoreHealth {
    let symbol_count = status.graph.symbols as i64;
    let file_count = store.current().graph().paths().len() as i64;
    let languages = languages_from_store(store);
    let ready = symbol_count > 0;
    CanonicalStoreHealth {
        level: if ready {
            HealthLevel::Ready
        } else {
            HealthLevel::Unavailable
        },
        symbol_count,
        file_count,
        relationship_count: status.graph.edges as i64,
        embedding_count: status.vector_count as i64,
        db_size_mb: status.facts_bytes as f64 / (1024.0 * 1024.0),
        languages,
        detail: if ready {
            format!("{symbol_count} symbols across {file_count} files")
        } else {
            "Checkout store opened but has no indexed symbols".to_string()
        },
    }
}

fn languages_from_store(store: &CheckoutStore) -> Vec<String> {
    let Ok(facts) = store.current().facts() else {
        return Vec::new();
    };
    let Ok(paths) = facts.reader().paths() else {
        return Vec::new();
    };
    let mut languages: Vec<String> = paths.into_iter().map(|row| row.language).collect();
    languages.sort();
    languages.dedup();
    languages
}

fn tantivy_from_status(workspace_id: &str, status: &StoreStatus) -> ProjectionHealth {
    if status.tantivy == TantivyState::Present && status.graph.symbols > 0 {
        ProjectionHealth {
            name: TANTIVY_PROJECTION_NAME.to_string(),
            level: HealthLevel::Ready,
            state: ProjectionState::Ready,
            freshness: ProjectionFreshness::Current,
            workspace_id: Some(workspace_id.to_string()),
            canonical_revision: None,
            projected_revision: None,
            revision_lag: None,
            repair_needed: false,
            detail: "Tantivy projection present beside facts.sqlite".to_string(),
        }
    } else {
        unavailable_projection(
            TANTIVY_PROJECTION_NAME,
            Some(workspace_id),
            "Tantivy projection is not present beside facts.sqlite",
        )
    }
}

fn unavailable_projection(
    name: &str,
    workspace_id: Option<&str>,
    detail: &str,
) -> ProjectionHealth {
    ProjectionHealth {
        name: name.to_string(),
        level: HealthLevel::Unavailable,
        state: ProjectionState::Missing,
        freshness: ProjectionFreshness::Unavailable,
        workspace_id: workspace_id.map(str::to_string),
        canonical_revision: None,
        projected_revision: None,
        revision_lag: None,
        repair_needed: false,
        detail: detail.to_string(),
    }
}
