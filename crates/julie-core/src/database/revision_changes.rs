use super::*;
use anyhow::Result;
use rusqlite::{OptionalExtension, Transaction, params};
use std::collections::HashMap;
use tracing::debug;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevisionChangeKind {
    Added,
    Modified,
    Deleted,
}

impl RevisionChangeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Modified => "modified",
            Self::Deleted => "deleted",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "added" => Some(Self::Added),
            "modified" => Some(Self::Modified),
            "deleted" => Some(Self::Deleted),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevisionFileChange {
    pub revision: i64,
    pub workspace_id: String,
    pub file_path: String,
    pub change_kind: RevisionChangeKind,
    pub old_hash: Option<String>,
    pub new_hash: Option<String>,
}

pub(crate) fn snapshot_file_hashes_tx(
    tx: &Transaction<'_>,
    file_paths: &[String],
) -> Result<HashMap<String, String>> {
    let mut hashes = HashMap::new();
    for file_path in file_paths {
        let hash = tx
            .query_row(
                "SELECT hash FROM files WHERE path = ?1",
                params![file_path],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if let Some(hash) = hash {
            hashes.insert(file_path.clone(), hash);
        }
    }
    Ok(hashes)
}

pub(crate) fn record_revision_file_changes_tx(
    tx: &Transaction<'_>,
    revision: i64,
    workspace_id: &str,
    changes: &[RevisionFileChange],
) -> Result<()> {
    if changes.is_empty() {
        return Ok(());
    }

    let mut stmt = tx.prepare(
        "INSERT OR REPLACE INTO revision_file_changes
         (revision, workspace_id, file_path, change_kind, old_hash, new_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;

    for change in changes {
        stmt.execute(params![
            revision,
            workspace_id,
            change.file_path,
            change.change_kind.as_str(),
            change.old_hash,
            change.new_hash
        ])?;
    }

    Ok(())
}

impl SymbolDatabase {
    pub(crate) fn create_revision_file_changes_table(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS revision_file_changes (
                revision INTEGER NOT NULL,
                workspace_id TEXT NOT NULL,
                file_path TEXT NOT NULL,
                change_kind TEXT NOT NULL CHECK(change_kind IN ('added', 'modified', 'deleted')),
                old_hash TEXT,
                new_hash TEXT,
                PRIMARY KEY (revision, workspace_id, file_path)
            )",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_revision_file_changes_workspace_revision
             ON revision_file_changes(workspace_id, revision)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_revision_file_changes_workspace_path
             ON revision_file_changes(workspace_id, file_path)",
            [],
        )?;

        debug!("Created revision_file_changes table and indexes");
        Ok(())
    }

    pub fn get_revision_file_changes_between(
        &self,
        workspace_id: &str,
        from_revision: i64,
        to_revision: i64,
    ) -> Result<Vec<RevisionFileChange>> {
        let mut stmt = self.conn.prepare(
            "SELECT revision, workspace_id, file_path, change_kind, old_hash, new_hash
             FROM revision_file_changes
             WHERE workspace_id = ?1
               AND revision > ?2
               AND revision <= ?3
             ORDER BY revision, file_path",
        )?;

        let rows = stmt.query_map(params![workspace_id, from_revision, to_revision], |row| {
            let kind: String = row.get(3)?;
            let change_kind = RevisionChangeKind::from_str(&kind).ok_or_else(|| {
                rusqlite::Error::FromSqlConversionFailure(
                    3,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("Unknown revision change kind: {}", kind),
                    )),
                )
            })?;
            Ok(RevisionFileChange {
                revision: row.get(0)?,
                workspace_id: row.get(1)?,
                file_path: row.get(2)?,
                change_kind,
                old_hash: row.get(4)?,
                new_hash: row.get(5)?,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}
