use std::collections::HashSet;

use tracing::{info, warn};

use crate::resolver;
use julie_extractors::PendingRelationship;
use julie_extractors::base::StructuredPendingRelationship;

pub fn resolve_pending_relationships(
    db: &std::sync::Arc<std::sync::Mutex<julie_core::database::SymbolDatabase>>,
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
    &julie_extractors::base::RelationshipKind,
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
