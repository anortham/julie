use super::{HealthLevel, IndexingHealth};

pub(crate) fn indexing_health(
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
