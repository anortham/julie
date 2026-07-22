use crate::{
    database::{ProjectionState as DbProjectionState, ProjectionStatus as DbProjectionStatus},
    search::projection::TANTIVY_PROJECTION_NAME,
};
use anyhow::Result;
use julie_pipeline::indexing_core::web_edges::WEB_EDGES_PROJECTION_NAME;

use super::{HealthLevel, ProjectionFreshness, ProjectionHealth, ProjectionState};

#[derive(Debug, Clone, Copy)]
pub(crate) enum ProjectionPolicy {
    Tantivy { physical_ready: bool },
    WebEdges,
}

impl ProjectionPolicy {
    fn name(self) -> &'static str {
        match self {
            Self::Tantivy { .. } => TANTIVY_PROJECTION_NAME,
            Self::WebEdges => WEB_EDGES_PROJECTION_NAME,
        }
    }

    fn display_name(self) -> &'static str {
        match self {
            Self::Tantivy { .. } => "Tantivy",
            Self::WebEdges => WEB_EDGES_PROJECTION_NAME,
        }
    }

    fn state(self, projection: Option<&DbProjectionState>) -> ProjectionState {
        match self {
            Self::Tantivy { physical_ready } if physical_ready => ProjectionState::Ready,
            Self::Tantivy { .. } => ProjectionState::Missing,
            Self::WebEdges
                if projection.is_some_and(|state| state.status == DbProjectionStatus::Ready) =>
            {
                ProjectionState::Ready
            }
            Self::WebEdges => ProjectionState::Missing,
        }
    }

    fn projected_revision(self, projection: Option<&DbProjectionState>) -> Option<i64> {
        let projection = projection?;
        match self {
            Self::Tantivy { .. } => projection.projected_revision.or_else(|| {
                (projection.status == DbProjectionStatus::Ready)
                    .then_some(projection.canonical_revision)
                    .flatten()
            }),
            Self::WebEdges => projection.projected_revision,
        }
    }

    fn can_be_current(self) -> bool {
        match self {
            Self::Tantivy { physical_ready } => physical_ready,
            Self::WebEdges => true,
        }
    }
}

pub(crate) fn projection_health_for_workspace(
    workspace_id: &str,
    db: &crate::database::SymbolDatabase,
    symbol_count: i64,
    policy: ProjectionPolicy,
) -> Result<ProjectionHealth> {
    let name = policy.name();
    if symbol_count <= 0 {
        return Ok(ProjectionHealth {
            name: name.to_string(),
            level: HealthLevel::Unavailable,
            state: ProjectionState::Missing,
            freshness: ProjectionFreshness::Unavailable,
            workspace_id: Some(workspace_id.to_string()),
            canonical_revision: None,
            projected_revision: None,
            revision_lag: None,
            repair_needed: false,
            detail: format!(
                "{} projection is not usable because SQLite has no indexed symbols",
                policy.display_name()
            ),
        });
    }

    let projection = db.get_projection_state(name, workspace_id)?;
    let canonical_revision = db.get_current_canonical_revision(workspace_id)?;
    let projected_revision = policy.projected_revision(projection.as_ref());
    let state = policy.state(projection.as_ref());

    if canonical_revision.is_none() {
        return Ok(ProjectionHealth {
            name: name.to_string(),
            level: HealthLevel::Degraded,
            state,
            freshness: ProjectionFreshness::RebuildRequired,
            workspace_id: Some(workspace_id.to_string()),
            canonical_revision: None,
            projected_revision,
            revision_lag: None,
            repair_needed: true,
            detail: format!(
                "Canonical revision metadata is missing for workspace {workspace_id}; canonical-store repair is required before {name} freshness can be evaluated"
            ),
        });
    }

    let revision_lag = canonical_revision
        .zip(projected_revision)
        .map(|(canonical, projected)| canonical.saturating_sub(projected));
    let freshness = projection_freshness(
        policy,
        projection.as_ref(),
        canonical_revision,
        projected_revision,
        revision_lag,
    );
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
        policy,
        state,
        freshness,
        canonical_revision,
        projected_revision,
        projection
            .as_ref()
            .and_then(|state| state.detail.as_deref()),
    );

    Ok(ProjectionHealth {
        name: name.to_string(),
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

fn projection_freshness(
    policy: ProjectionPolicy,
    projection: Option<&DbProjectionState>,
    canonical_revision: Option<i64>,
    projected_revision: Option<i64>,
    revision_lag: Option<i64>,
) -> ProjectionFreshness {
    if !policy.can_be_current() {
        return ProjectionFreshness::RebuildRequired;
    }

    match projection.map(|state| state.status) {
        Some(DbProjectionStatus::Ready) if projected_revision == canonical_revision => {
            ProjectionFreshness::Current
        }
        Some(DbProjectionStatus::Ready | DbProjectionStatus::Building)
            if revision_lag.unwrap_or(0) > 0 =>
        {
            ProjectionFreshness::Lagging
        }
        Some(
            DbProjectionStatus::Ready
            | DbProjectionStatus::Building
            | DbProjectionStatus::Missing
            | DbProjectionStatus::Stale,
        )
        | None => ProjectionFreshness::RebuildRequired,
    }
}

fn projection_detail(
    workspace_id: &str,
    policy: ProjectionPolicy,
    state: ProjectionState,
    freshness: ProjectionFreshness,
    canonical_revision: Option<i64>,
    projected_revision: Option<i64>,
    state_detail: Option<&str>,
) -> String {
    let name = policy.display_name();
    match (state, freshness, canonical_revision, projected_revision) {
        (_, ProjectionFreshness::Unavailable, _, _) => {
            format!("{name} projection is not usable because SQLite has no indexed symbols")
        }
        (ProjectionState::Missing, _, _, Some(projected)) => match policy {
            ProjectionPolicy::Tantivy { .. } => format!(
                "Tantivy handle is unavailable for workspace {workspace_id}; last recorded projection revision {projected}"
            ),
            ProjectionPolicy::WebEdges => format!(
                "{name} durable projection state is unavailable for workspace {workspace_id}; last recorded revision {projected}"
            ),
        },
        (ProjectionState::Missing, _, Some(canonical), None) => match policy {
            ProjectionPolicy::Tantivy { .. } => format!(
                "Tantivy handle is unavailable for workspace {workspace_id}; canonical revision {canonical} needs rebuild"
            ),
            ProjectionPolicy::WebEdges => format!(
                "{name} durable projection state is unavailable for workspace {workspace_id}; canonical revision {canonical} needs rebuild"
            ),
        },
        (
            ProjectionState::Ready,
            ProjectionFreshness::Current,
            Some(canonical),
            Some(projected),
        ) => format!(
            "{name} projection is current for workspace {workspace_id} at revision {projected}/{canonical}"
        ),
        (
            ProjectionState::Ready,
            ProjectionFreshness::Lagging,
            Some(canonical),
            Some(projected),
        ) => format!(
            "{name} projection for workspace {workspace_id} is lagging at revision {projected}/{canonical}"
        ),
        (
            ProjectionState::Ready,
            ProjectionFreshness::RebuildRequired,
            Some(canonical),
            projected,
        ) => {
            let serving = projected
                .map(|revision| format!("; serving revision {revision}/{canonical}"))
                .unwrap_or_default();
            let detail = state_detail
                .filter(|detail| !detail.is_empty())
                .map(|detail| format!(": {detail}"))
                .unwrap_or_default();
            format!("{name} projection repair needed for workspace {workspace_id}{serving}{detail}")
        }
        (_, _, Some(canonical), None) => format!(
            "{name} projection detail unavailable for workspace {workspace_id}; canonical revision {canonical}"
        ),
        _ => format!("{name} projection state is unavailable"),
    }
}
