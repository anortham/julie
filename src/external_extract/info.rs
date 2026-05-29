use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use rusqlite::{Connection, Error as SqlError, OpenFlags};
use serde::{Deserialize, Serialize};

use crate::database::LATEST_SCHEMA_VERSION;
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
    let conn = open_read_only(db_path)?;
    let schema_version = read_schema_version_from_conn(&conn)?;
    let schema_state = classify_schema(schema_version);
    if schema_state == ExternalInfoSchemaState::Newer {
        let version = schema_version.expect("newer schema has version");
        return Err(anyhow!(
            "database schema version ({version}) is newer than current binary ({LATEST_SCHEMA_VERSION})"
        ));
    }

    let metadata_values = read_metadata_values(&conn)?;
    let missing_metadata_keys = missing_metadata_keys(metadata_values.as_ref());
    let metadata = match metadata_values {
        Some(values) => metadata_from_map(&values)?,
        None => None,
    };
    let latest_revision = metadata
        .as_ref()
        .map(|metadata| read_latest_revision(&conn, &metadata.workspace_id))
        .transpose()?
        .flatten();

    Ok(ExternalExtractInfo {
        db: db_path.to_path_buf(),
        schema_version,
        schema_state,
        metadata,
        missing_metadata_keys,
        latest_revision,
        counts: ExternalExtractCounts {
            files: count_table(&conn, "files")?,
            symbols: count_table(&conn, "symbols")?,
            relationships: count_table(&conn, "relationships")?,
            identifiers: count_table(&conn, "identifiers")?,
            types: count_table(&conn, "types")?,
            type_arguments: count_table(&conn, "type_arguments")?,
        },
    })
}

pub(crate) fn read_schema_version_read_only(db_path: &Path) -> Result<Option<i32>> {
    let conn = open_read_only(db_path)?;
    read_schema_version_from_conn(&conn)
}

fn open_read_only(db_path: &Path) -> Result<Connection> {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY
        | OpenFlags::SQLITE_OPEN_URI
        | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    Connection::open_with_flags(db_path, flags)
        .map_err(|error| anyhow!("failed to open database read-only: {error}"))
}

fn read_schema_version_from_conn(conn: &Connection) -> Result<Option<i32>> {
    optional_query(conn, "SELECT MAX(version) FROM schema_version", [])
}

fn read_metadata_values(conn: &Connection) -> Result<Option<HashMap<String, String>>> {
    let mut stmt = match conn.prepare("SELECT key, value FROM external_extract_metadata") {
        Ok(stmt) => stmt,
        Err(error) if is_missing_table(&error) => return Ok(None),
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
    Ok(Some(values))
}

fn missing_metadata_keys(values: Option<&HashMap<String, String>>) -> Vec<String> {
    REQUIRED_METADATA_KEYS
        .iter()
        .filter(|key| values.is_none_or(|values| !values.contains_key(**key)))
        .map(|key| (*key).to_string())
        .collect()
}

fn classify_schema(schema_version: Option<i32>) -> ExternalInfoSchemaState {
    match schema_version {
        None => ExternalInfoSchemaState::Missing,
        Some(version) if version < LATEST_SCHEMA_VERSION => ExternalInfoSchemaState::Older,
        Some(version) if version > LATEST_SCHEMA_VERSION => ExternalInfoSchemaState::Newer,
        Some(_) => ExternalInfoSchemaState::Current,
    }
}

fn count_table(conn: &Connection, table: &str) -> Result<u64> {
    assert!(
        table.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
        "count_table table name must be identifier-safe: {table:?}"
    );
    let sql = format!("SELECT COUNT(*) FROM {table}");
    let count = optional_query::<i64, _>(conn, &sql, [])?.unwrap_or(0);
    Ok(count.try_into()?)
}

fn read_latest_revision(conn: &Connection, workspace_id: &str) -> Result<Option<i64>> {
    optional_query(
        conn,
        "SELECT MAX(revision) FROM canonical_revisions WHERE workspace_id = ?1",
        [workspace_id],
    )
}

fn optional_query<T, P>(conn: &Connection, sql: &str, params: P) -> Result<Option<T>>
where
    T: rusqlite::types::FromSql,
    P: rusqlite::Params,
{
    match conn.query_row(sql, params, |row| row.get::<_, Option<T>>(0)) {
        Ok(value) => Ok(value),
        Err(error) if is_missing_table(&error) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn is_missing_table(error: &SqlError) -> bool {
    matches!(
        error,
        SqlError::SqliteFailure(_, Some(message)) if message.contains("no such table")
    )
}
