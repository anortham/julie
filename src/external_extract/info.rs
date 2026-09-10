use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use rusqlite::Error as SqlError;
use serde::{Deserialize, Serialize};

use julie_facts::version::FACTS_SCHEMA_VERSION;
use julie_facts::{FactsStore, Opened};

use crate::external_extract::metadata::{
    ExternalExtractMetadata, REQUIRED_METADATA_KEYS, metadata_from_map,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalInfoSchemaState {
    Missing,
    Older,
    Current,
    Newer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalExtractCounts {
    pub files: u64,
    pub symbols: u64,
    pub relationships: u64,
    pub identifiers: u64,
    pub types: u64,
    pub type_arguments: u64,
    pub literals: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalExtractInfo {
    pub db: PathBuf,
    pub schema_version: Option<i32>,
    pub schema_state: ExternalInfoSchemaState,
    pub metadata: Option<ExternalExtractMetadata>,
    pub missing_metadata_keys: Vec<String>,
    pub latest_revision: Option<i64>,
    pub counts: ExternalExtractCounts,
}

pub fn read_external_extract_info(db_path: &Path) -> Result<ExternalExtractInfo> {
    if !db_path.exists() {
        return Ok(ExternalExtractInfo {
            db: db_path.to_path_buf(),
            schema_version: None,
            schema_state: ExternalInfoSchemaState::Missing,
            metadata: None,
            missing_metadata_keys: REQUIRED_METADATA_KEYS
                .iter()
                .map(|key| (*key).to_string())
                .collect(),
            latest_revision: None,
            counts: ExternalExtractCounts {
                files: 0,
                symbols: 0,
                relationships: 0,
                identifiers: 0,
                types: 0,
                type_arguments: 0,
                literals: 0,
            },
        });
    }

    match FactsStore::open(db_path)? {
        Opened::VersionMismatch { found_schema, .. } if found_schema > FACTS_SCHEMA_VERSION => {
            return Err(anyhow!(
                "database schema version ({found_schema}) is newer than current binary ({FACTS_SCHEMA_VERSION})"
            ));
        }
        Opened::VersionMismatch { found_schema, .. } => {
            return Ok(facts_mismatch_info(db_path, found_schema));
        }
        Opened::Ready(store) => read_ready_info(db_path, &store),
    }
}

fn facts_mismatch_info(db_path: &Path, found_schema: i32) -> ExternalExtractInfo {
    let schema_state = if found_schema < FACTS_SCHEMA_VERSION {
        ExternalInfoSchemaState::Older
    } else {
        ExternalInfoSchemaState::Newer
    };
    ExternalExtractInfo {
        db: db_path.to_path_buf(),
        schema_version: Some(found_schema),
        schema_state,
        metadata: None,
        missing_metadata_keys: REQUIRED_METADATA_KEYS
            .iter()
            .map(|key| (*key).to_string())
            .collect(),
        latest_revision: None,
        counts: ExternalExtractCounts {
            files: 0,
            symbols: 0,
            relationships: 0,
            identifiers: 0,
            types: 0,
            type_arguments: 0,
            literals: 0,
        },
    }
}

fn read_ready_info(db_path: &Path, store: &FactsStore) -> Result<ExternalExtractInfo> {
    let reader = store.reader();
    let schema_version = meta_i32(store, "schema_version")?;
    let metadata_values = load_metadata_map(store)?;
    let missing_metadata_keys = missing_metadata_keys(Some(&metadata_values));
    let metadata = metadata_from_map(&metadata_values)?;
    let latest_revision = metadata
        .as_ref()
        .and_then(|metadata| metadata.analyzed_revision);
    Ok(ExternalExtractInfo {
        db: db_path.to_path_buf(),
        schema_version,
        schema_state: classify_schema(schema_version),
        metadata,
        missing_metadata_keys,
        latest_revision,
        counts: ExternalExtractCounts {
            files: count_table(store, "paths")?,
            symbols: reader.symbol_count().unwrap_or(0),
            relationships: count_table(store, "relationships")?,
            identifiers: count_table(store, "identifiers")?,
            types: count_table(store, "types")?,
            type_arguments: count_table(store, "type_arguments")?,
            literals: count_table(store, "literals")?,
        },
    })
}

fn classify_schema(schema_version: Option<i32>) -> ExternalInfoSchemaState {
    match schema_version {
        None => ExternalInfoSchemaState::Missing,
        Some(version) if version < FACTS_SCHEMA_VERSION => ExternalInfoSchemaState::Older,
        Some(version) if version > FACTS_SCHEMA_VERSION => ExternalInfoSchemaState::Newer,
        Some(_) => ExternalInfoSchemaState::Current,
    }
}

fn load_metadata_map(store: &FactsStore) -> Result<HashMap<String, String>> {
    let mut stmt = match store.conn().prepare("SELECT key, value FROM meta") {
        Ok(stmt) => stmt,
        Err(error) if is_missing_table(&error) => return Ok(HashMap::new()),
        Err(error) => return Err(error.into()),
    };
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

fn missing_metadata_keys(values: Option<&HashMap<String, String>>) -> Vec<String> {
    REQUIRED_METADATA_KEYS
        .iter()
        .filter(|key| values.is_none_or(|values| !values.contains_key(**key)))
        .map(|key| (*key).to_string())
        .collect()
}

fn meta_i32(store: &FactsStore, key: &str) -> Result<Option<i32>> {
    let value: rusqlite::Result<String> =
        store
            .conn()
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| {
                row.get(0)
            });
    match value {
        Ok(text) => Ok(text.parse().ok()),
        Err(error) if is_missing_table(&error) => Ok(None),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn count_table(store: &FactsStore, table: &str) -> Result<u64> {
    assert!(
        table.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
        "count_table table name must be identifier-safe: {table:?}"
    );
    let sql = format!("SELECT COUNT(*) FROM {table}");
    match store.conn().query_row(&sql, [], |row| row.get::<_, i64>(0)) {
        Ok(count) => Ok(count.max(0) as u64),
        Err(error) if is_missing_table(&error) => Ok(0),
        Err(error) => Err(error.into()),
    }
}

fn is_missing_table(error: &SqlError) -> bool {
    matches!(
        error,
        SqlError::SqliteFailure(_, Some(message)) if message.contains("no such table")
    )
}
