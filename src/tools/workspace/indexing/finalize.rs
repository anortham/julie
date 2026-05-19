use std::collections::HashSet;

use anyhow::Result;
use tracing::{info, warn};

use super::resolver;
use super::route::IndexRoute;
use crate::extractors::PendingRelationship;
use crate::handler::JulieServerHandler;
use julie_extractors::base::StructuredPendingRelationship;

pub(crate) fn resolve_pending_relationships(
    db: &std::sync::Arc<std::sync::Mutex<crate::database::SymbolDatabase>>,
    pending_relationships: &[PendingRelationship],
    structured_pending_relationships: &[StructuredPendingRelationship],
) {
    if pending_relationships.is_empty() && structured_pending_relationships.is_empty() {
        return;
    }

    let resolution_start = std::time::Instant::now();
    let mut db_lock = match db.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            warn!("Database mutex poisoned during relationship resolution, recovering");
            poisoned.into_inner()
        }
    };

    let (resolved_relationships, stats) = if structured_pending_relationships.is_empty() {
        resolver::resolve_batch(pending_relationships, &db_lock)
    } else {
        let (mut resolved, mut stats) =
            resolver::resolve_structured_batch(structured_pending_relationships, &db_lock);
        let structured_keys: HashSet<_> = structured_pending_relationships
            .iter()
            .map(|structured| pending_key(&structured.pending))
            .collect();
        let legacy_only: Vec<PendingRelationship> = pending_relationships
            .iter()
            .filter(|pending| !structured_keys.contains(&pending_key(pending)))
            .cloned()
            .collect();
        if !legacy_only.is_empty() {
            let (legacy_resolved, legacy_stats) = resolver::resolve_batch(&legacy_only, &db_lock);
            resolved.extend(legacy_resolved);
            stats.total += legacy_stats.total;
            stats.resolved += legacy_stats.resolved;
            stats.no_candidates += legacy_stats.no_candidates;
            stats.no_valid_candidates += legacy_stats.no_valid_candidates;
            stats.lookup_errors += legacy_stats.lookup_errors;
        }
        (resolved, stats)
    };
    if !resolved_relationships.is_empty()
        && let Err(e) = db_lock.bulk_store_relationships(&resolved_relationships)
    {
        warn!("Failed to store resolved relationships: {}", e);
    }

    stats.log_summary();
    info!(
        "⏱️  resolve_pending_relationships: {:.2}s",
        resolution_start.elapsed().as_secs_f64()
    );
}

fn pending_key(
    pending: &PendingRelationship,
) -> (
    &str,
    &str,
    &crate::extractors::base::RelationshipKind,
    &str,
    u32,
) {
    (
        pending.from_symbol_id.as_str(),
        pending.callee_name.as_str(),
        &pending.kind,
        pending.file_path.as_str(),
        pending.line_number,
    )
}

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
