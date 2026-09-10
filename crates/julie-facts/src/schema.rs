//! DDL for `facts.sqlite` (design section 6.1) and the `meta` table.

use anyhow::Result;
use rusqlite::{Connection, params};

use crate::version::{FACTS_SCHEMA_VERSION, SEMANTIC_INDEX_ENGINE_VERSION};

const SPAN_COLUMNS: &str = "start_line INTEGER NOT NULL,
    start_col INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    end_col INTEGER NOT NULL,
    start_byte INTEGER NOT NULL,
    end_byte INTEGER NOT NULL";

/// Every table and index, in creation order. `vectors` and `encoder` arrive
/// with semantics (Task 10); `test_verdicts` stays empty until CT lands.
pub fn create_schema(conn: &Connection) -> Result<()> {
    let ddl = format!(
        "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE blobs (
    hash TEXT PRIMARY KEY,
    language TEXT NOT NULL,
    extractor_version TEXT NOT NULL,
    byte_len INTEGER NOT NULL
);
CREATE TABLE paths (
    path TEXT PRIMARY KEY,
    blob_hash TEXT NOT NULL REFERENCES blobs(hash),
    language TEXT NOT NULL
);
CREATE INDEX paths_blob_hash ON paths(blob_hash);
CREATE TABLE symbols (
    blob_hash TEXT NOT NULL REFERENCES blobs(hash),
    ordinal INTEGER NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    {SPAN_COLUMNS},
    body_start_line INTEGER,
    body_start_col INTEGER,
    body_end_line INTEGER,
    body_end_col INTEGER,
    body_start_byte INTEGER,
    body_end_byte INTEGER,
    body_hash TEXT,
    signature TEXT,
    doc_comment TEXT,
    visibility TEXT,
    parent_ordinal INTEGER,
    annotations TEXT NOT NULL,
    metadata TEXT,
    semantic_group TEXT,
    confidence REAL,
    content_type TEXT,
    PRIMARY KEY (blob_hash, ordinal)
);
CREATE INDEX symbols_name ON symbols(name);
CREATE TABLE identifiers (
    blob_hash TEXT NOT NULL REFERENCES blobs(hash),
    ordinal INTEGER NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    {SPAN_COLUMNS},
    containing_ordinal INTEGER,
    receiver_type TEXT,
    code_context TEXT,
    confidence REAL NOT NULL,
    PRIMARY KEY (blob_hash, ordinal)
);
CREATE INDEX identifiers_name ON identifiers(name);
CREATE INDEX identifiers_blob_kind ON identifiers(blob_hash, kind);
CREATE TABLE relationships (
    blob_hash TEXT NOT NULL REFERENCES blobs(hash),
    ordinal INTEGER NOT NULL,
    from_ordinal INTEGER,
    to_name TEXT NOT NULL,
    to_blob_hash TEXT,
    to_ordinal INTEGER,
    kind TEXT NOT NULL,
    line_number INTEGER NOT NULL,
    span TEXT,
    reference_site_is_exact INTEGER NOT NULL,
    confidence REAL NOT NULL,
    metadata TEXT,
    PRIMARY KEY (blob_hash, ordinal)
);
CREATE TABLE types (
    blob_hash TEXT NOT NULL REFERENCES blobs(hash),
    symbol_ordinal INTEGER NOT NULL,
    resolved_type TEXT NOT NULL,
    generic_params TEXT,
    constraints TEXT,
    is_inferred INTEGER NOT NULL,
    PRIMARY KEY (blob_hash, symbol_ordinal)
);
CREATE TABLE source_regions (
    blob_hash TEXT NOT NULL REFERENCES blobs(hash),
    ordinal INTEGER NOT NULL,
    kind TEXT NOT NULL,
    containing_ordinal INTEGER,
    {SPAN_COLUMNS},
    metadata TEXT,
    PRIMARY KEY (blob_hash, ordinal)
);
CREATE TABLE structural_facts (
    blob_hash TEXT NOT NULL REFERENCES blobs(hash),
    ordinal INTEGER NOT NULL,
    pattern_id TEXT NOT NULL,
    capture_name TEXT NOT NULL,
    node_kind TEXT NOT NULL,
    containing_ordinal INTEGER,
    {SPAN_COLUMNS},
    confidence REAL NOT NULL,
    metadata TEXT,
    PRIMARY KEY (blob_hash, ordinal)
);
CREATE INDEX structural_facts_pattern ON structural_facts(pattern_id);
CREATE TABLE complexity_metrics (
    blob_hash TEXT NOT NULL REFERENCES blobs(hash),
    ordinal INTEGER NOT NULL,
    scope TEXT NOT NULL,
    symbol_ordinal INTEGER,
    algorithm_id TEXT NOT NULL,
    covered_lines INTEGER NOT NULL,
    covered_bytes INTEGER NOT NULL,
    decision_count INTEGER NOT NULL,
    loop_count INTEGER NOT NULL,
    max_nesting_depth INTEGER NOT NULL,
    parameter_count INTEGER,
    {SPAN_COLUMNS},
    metadata TEXT,
    PRIMARY KEY (blob_hash, ordinal)
);
CREATE TABLE literals (
    blob_hash TEXT NOT NULL REFERENCES blobs(hash),
    ordinal INTEGER NOT NULL,
    literal_text TEXT NOT NULL,
    kind TEXT NOT NULL,
    carrier TEXT,
    arg_position INTEGER NOT NULL,
    containing_ordinal INTEGER,
    {SPAN_COLUMNS},
    confidence REAL NOT NULL,
    PRIMARY KEY (blob_hash, ordinal)
);
CREATE TABLE type_arguments (
    blob_hash TEXT NOT NULL REFERENCES blobs(hash),
    ordinal INTEGER NOT NULL,
    identifier_ordinal INTEGER NOT NULL,
    parent_ordinal INTEGER,
    position INTEGER NOT NULL,
    type_name TEXT NOT NULL,
    PRIMARY KEY (blob_hash, ordinal)
);
CREATE TABLE diagnostics (
    blob_hash TEXT NOT NULL REFERENCES blobs(hash),
    ordinal INTEGER NOT NULL,
    kind TEXT NOT NULL,
    message TEXT,
    {SPAN_COLUMNS},
    PRIMARY KEY (blob_hash, ordinal)
);
CREATE TABLE test_verdicts (
    blob_hash TEXT NOT NULL REFERENCES blobs(hash),
    ordinal INTEGER NOT NULL,
    PRIMARY KEY (blob_hash, ordinal)
);"
    );
    conn.execute_batch(&ddl)?;
    conn.execute(
        "INSERT INTO meta (key, value) VALUES ('schema_version', ?1), ('engine_version', ?2)",
        params![
            FACTS_SCHEMA_VERSION.to_string(),
            SEMANTIC_INDEX_ENGINE_VERSION
        ],
    )?;
    Ok(())
}

/// The versions recorded in `meta`, or `None` when the file has no schema yet.
pub fn read_versions(conn: &Connection) -> Result<Option<(i32, String)>> {
    let has_tables: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'",
        [],
        |r| r.get(0),
    )?;
    if has_tables == 0 {
        return Ok(None);
    }
    let value = |key: &str| -> Result<Option<String>> {
        Ok(conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
            .ok())
    };
    let schema = value("schema_version")?
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(0);
    let engine = value("engine_version")?.unwrap_or_default();
    Ok(Some((schema, engine)))
}
