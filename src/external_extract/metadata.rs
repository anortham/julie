use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Result, anyhow};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::database::{LATEST_SCHEMA_VERSION, SymbolDatabase};
use crate::external_extract::lock::ExternalExtractOperationLock;

pub const EXTRACT_CONTRACT_VERSION: i32 = 1;

pub const REQUIRED_METADATA_KEYS: [&str; 9] = [
    "julie_version",
    "sqlite_schema_version",
    "extract_contract_version",
    "workspace_id",
    "root_path",
    "created_at",
    "updated_at",
    "analysis_state",
    "analyzed_revision",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalExtractMetadata {
    pub julie_version: String,
    pub sqlite_schema_version: i32,
    pub extract_contract_version: i32,
    pub workspace_id: String,
    pub root_path: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub analysis_state: String,
    pub analyzed_revision: Option<i64>,
}

pub fn open_external_extract_database<P: AsRef<Path>>(
    db_path: P,
    strict_schema: bool,
) -> Result<SymbolDatabase> {
    validate_external_extract_schema_policy(db_path.as_ref(), strict_schema)?;
    SymbolDatabase::new(db_path)
}

pub struct ExternalExtractDatabaseOperation {
    db: SymbolDatabase,
    _lock: ExternalExtractOperationLock,
}

impl ExternalExtractDatabaseOperation {
    pub fn db(&self) -> &SymbolDatabase {
        &self.db
    }

    pub fn db_mut(&mut self) -> &mut SymbolDatabase {
        &mut self.db
    }
}

pub fn open_external_extract_database_for_operation<P: AsRef<Path>>(
    db_path: P,
    strict_schema: bool,
) -> Result<ExternalExtractDatabaseOperation> {
    let lock = ExternalExtractOperationLock::acquire(db_path.as_ref())?;
    validate_external_extract_schema_policy(db_path.as_ref(), strict_schema)?;
    let db = SymbolDatabase::new(db_path)?;
    Ok(ExternalExtractDatabaseOperation { db, _lock: lock })
}

pub fn validate_external_extract_schema_policy(db_path: &Path, strict_schema: bool) -> Result<()> {
    if !db_path.exists() {
        return Ok(());
    }

    let schema_version =
        crate::external_extract::info::read_schema_version_read_only(db_path)?.unwrap_or(0);

    if schema_version > LATEST_SCHEMA_VERSION {
        return Err(anyhow!(
            "database schema version ({schema_version}) is newer than current binary ({LATEST_SCHEMA_VERSION})"
        ));
    }

    if strict_schema && schema_version < LATEST_SCHEMA_VERSION {
        return Err(anyhow!(
            "database schema version ({schema_version}) is older than current binary ({LATEST_SCHEMA_VERSION}); rerun without --strict-schema to migrate"
        ));
    }

    Ok(())
}

pub fn ensure_external_extract_metadata(
    db: &SymbolDatabase,
    root_path: &Path,
    requested_workspace_id: Option<&str>,
) -> Result<ExternalExtractMetadata> {
    ensure_external_extract_metadata_with_root_policy(db, root_path, requested_workspace_id, false)
}

pub fn ensure_external_extract_metadata_with_root_policy(
    db: &SymbolDatabase,
    root_path: &Path,
    requested_workspace_id: Option<&str>,
    allow_root_rebuild: bool,
) -> Result<ExternalExtractMetadata> {
    let now = unix_timestamp()?;
    let existing = load_metadata_map(db)?;
    let normalized_root_path = normalized_root_path(root_path);
    let workspace_id = match (existing.get("workspace_id"), requested_workspace_id) {
        (Some(existing), Some(requested)) if existing != requested => {
            return Err(anyhow!(
                "workspace id mismatch: database has '{existing}', requested '{requested}'"
            ));
        }
        (Some(existing), _) => existing.clone(),
        (None, Some(requested)) => requested.to_string(),
        (None, None) => Uuid::new_v4().to_string(),
    };

    if let Some(existing_root) = existing.get("root_path")
        && existing_root != &normalized_root_path
        && !allow_root_rebuild
    {
        return Err(anyhow!(
            "root path mismatch: database has '{existing_root}', requested '{normalized_root_path}'; rerun extract scan --force to rebuild for the new root"
        ));
    }

    let created_at = existing
        .get("created_at")
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(now);
    let analyzed_revision = existing
        .get("analyzed_revision")
        .and_then(|value| value.parse::<i64>().ok());

    let metadata = ExternalExtractMetadata {
        julie_version: env!("CARGO_PKG_VERSION").to_string(),
        sqlite_schema_version: db.get_schema_version()?,
        extract_contract_version: EXTRACT_CONTRACT_VERSION,
        workspace_id,
        root_path: normalized_root_path,
        created_at,
        updated_at: now,
        analysis_state: existing
            .get("analysis_state")
            .cloned()
            .unwrap_or_else(|| "pending".to_string()),
        analyzed_revision,
    };

    write_metadata(db, &metadata)?;
    Ok(metadata)
}

pub fn load_external_extract_metadata(
    db: &SymbolDatabase,
) -> Result<Option<ExternalExtractMetadata>> {
    metadata_from_map(&load_metadata_map(db)?)
}

pub fn mark_external_extract_analysis_stale(db: &SymbolDatabase) -> Result<()> {
    let now = unix_timestamp()?;
    let tx = db.conn.unchecked_transaction()?;
    write_analysis_metadata_tx(&tx, "stale", None, now)?;
    tx.commit()?;
    Ok(())
}

pub fn mark_external_extract_analysis_current(
    db: &SymbolDatabase,
    analyzed_revision: Option<i64>,
) -> Result<()> {
    let now = unix_timestamp()?;
    let tx = db.conn.unchecked_transaction()?;
    write_analysis_metadata_tx(&tx, "current", analyzed_revision, now)?;
    tx.commit()?;
    Ok(())
}

fn write_analysis_metadata_tx(
    tx: &rusqlite::Transaction<'_>,
    analysis_state: &str,
    analyzed_revision: Option<i64>,
    now: i64,
) -> Result<()> {
    tx.execute(
        "INSERT INTO external_extract_metadata (key, value, updated_at)
         VALUES ('analysis_state', ?1, ?2)
         ON CONFLICT(key) DO UPDATE SET
            value = excluded.value,
            updated_at = excluded.updated_at",
        params![analysis_state, now],
    )?;
    tx.execute(
        "INSERT INTO external_extract_metadata (key, value, updated_at)
         VALUES ('analyzed_revision', ?1, ?2)
         ON CONFLICT(key) DO UPDATE SET
            value = excluded.value,
            updated_at = excluded.updated_at",
        params![
            analyzed_revision
                .map(|revision| revision.to_string())
                .unwrap_or_default(),
            now
        ],
    )?;
    tx.execute(
        "UPDATE external_extract_metadata
         SET value = ?1, updated_at = ?1
         WHERE key = 'updated_at'",
        params![now],
    )?;
    Ok(())
}

pub(crate) fn metadata_from_map(
    values: &HashMap<String, String>,
) -> Result<Option<ExternalExtractMetadata>> {
    if REQUIRED_METADATA_KEYS
        .iter()
        .any(|key| !values.contains_key(*key))
    {
        return Ok(None);
    }

    Ok(Some(ExternalExtractMetadata {
        julie_version: values["julie_version"].clone(),
        sqlite_schema_version: parse_required_i32(values, "sqlite_schema_version")?,
        extract_contract_version: parse_required_i32(values, "extract_contract_version")?,
        workspace_id: values["workspace_id"].clone(),
        root_path: values["root_path"].clone(),
        created_at: parse_required_i64(values, "created_at")?,
        updated_at: parse_required_i64(values, "updated_at")?,
        analysis_state: values["analysis_state"].clone(),
        analyzed_revision: values
            .get("analyzed_revision")
            .filter(|value| !value.is_empty())
            .map(|value| value.parse::<i64>())
            .transpose()?,
    }))
}

fn load_metadata_map(db: &SymbolDatabase) -> Result<HashMap<String, String>> {
    let mut stmt = db
        .conn
        .prepare("SELECT key, value FROM external_extract_metadata")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;

    let mut values = HashMap::new();
    for row in rows {
        let (key, value) = row?;
        values.insert(key, value);
    }
    Ok(values)
}

fn write_metadata(db: &SymbolDatabase, metadata: &ExternalExtractMetadata) -> Result<()> {
    for (key, value) in [
        ("julie_version", metadata.julie_version.clone()),
        (
            "sqlite_schema_version",
            metadata.sqlite_schema_version.to_string(),
        ),
        (
            "extract_contract_version",
            metadata.extract_contract_version.to_string(),
        ),
        ("workspace_id", metadata.workspace_id.clone()),
        ("root_path", metadata.root_path.clone()),
        ("created_at", metadata.created_at.to_string()),
        ("updated_at", metadata.updated_at.to_string()),
        ("analysis_state", metadata.analysis_state.clone()),
        (
            "analyzed_revision",
            metadata
                .analyzed_revision
                .map(|revision| revision.to_string())
                .unwrap_or_default(),
        ),
    ] {
        db.conn.execute(
            "INSERT INTO external_extract_metadata (key, value, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                updated_at = excluded.updated_at",
            params![key, value, metadata.updated_at],
        )?;
    }

    Ok(())
}

fn normalized_root_path(root_path: &Path) -> String {
    root_path
        .canonicalize()
        .unwrap_or_else(|_| root_path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

fn parse_required_i32(values: &HashMap<String, String>, key: &str) -> Result<i32> {
    values
        .get(key)
        .ok_or_else(|| anyhow!("missing external extract metadata key '{key}'"))?
        .parse::<i32>()
        .map_err(|error| anyhow!("invalid external extract metadata key '{key}': {error}"))
}

fn parse_required_i64(values: &HashMap<String, String>, key: &str) -> Result<i64> {
    values
        .get(key)
        .ok_or_else(|| anyhow!("missing external extract metadata key '{key}'"))?
        .parse::<i64>()
        .map_err(|error| anyhow!("invalid external extract metadata key '{key}': {error}"))
}

fn unix_timestamp() -> Result<i64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_secs()
        .try_into()?)
}
