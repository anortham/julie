use anyhow::Result;
use julie_pipeline::indexing_core::web_edges::WEB_EDGES_PROJECTION_NAME;

use crate::{handler::JulieServerHandler, search::projection::TANTIVY_PROJECTION_NAME};

use super::{
    CanonicalStoreHealth, DataPlaneHealth, HealthLevel, PrimaryWorkspaceHealth,
    ProjectionFreshness, ProjectionHealth, ProjectionPolicy, ProjectionState, indexing_health,
    overall_from_levels, projection_health_for_workspace,
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
            let projections = vec![
                unavailable_projection(
                    TANTIVY_PROJECTION_NAME,
                    None,
                    "No Tantivy projection exists because no primary workspace is bound",
                ),
                unavailable_projection(
                    WEB_EDGES_PROJECTION_NAME,
                    None,
                    "No web_edges projection exists because no primary workspace is bound",
                ),
            ];
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
                projections,
                indexing,
            })
        }
        PrimaryWorkspaceHealth::Ready(state) => {
            let workspace_id = state.binding.workspace_id.as_str();

            let pooled_db = handler
                .get_pooled_database_for_workspace(workspace_id)
                .await
                .ok();
            let store = handler
                .checkout_store_for_workspace(workspace_id, &state.binding.workspace_root)
                .await
                .ok();
            let canonical_store = match store.as_ref() {
                Some(store) => canonical_store_from_checkout(store),
                None => CanonicalStoreHealth {
                    level: HealthLevel::Unavailable,
                    symbol_count: 0,
                    file_count: 0,
                    relationship_count: 0,
                    embedding_count: 0,
                    db_size_mb: 0.0,
                    languages: Vec::new(),
                    detail: "Checkout store failed to open for the primary workspace".to_string(),
                },
            };

            let projections: Vec<ProjectionHealth> = match pooled_db.as_ref() {
                Some(db) => [
                    ProjectionPolicy::Tantivy {
                        physical_ready: state.search_index_ready,
                    },
                    ProjectionPolicy::WebEdges,
                ]
                .into_iter()
                .map(|policy| {
                    let name = match policy {
                        ProjectionPolicy::Tantivy { .. } => TANTIVY_PROJECTION_NAME,
                        ProjectionPolicy::WebEdges => WEB_EDGES_PROJECTION_NAME,
                    };
                    projection_health_for_workspace(
                        workspace_id,
                        db,
                        canonical_store.symbol_count,
                        policy,
                    )
                    .unwrap_or_else(|err| {
                        unavailable_projection(
                            name,
                            Some(workspace_id),
                            &format!("Failed to read {name} projection state: {err}"),
                        )
                    })
                })
                .collect(),
                None => [TANTIVY_PROJECTION_NAME, WEB_EDGES_PROJECTION_NAME]
                    .into_iter()
                    .map(|name| {
                        unavailable_projection(
                            name,
                            Some(workspace_id),
                            "No SQLite database is connected for the primary workspace",
                        )
                    })
                    .collect(),
            };

            let mut projections = projections;
            if let Some(store) = store.as_ref() {
                overlay_tantivy_from_store(&mut projections, workspace_id, &store.status());
            }

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

fn canonical_store_from_checkout(
    store: &julie_index::checkout_store::CheckoutStore,
) -> CanonicalStoreHealth {
    let status = store.status();
    let symbol_count = status.graph.symbols as i64;
    let file_count = store.current().graph().paths().len() as i64;
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
        languages: Vec::new(),
        detail: if ready {
            format!("{symbol_count} symbols across {file_count} files")
        } else {
            "Checkout store opened but has no indexed symbols".to_string()
        },
    }
}

fn overlay_tantivy_from_store(
    projections: &mut [ProjectionHealth],
    workspace_id: &str,
    status: &julie_index::checkout_store::StoreStatus,
) {
    if status.tantivy != julie_index::checkout_store::TantivyState::Present
        || status.graph.symbols == 0
    {
        return;
    }
    if let Some(projection) = projections
        .iter_mut()
        .find(|projection| projection.name == TANTIVY_PROJECTION_NAME)
    {
        projection.level = HealthLevel::Ready;
        projection.state = ProjectionState::Ready;
        projection.freshness = ProjectionFreshness::Current;
        projection.repair_needed = false;
        projection.workspace_id = Some(workspace_id.to_string());
        projection.detail = "Tantivy projection present beside facts.sqlite".to_string();
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
