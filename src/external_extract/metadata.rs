use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Result, anyhow};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use julie_facts::version::FACTS_SCHEMA_VERSION;
use julie_facts::{FactsStore, Opened};

/// Version of the `julie-server extract` output contract consumed by the Miller
/// bridge (`~/source/codesearch`), which gates ingestion with an exact-equality
/// check on this value. Bump ONLY in lockstep with a coordinated Miller-gate
/// update.
pub const EXTRACT_CONTRACT_VERSION: i32 = 3;

pub const EXTRACT_HASH_ALGORITHM: &str = "blake3";

pub const REQUIRED_METADATA_KEYS: [&str; 10] = [
    "julie_version",
    "sqlite_schema_version",
    "extract_contract_version",
    "hash_algorithm",
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
    pub hash_algorithm: String,
    pub workspace_id: String,
    pub root_path: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub analysis_state: String,
    pub analyzed_revision: Option<i64>,
}

pub fn open_facts_store(db_path: &Path, strict_schema: bool) -> Result<FactsStore> {
    validate_facts_schema_policy(db_path, strict_schema)?;
    match FactsStore::open(db_path)? {
        Opened::Ready(store) => Ok(store),
        Opened::VersionMismatch {
            found_schema,
            found_engine,
        } => Err(anyhow!(
            "facts.sqlite version mismatch: schema {found_schema}, engine {found_engine}"
        )),
    }
}

pub fn validate_facts_schema_policy(db_path: &Path, strict_schema: bool) -> Result<()> {
    if !db_path.exists() {
        return Ok(());
    }
    match FactsStore::open(db_path)? {
        Opened::Ready(_) => Ok(()),
        Opened::VersionMismatch {
            found_schema,
            found_engine,
        } => {
            if strict_schema {
                Err(anyhow!(
                    "facts.sqlite version mismatch: schema {found_schema}, engine {found_engine}"
                ))
            } else {
                Err(anyhow!(
                    "facts.sqlite version mismatch: schema {found_schema}, engine {found_engine}; delete the file and rerun extract"
                ))
            }
        }
    }
}

pub fn ensure_external_extract_metadata(
    store: &FactsStore,
    root_path: &Path,
    requested_workspace_id: Option<&str>,
) -> Result<ExternalExtractMetadata> {
    ensure_external_extract_metadata_with_root_policy(
        store,
        root_path,
        requested_workspace_id,
        false,
    )
}

pub fn ensure_external_extract_metadata_with_root_policy(
    store: &FactsStore,
    root_path: &Path,
    requested_workspace_id: Option<&str>,
    allow_root_rebuild: bool,
) -> Result<ExternalExtractMetadata> {
    let now = unix_timestamp()?;
    let existing = load_metadata_map(store)?;
    let normalized_root_path = normalized_root_path(root_path);
    let workspace_id = match (existing.get("workspace_id"), requested_workspace_id) {
        (Some(existing), Some(requested)) if existing != requested && !allow_root_rebuild => {
            return Err(anyhow!(
                "workspace id mismatch: database has '{existing}', requested '{requested}'; rerun extract scan --force to rebuild for the requested workspace id"
            ));
        }
        (Some(_), Some(requested)) => requested.to_string(),
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
        sqlite_schema_version: FACTS_SCHEMA_VERSION,
        extract_contract_version: EXTRACT_CONTRACT_VERSION,
        hash_algorithm: EXTRACT_HASH_ALGORITHM.to_string(),
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

    write_metadata(store, &metadata)?;
    Ok(metadata)
}

pub fn load_external_extract_metadata(
    store: &FactsStore,
) -> Result<Option<ExternalExtractMetadata>> {
    metadata_from_map(&load_metadata_map(store)?)
}

pub fn mark_external_extract_analysis_stale(store: &FactsStore) -> Result<()> {
    let now = unix_timestamp()?;
    store.conn().execute(
        "INSERT INTO meta (key, value) VALUES ('analysis_state', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        ["stale"],
    )?;
    store.conn().execute(
        "INSERT INTO meta (key, value) VALUES ('updated_at', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [now.to_string()],
    )?;
    Ok(())
}

pub fn mark_external_extract_analysis_current(
    store: &FactsStore,
    analyzed_revision: Option<i64>,
) -> Result<()> {
    let now = unix_timestamp()?;
    store.conn().execute(
        "INSERT INTO meta (key, value) VALUES ('analysis_state', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        ["current"],
    )?;
    store.conn().execute(
        "INSERT INTO meta (key, value) VALUES ('analyzed_revision', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [analyzed_revision
            .map(|revision| revision.to_string())
            .unwrap_or_default()],
    )?;
    store.conn().execute(
        "INSERT INTO meta (key, value) VALUES ('updated_at', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [now.to_string()],
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
        hash_algorithm: values["hash_algorithm"].clone(),
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

fn load_metadata_map(store: &FactsStore) -> Result<HashMap<String, String>> {
    let mut stmt = store.conn().prepare("SELECT key, value FROM meta")?;
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

fn write_metadata(store: &FactsStore, metadata: &ExternalExtractMetadata) -> Result<()> {
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
        ("hash_algorithm", metadata.hash_algorithm.clone()),
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
        store.conn().execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
    }
    Ok(())
}

fn parse_required_i32(values: &HashMap<String, String>, key: &str) -> Result<i32> {
    values
        .get(key)
        .ok_or_else(|| anyhow!("missing metadata key {key}"))?
        .parse()
        .map_err(|error| anyhow!("invalid {key}: {error}"))
}

fn parse_required_i64(values: &HashMap<String, String>, key: &str) -> Result<i64> {
    values
        .get(key)
        .ok_or_else(|| anyhow!("missing metadata key {key}"))?
        .parse()
        .map_err(|error| anyhow!("invalid {key}: {error}"))
}

fn normalized_root_path(root: &Path) -> String {
    root.to_string_lossy().replace('\\', "/")
}

fn unix_timestamp() -> Result<i64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| anyhow!("clock error: {error}"))?
        .as_secs() as i64)
}
