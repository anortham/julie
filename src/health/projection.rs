use crate::{
    database::ProjectionStatus as DbProjectionStatus, search::projection::TANTIVY_PROJECTION_NAME,
};
use anyhow::Result;

use super::{HealthLevel, ProjectionFreshness, ProjectionState, SearchProjectionHealth};

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
