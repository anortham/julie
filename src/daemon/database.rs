//! Persistent daemon state: workspace registry, codehealth snapshots, tool call history.
//!
//! `DaemonDatabase` wraps a single SQLite connection to `~/.julie/daemon.db`.
//! It is shared across all sessions as `Arc<DaemonDatabase>`. The internal
//! `Mutex<Connection>` makes it safe to call from multiple tokio tasks.

use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use std::path::Path;
use tracing::{info, warn};

const DAEMON_SCHEMA_VERSION: i32 = 1;

/// Thread-safe daemon database. Shared across sessions as `Arc<DaemonDatabase>`.
///
/// Uses an internal `Mutex<Connection>` so callers don't need to lock externally.
/// This is the same pattern used by `SymbolDatabase`, which is held externally as
/// `Arc<Mutex<SymbolDatabase>>`. Here the lock is internal for ergonomics.
pub struct DaemonDatabase {
    conn: std::sync::Mutex<Connection>,
}

impl DaemonDatabase {
    /// Open (or create) the daemon database at `path`, running migrations as needed.
    ///
    /// If the database is corrupt, deletes and recreates it (corruption recovery).
    pub fn open(path: &Path) -> Result<Self> {
        let conn = match Connection::open(path) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to open daemon.db ({}), attempting recovery", e);
                if path.exists() {
                    std::fs::remove_file(path)?;
                }
                Connection::open(path)
                    .with_context(|| format!("Failed to create fresh daemon.db at {}", path.display()))?
            }
        };

        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA busy_timeout=5000;
             PRAGMA foreign_keys=ON;",
        )?;

        let db = Self {
            conn: std::sync::Mutex::new(conn),
        };

        {
            let mut conn = db.conn.lock().unwrap();
            Self::run_migrations(&mut conn)?;
        }

        Ok(db)
    }

    fn run_migrations(conn: &mut Connection) -> Result<()> {
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

        if current < DAEMON_SCHEMA_VERSION {
            Self::migration_001_initial_schema(conn)?;
        }

        Ok(())
    }

    fn migration_001_initial_schema(conn: &mut Connection) -> Result<()> {
        info!("daemon.db migration 001: initial schema");
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

            CREATE TABLE workspace_references (
                primary_workspace_id    TEXT NOT NULL REFERENCES workspaces(workspace_id) ON DELETE CASCADE,
                reference_workspace_id  TEXT NOT NULL REFERENCES workspaces(workspace_id) ON DELETE CASCADE,
                added_at                INTEGER NOT NULL,
                PRIMARY KEY (primary_workspace_id, reference_workspace_id)
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
        info!("daemon.db migration 001 complete");
        Ok(())
    }

    /// Returns true if a table with the given name exists in the database.
    pub fn table_exists(&self, table_name: &str) -> bool {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
            params![table_name],
            |row| row.get::<_, i32>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false)
    }
}
