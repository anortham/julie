use anyhow::Result;
use rusqlite::{Connection, params};
use tracing::info;

pub(super) fn run_migrations(conn: &mut Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version    INTEGER PRIMARY KEY,
            applied_at INTEGER NOT NULL
        );",
    )?;

    let current: i32 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_version",
        [],
        |row| row.get(0),
    )?;

    if current < 1 {
        migration_001_initial_schema(conn)?;
    }
    if current < 2 {
        migration_002_add_index_duration(conn)?;
    }
    if current < 3 {
        migration_003_cleanup_events_and_drop_workspace_references(conn)?;
    }
    if current < 4 {
        migration_004_retire_search_compare_tables(conn)?;
    }
    if current < 5 {
        migration_005_add_tool_call_input_bytes(conn)?;
    }
    if current < 6 {
        migration_006_add_daemon_state(conn)?;
    }
    if current < 7 {
        migration_007_drop_search_compare_tables(conn)?;
    }

    Ok(())
}

fn migration_001_initial_schema(conn: &mut Connection) -> Result<()> {
    info!("registry.db migration 001: initial schema");
    let tx = conn.transaction()?;

    tx.execute_batch(
        "CREATE TABLE workspaces (
            workspace_id    TEXT PRIMARY KEY,
            path            TEXT NOT NULL UNIQUE,
            status          TEXT NOT NULL DEFAULT 'pending',
            session_count   INTEGER NOT NULL DEFAULT 0,
            last_indexed    INTEGER,
            symbol_count    INTEGER,
            file_count      INTEGER,
            embedding_model TEXT,
            vector_count    INTEGER,
            created_at      INTEGER NOT NULL,
            updated_at      INTEGER NOT NULL
        );

        CREATE TABLE codehealth_snapshots (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            workspace_id    TEXT NOT NULL REFERENCES workspaces(workspace_id) ON DELETE CASCADE,
            timestamp       INTEGER NOT NULL,
            total_symbols   INTEGER NOT NULL,
            total_files     INTEGER NOT NULL,
            security_high   INTEGER NOT NULL DEFAULT 0,
            security_medium INTEGER NOT NULL DEFAULT 0,
            security_low    INTEGER NOT NULL DEFAULT 0,
            change_high     INTEGER NOT NULL DEFAULT 0,
            change_medium   INTEGER NOT NULL DEFAULT 0,
            change_low      INTEGER NOT NULL DEFAULT 0,
            symbols_tested    INTEGER NOT NULL DEFAULT 0,
            symbols_untested  INTEGER NOT NULL DEFAULT 0,
            avg_centrality  REAL,
            max_centrality  REAL
        );
        CREATE INDEX idx_snapshots_workspace_time
            ON codehealth_snapshots(workspace_id, timestamp);

        CREATE TABLE tool_calls (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            workspace_id  TEXT NOT NULL,
            session_id    TEXT NOT NULL,
            timestamp     INTEGER NOT NULL,
            tool_name     TEXT NOT NULL,
            duration_ms   REAL NOT NULL,
            result_count  INTEGER,
            source_bytes  INTEGER,
            input_bytes   INTEGER,
            output_bytes  INTEGER,
            success       INTEGER NOT NULL DEFAULT 1,
            metadata      TEXT
        );
        CREATE INDEX idx_tool_calls_timestamp ON tool_calls(timestamp);
        CREATE INDEX idx_tool_calls_tool_name  ON tool_calls(tool_name);
        CREATE INDEX idx_tool_calls_session    ON tool_calls(session_id);
        CREATE INDEX idx_tool_calls_workspace  ON tool_calls(workspace_id);

        INSERT INTO schema_version (version, applied_at)
        VALUES (1, unixepoch());",
    )?;

    tx.commit()?;
    info!("registry.db migration 001 complete");
    Ok(())
}

fn migration_002_add_index_duration(conn: &mut Connection) -> Result<()> {
    info!("registry.db migration 002: add index duration column");
    let tx = conn.transaction()?;
    tx.execute_batch(
        "ALTER TABLE workspaces ADD COLUMN last_index_duration_ms INTEGER;
         INSERT OR REPLACE INTO schema_version (version, applied_at)
         VALUES (2, unixepoch());",
    )?;
    tx.commit()?;
    info!("registry.db migration 002 complete");
    Ok(())
}

fn migration_003_cleanup_events_and_drop_workspace_references(conn: &mut Connection) -> Result<()> {
    info!("registry.db migration 003: add cleanup-event log and drop workspace pairings");
    let tx = conn.transaction()?;
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS workspace_cleanup_events (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            workspace_id  TEXT NOT NULL,
            path          TEXT NOT NULL,
            action        TEXT NOT NULL,
            reason        TEXT NOT NULL,
            timestamp     INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_workspace_cleanup_events_timestamp
            ON workspace_cleanup_events(timestamp DESC, id DESC);
        DROP TABLE IF EXISTS workspace_references;
        INSERT OR REPLACE INTO schema_version (version, applied_at)
        VALUES (3, unixepoch());",
    )?;
    tx.commit()?;
    info!("registry.db migration 003 complete");
    Ok(())
}

fn migration_004_retire_search_compare_tables(conn: &mut Connection) -> Result<()> {
    info!("registry.db migration 004: retire search compare tables");
    let tx = conn.transaction()?;
    tx.execute_batch(
        "INSERT OR REPLACE INTO schema_version (version, applied_at)
        VALUES (4, unixepoch());",
    )?;
    tx.commit()?;
    info!("registry.db migration 004 complete");
    Ok(())
}

fn migration_005_add_tool_call_input_bytes(conn: &mut Connection) -> Result<()> {
    info!("registry.db migration 005: add input_bytes to tool_calls");
    let has_tool_calls = table_exists_in(conn, "tool_calls")?;
    let has_input_bytes: bool = {
        let mut stmt = conn.prepare("PRAGMA table_info(tool_calls)")?;
        let mut rows = stmt.query([])?;
        let mut found = false;
        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            if name == "input_bytes" {
                found = true;
                break;
            }
        }
        found
    };
    let tx = conn.transaction()?;
    if !has_tool_calls {
        tx.execute_batch(
            "CREATE TABLE tool_calls (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                workspace_id  TEXT NOT NULL,
                session_id    TEXT NOT NULL,
                timestamp     INTEGER NOT NULL,
                tool_name     TEXT NOT NULL,
                duration_ms   REAL NOT NULL,
                result_count  INTEGER,
                source_bytes  INTEGER,
                input_bytes   INTEGER,
                output_bytes  INTEGER,
                success       INTEGER NOT NULL DEFAULT 1,
                metadata      TEXT
            );",
        )?;
    } else if !has_input_bytes {
        tx.execute("ALTER TABLE tool_calls ADD COLUMN input_bytes INTEGER", [])?;
    }
    tx.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_tool_calls_timestamp ON tool_calls(timestamp);
         CREATE INDEX IF NOT EXISTS idx_tool_calls_tool_name  ON tool_calls(tool_name);
         CREATE INDEX IF NOT EXISTS idx_tool_calls_session    ON tool_calls(session_id);
         CREATE INDEX IF NOT EXISTS idx_tool_calls_workspace  ON tool_calls(workspace_id);",
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO schema_version (version, applied_at)
         VALUES (5, unixepoch())",
        [],
    )?;
    tx.commit()?;
    info!("registry.db migration 005 complete");
    Ok(())
}

fn migration_006_add_daemon_state(conn: &mut Connection) -> Result<()> {
    info!("registry.db migration 006: add daemon_state");
    let tx = conn.transaction()?;
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS daemon_state (
            id                 INTEGER PRIMARY KEY CHECK (id = 1),
            started_at_unix    INTEGER NOT NULL
        );",
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO daemon_state (id, started_at_unix)
         VALUES (1, unixepoch())",
        [],
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO schema_version (version, applied_at)
         VALUES (6, unixepoch())",
        [],
    )?;
    tx.commit()?;
    info!("registry.db migration 006 complete");
    Ok(())
}

fn migration_007_drop_search_compare_tables(conn: &mut Connection) -> Result<()> {
    info!("registry.db migration 007: drop retired search compare tables");
    let tx = conn.transaction()?;
    tx.execute_batch(
        "DROP TABLE IF EXISTS search_compare_cases;
         DROP TABLE IF EXISTS search_compare_runs;
         INSERT OR REPLACE INTO schema_version (version, applied_at)
         VALUES (7, unixepoch());",
    )?;
    tx.commit()?;
    info!("registry.db migration 007 complete");
    Ok(())
}

fn table_exists_in(conn: &Connection, table_name: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        params![table_name],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}
