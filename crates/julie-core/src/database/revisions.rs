use super::*;
use anyhow::{Result, anyhow};
use rusqlite::{Transaction, params};
use tracing::debug;

fn get_unix_timestamp() -> Result<i64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .map_err(|e| anyhow!("System time error: {}", e))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanonicalRevisionKind {
    Fresh,
    Incremental,
}

impl CanonicalRevisionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fresh => "fresh",
            Self::Incremental => "incremental",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "fresh" => Some(Self::Fresh),
            "incremental" => Some(Self::Incremental),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalRevision {
    pub revision: i64,
    pub workspace_id: String,
    pub kind: CanonicalRevisionKind,
    pub cleaned_file_count: i64,
    pub file_count: i64,
    pub symbol_count: i64,
    pub relationship_count: i64,
    pub identifier_count: i64,
    pub type_count: i64,
    pub created_at: i64,
}

pub(crate) fn record_canonical_revision_tx(
    tx: &Transaction<'_>,
    workspace_id: &str,
    kind: CanonicalRevisionKind,
    cleaned_file_count: i64,
    file_count: i64,
    symbol_count: i64,
    relationship_count: i64,
    identifier_count: i64,
    type_count: i64,
) -> Result<i64> {
    let now = get_unix_timestamp()?;
    tx.execute(
        "INSERT INTO canonical_revisions
         (workspace_id, kind, cleaned_file_count, file_count, symbol_count,
          relationship_count, identifier_count, type_count, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            workspace_id,
            kind.as_str(),
            cleaned_file_count,
            file_count,
            symbol_count,
            relationship_count,
            identifier_count,
            type_count,
            now
        ],
    )?;

    let revision = tx.last_insert_rowid();
    debug!(
        "Recorded canonical revision {} for workspace {} ({})",
        revision,
        workspace_id,
        kind.as_str()
    );
    Ok(revision)
}

impl SymbolDatabase {
    pub(crate) fn create_canonical_revisions_table(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS canonical_revisions (
                revision INTEGER PRIMARY KEY AUTOINCREMENT,
                workspace_id TEXT NOT NULL,
                kind TEXT NOT NULL CHECK(kind IN ('fresh', 'incremental')),
                cleaned_file_count INTEGER NOT NULL DEFAULT 0,
                file_count INTEGER NOT NULL DEFAULT 0,
                symbol_count INTEGER NOT NULL DEFAULT 0,
                relationship_count INTEGER NOT NULL DEFAULT 0,
                identifier_count INTEGER NOT NULL DEFAULT 0,
                type_count INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL
            )",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_canonical_revisions_workspace_revision
             ON canonical_revisions(workspace_id, revision DESC)",
            [],
        )?;

        debug!("Created canonical_revisions table and indexes");
        Ok(())
    }

    pub fn get_latest_canonical_revision(
        &self,
        workspace_id: &str,
    ) -> Result<Option<CanonicalRevision>> {
        let mut stmt = self.conn.prepare(
            "SELECT revision, workspace_id, kind, cleaned_file_count, file_count, symbol_count,
                    relationship_count, identifier_count, type_count, created_at
             FROM canonical_revisions
             WHERE workspace_id = ?1
             ORDER BY revision DESC
             LIMIT 1",
        )?;

        let mut rows = stmt.query(params![workspace_id])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };

        let kind: String = row.get(2)?;
        let kind = CanonicalRevisionKind::from_str(&kind)
            .ok_or_else(|| anyhow!("Unknown canonical revision kind: {}", kind))?;

        Ok(Some(CanonicalRevision {
            revision: row.get(0)?,
            workspace_id: row.get(1)?,
            kind,
            cleaned_file_count: row.get(3)?,
            file_count: row.get(4)?,
            symbol_count: row.get(5)?,
            relationship_count: row.get(6)?,
            identifier_count: row.get(7)?,
            type_count: row.get(8)?,
            created_at: row.get(9)?,
        }))
    }

    pub fn get_current_canonical_revision(&self, workspace_id: &str) -> Result<Option<i64>> {
        Ok(self
            .get_latest_canonical_revision(workspace_id)?
            .map(|revision| revision.revision))
    }

    pub fn ensure_canonical_revision(
        &mut self,
        workspace_id: &str,
    ) -> Result<Option<CanonicalRevision>> {
        if let Some(existing) = self.get_latest_canonical_revision(workspace_id)? {
            return Ok(Some(existing));
        }

        let file_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))?;
        let symbol_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))?;
        let relationship_count: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM relationships", [], |row| row.get(0))?;
        let identifier_count: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM identifiers", [], |row| row.get(0))?;
        let type_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM types", [], |row| row.get(0))?;

        if file_count == 0
            && symbol_count == 0
            && relationship_count == 0
            && identifier_count == 0
            && type_count == 0
        {
            return Ok(None);
        }

        let tx = self.conn.transaction()?;
        record_canonical_revision_tx(
            &tx,
            workspace_id,
            CanonicalRevisionKind::Fresh,
            0,
            file_count,
            symbol_count,
            relationship_count,
            identifier_count,
            type_count,
        )?;
        tx.commit()?;

        self.get_latest_canonical_revision(workspace_id)
    }

    pub fn count_projection_source_docs(&self) -> Result<i64> {
        let file_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))?;
        let symbol_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))?;
        Ok(file_count + symbol_count)
    }
}
