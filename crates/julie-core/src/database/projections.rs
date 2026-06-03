use super::*;
use anyhow::{Result, anyhow};
use rusqlite::params;
use tracing::debug;

fn get_unix_timestamp() -> Result<i64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .map_err(|e| anyhow!("System time error: {}", e))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionStatus {
    Missing,
    Building,
    Ready,
    Stale,
}

impl ProjectionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Building => "building",
            Self::Ready => "ready",
            Self::Stale => "stale",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "missing" => Some(Self::Missing),
            "building" => Some(Self::Building),
            "ready" => Some(Self::Ready),
            "stale" => Some(Self::Stale),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionState {
    pub projection: String,
    pub workspace_id: String,
    pub status: ProjectionStatus,
    pub canonical_revision: Option<i64>,
    pub projected_revision: Option<i64>,
    pub detail: Option<String>,
    pub updated_at: i64,
}

impl SymbolDatabase {
    pub(crate) fn create_projection_states_table(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS projection_states (
                projection TEXT NOT NULL,
                workspace_id TEXT NOT NULL,
                status TEXT NOT NULL CHECK(status IN ('missing', 'building', 'ready', 'stale')),
                canonical_revision INTEGER,
                projected_revision INTEGER,
                detail TEXT,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY (projection, workspace_id)
            )",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_projection_states_workspace
             ON projection_states(workspace_id)",
            [],
        )?;

        debug!("Created projection_states table and indexes");
        Ok(())
    }

    pub fn upsert_projection_state(
        &self,
        projection: &str,
        workspace_id: &str,
        status: ProjectionStatus,
        canonical_revision: Option<i64>,
        projected_revision: Option<i64>,
        detail: Option<&str>,
    ) -> Result<ProjectionState> {
        let updated_at = get_unix_timestamp()?;
        self.conn.execute(
            "INSERT INTO projection_states
             (projection, workspace_id, status, canonical_revision, projected_revision, detail, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(projection, workspace_id) DO UPDATE SET
                 status = excluded.status,
                 canonical_revision = excluded.canonical_revision,
                 projected_revision = excluded.projected_revision,
                 detail = excluded.detail,
                 updated_at = excluded.updated_at",
            params![
                projection,
                workspace_id,
                status.as_str(),
                canonical_revision,
                projected_revision,
                detail,
                updated_at
            ],
        )?;

        Ok(ProjectionState {
            projection: projection.to_string(),
            workspace_id: workspace_id.to_string(),
            status,
            canonical_revision,
            projected_revision,
            detail: detail.map(str::to_string),
            updated_at,
        })
    }

    pub fn get_projection_state(
        &self,
        projection: &str,
        workspace_id: &str,
    ) -> Result<Option<ProjectionState>> {
        let mut stmt = self.conn.prepare(
            "SELECT projection, workspace_id, status, canonical_revision, projected_revision, detail, updated_at
             FROM projection_states
             WHERE projection = ?1 AND workspace_id = ?2",
        )?;
        let mut rows = stmt.query(params![projection, workspace_id])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };

        let status: String = row.get(2)?;
        let status = ProjectionStatus::from_str(&status)
            .ok_or_else(|| anyhow!("Unknown projection status: {}", status))?;

        Ok(Some(ProjectionState {
            projection: row.get(0)?,
            workspace_id: row.get(1)?,
            status,
            canonical_revision: row.get(3)?,
            projected_revision: row.get(4)?,
            detail: row.get(5)?,
            updated_at: row.get(6)?,
        }))
    }
}
