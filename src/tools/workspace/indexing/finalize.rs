use anyhow::Result;
use tracing::{info, warn};

use super::route::IndexRoute;
use crate::handler::JulieServerHandler;

// resolve_pending_relationships relocated to julie_pipeline::finalize
pub(crate) use julie_pipeline::finalize::resolve_pending_relationships;

pub(crate) fn analyze_batch(
    handler: &JulieServerHandler,
    route: &IndexRoute,
    db: &std::sync::Arc<std::sync::Mutex<crate::database::SymbolDatabase>>,
) -> Result<()> {
    let t = std::time::Instant::now();
    {
        let db_lock = match db.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                warn!("Database mutex poisoned during reference scoring, recovering");
                poisoned.into_inner()
            }
        };
        if let Err(e) = db_lock.compute_reference_scores() {
            warn!("Failed to compute reference scores: {}", e);
        }
    }
    info!(
        "⏱️  compute_reference_scores: {:.2}s",
        t.elapsed().as_secs_f64()
    );

    let language_configs = crate::search::LanguageConfigs::load_embedded();
    let t = std::time::Instant::now();
    {
        let db_lock = match db.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                warn!("Database mutex poisoned during test quality analysis, recovering");
                poisoned.into_inner()
            }
        };
        if let Err(e) = crate::analysis::compute_test_quality_metrics(&db_lock, &language_configs) {
            warn!("Failed to compute test quality metrics: {}", e);
        }
    }
    info!(
        "⏱️  compute_test_quality_metrics: {:.2}s",
        t.elapsed().as_secs_f64()
    );

    let t = std::time::Instant::now();
    {
        let db_lock = match db.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                warn!("Database mutex poisoned during test linkage analysis, recovering");
                poisoned.into_inner()
            }
        };
        if let Err(e) = crate::analysis::compute_test_linkage(&db_lock) {
            warn!("Failed to compute test linkage: {}", e);
        }
    }
    info!(
        "⏱️  compute_test_linkage: {:.2}s",
        t.elapsed().as_secs_f64()
    );

    if let Some(ref daemon_db) = handler.daemon_db {
        let current_primary_id = if route.is_primary {
            handler
                .current_workspace_id()
                .or_else(|| handler.loaded_workspace_id())
        } else {
            None
        };
        let snapshot_ws_id = current_primary_id.as_deref().unwrap_or(&route.workspace_id);
        {
            let db_lock = match db.lock() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    warn!("Database mutex poisoned during codehealth snapshot, recovering");
                    poisoned.into_inner()
                }
            };
            if let Err(e) = daemon_db.snapshot_codehealth_from_db(snapshot_ws_id, &db_lock) {
                warn!("Failed to capture codehealth snapshot: {}", e);
            } else {
                info!(workspace_id = %snapshot_ws_id, "Codehealth snapshot captured");
            }
        }
    }

    Ok(())
}
