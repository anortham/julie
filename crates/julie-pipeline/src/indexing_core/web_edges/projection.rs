use anyhow::Result;
use julie_core::database::{ProjectionStatus, SymbolDatabase};

use super::rebuild_web_edges;

pub const WEB_EDGES_PROJECTION_NAME: &str = "web_edges";

pub fn rebuild_web_edges_for_workspace(
    db: &mut SymbolDatabase,
    workspace_id: &str,
) -> Result<usize> {
    let edge_count = rebuild_web_edges(db)?;
    let canonical_revision = db.get_current_canonical_revision(workspace_id)?;
    db.upsert_projection_state(
        WEB_EDGES_PROJECTION_NAME,
        workspace_id,
        ProjectionStatus::Ready,
        canonical_revision,
        canonical_revision,
        None,
    )?;
    Ok(edge_count)
}

pub fn ensure_web_edges_current(db: &mut SymbolDatabase, workspace_id: &str) -> Result<bool> {
    let canonical_revision = db.get_current_canonical_revision(workspace_id)?;
    let projection_is_current = matches!(
        db.get_projection_state(WEB_EDGES_PROJECTION_NAME, workspace_id)?,
        Some(state)
            if state.status == ProjectionStatus::Ready
                && state.canonical_revision == canonical_revision
                && state.projected_revision == canonical_revision
    );
    if projection_is_current {
        return Ok(false);
    }

    rebuild_web_edges_for_workspace(db, workspace_id)?;
    Ok(true)
}
