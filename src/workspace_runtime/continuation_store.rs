//! src/workspace_runtime/continuation_store.rs
//! Dedicated SQLite backing store `<index_root>/continuations.db`.

use rusqlite::{Connection, params};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static ACCESS_COUNTER: AtomicU64 = AtomicU64::new(1);

fn next_access_time() -> i64 {
    let base = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as i64;
    base.saturating_add(ACCESS_COUNTER.fetch_add(1, Ordering::Relaxed) as i64)
}

use super::continuation::{
    ContinuationBinding, ContinuationError, ContinuationFailure, DEFAULT_CONTINUATION_TTL_SECS,
    MAX_SNAPSHOT_SIZE_BYTES, MAX_WORKSPACE_CONTINUATION_BUDGET_BYTES, generate_continuation_token,
    is_valid_continuation_token, validate_handle_binding,
};
use julie_context::{SpilloverFormat, SpilloverPage};

pub struct ContinuationStore {
    index_root: PathBuf,
    db_path: PathBuf,
    conn: Mutex<Connection>,
    max_budget_bytes: usize,
    default_ttl: Duration,
    max_snapshot_bytes: usize,
}

impl ContinuationStore {
    /// Opens or creates `<index_root>/continuations.db` with 0o600 permissions.
    pub fn open(index_root: &Path) -> Result<Self, ContinuationError> {
        Self::with_max_snapshot(
            index_root,
            MAX_WORKSPACE_CONTINUATION_BUDGET_BYTES,
            Duration::from_secs(DEFAULT_CONTINUATION_TTL_SECS),
            MAX_SNAPSHOT_SIZE_BYTES,
        )
    }

    /// Configurable constructor for custom budgets and limits.
    pub fn with_max_snapshot(
        root_or_file: &Path,
        max_budget_bytes: usize,
        default_ttl: Duration,
        max_snapshot_bytes: usize,
    ) -> Result<Self, ContinuationError> {
        let (index_root, db_path) = if root_or_file.extension().is_some_and(|ext| ext == "db") {
            let parent = root_or_file
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf();
            (parent, root_or_file.to_path_buf())
        } else {
            (
                root_or_file.to_path_buf(),
                root_or_file.join("continuations.db"),
            )
        };

        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(&db_path)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&db_path, std::fs::Permissions::from_mode(0o600));
        }

        conn.execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;
             PRAGMA busy_timeout = 5000; PRAGMA foreign_keys = ON;",
        )?;
        Self::init_schema(&conn)?;

        Ok(Self {
            index_root,
            db_path,
            conn: Mutex::new(conn),
            max_budget_bytes,
            default_ttl,
            max_snapshot_bytes,
        })
    }

    pub fn index_root(&self) -> &Path {
        &self.index_root
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    fn init_schema(conn: &Connection) -> Result<(), ContinuationError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS continuation_snapshots (
                snapshot_id TEXT PRIMARY KEY, workspace_id TEXT NOT NULL,
                tool TEXT NOT NULL, arguments_hash TEXT NOT NULL,
                generation INTEGER NOT NULL, source_hashes TEXT NOT NULL,
                title TEXT NOT NULL, format TEXT NOT NULL,
                total_pages INTEGER NOT NULL, total_rows INTEGER NOT NULL,
                size_bytes INTEGER NOT NULL, created_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL, last_accessed_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS continuation_pages (
                token TEXT PRIMARY KEY,
                snapshot_id TEXT NOT NULL REFERENCES continuation_snapshots(snapshot_id) ON DELETE CASCADE,
                page_index INTEGER NOT NULL, rows_json TEXT NOT NULL,
                next_token TEXT, size_bytes INTEGER NOT NULL,
                created_at INTEGER NOT NULL, expires_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS continuation_tombstones (
                token TEXT PRIMARY KEY, workspace_id TEXT NOT NULL,
                tool TEXT NOT NULL, reason TEXT NOT NULL,
                created_at INTEGER NOT NULL, expires_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_snapshots_ws_access ON continuation_snapshots(workspace_id, last_accessed_at);
            CREATE INDEX IF NOT EXISTS idx_snapshots_expires ON continuation_snapshots(expires_at);
            CREATE INDEX IF NOT EXISTS idx_tombstones_expires ON continuation_tombstones(expires_at);",
        )?;
        Ok(())
    }

    /// Stores pre-rendered rows into paginated continuation pages under budget constraints.
    pub fn create(
        &self,
        binding: ContinuationBinding,
        title: String,
        rows: Vec<String>,
        page_size: usize,
        format: SpilloverFormat,
        ttl: Option<Duration>,
    ) -> Result<Option<String>, ContinuationError> {
        if rows.is_empty() {
            return Ok(None);
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let ttl_secs = ttl.unwrap_or(self.default_ttl).as_secs();
        let expires_at = now + ttl_secs;

        let chunks: Vec<Vec<String>> = rows.chunks(page_size.max(1)).map(|c| c.to_vec()).collect();
        let total_pages = chunks.len();

        let mut total_size_bytes = 0usize;
        let mut page_payloads = Vec::with_capacity(total_pages);
        for chunk in &chunks {
            let json = serde_json::to_string(chunk).unwrap_or_default();
            let size = json.len() + 128;
            total_size_bytes += size;
            page_payloads.push((json, size));
        }

        if total_size_bytes > self.max_snapshot_bytes {
            return Err(ContinuationError::OversizedPayload {
                size: total_size_bytes,
                max: self.max_snapshot_bytes,
            });
        }

        let tokens: Vec<String> = (0..total_pages)
            .map(|_| generate_continuation_token())
            .collect();
        let first_token = tokens[0].clone();
        let snapshot_id = generate_continuation_token();

        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        Self::purge_expired_tx(&tx, now, ttl_secs)?;
        self.enforce_budget_tx(&tx, &binding.workspace_id, total_size_bytes, now, ttl_secs)?;

        tx.execute(
            "INSERT INTO continuation_snapshots (
                snapshot_id, workspace_id, tool, arguments_hash, generation, source_hashes,
                title, format, total_pages, total_rows, size_bytes, created_at, expires_at, last_accessed_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                snapshot_id, binding.workspace_id, binding.tool, binding.arguments_hash,
                binding.generation as i64, binding.source_hashes, title, format.as_str(),
                total_pages as i64, rows.len() as i64, total_size_bytes as i64,
                now as i64, expires_at as i64, next_access_time()
            ],
        )?;

        for (i, (json, size)) in page_payloads.into_iter().enumerate() {
            let next_token = if i + 1 < total_pages {
                Some(&tokens[i + 1])
            } else {
                None
            };
            tx.execute(
                "INSERT INTO continuation_pages (token, snapshot_id, page_index, rows_json, next_token, size_bytes, created_at, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![tokens[i], snapshot_id, i as i64, json, next_token, size as i64, now as i64, expires_at as i64],
            )?;
        }

        tx.commit()?;
        Ok(Some(first_token))
    }

    /// Stores raw byte payload under budget constraints (used by contract tests).
    pub fn create_raw(
        &self,
        binding: &ContinuationBinding,
        payload: &[u8],
    ) -> Result<String, ContinuationFailure> {
        let size = payload.len();
        if size > self.max_snapshot_bytes {
            return Err(ContinuationFailure::oversized(
                size,
                self.max_snapshot_bytes,
            ));
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let ttl_secs = self.default_ttl.as_secs();
        let expires_at = now + ttl_secs;
        let token = generate_continuation_token();
        let snapshot_id = generate_continuation_token();
        let json = serde_json::to_string(&vec![hex::encode(payload)]).unwrap_or_default();

        let mut conn = self.conn.lock().unwrap();
        let tx = conn
            .transaction()
            .map_err(|e| ContinuationFailure::invalid(e.to_string()))?;

        Self::purge_expired_tx(&tx, now, ttl_secs)
            .map_err(|e| ContinuationFailure::invalid(e.to_string()))?;
        self.enforce_budget_tx(&tx, &binding.workspace_id, size, now, ttl_secs)
            .map_err(|e| ContinuationFailure::invalid(e.to_string()))?;

        tx.execute(
            "INSERT INTO continuation_snapshots (
                snapshot_id, workspace_id, tool, arguments_hash, generation, source_hashes,
                title, format, total_pages, total_rows, size_bytes, created_at, expires_at, last_accessed_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, 1, ?9, ?10, ?11, ?12)",
            params![
                snapshot_id, binding.workspace_id, binding.tool, binding.arguments_hash,
                binding.generation as i64, binding.source_hashes, "Raw", "compact",
                size as i64, now as i64, expires_at as i64, next_access_time()
            ],
        ).map_err(|e| ContinuationFailure::invalid(e.to_string()))?;

        tx.execute(
            "INSERT INTO continuation_pages (token, snapshot_id, page_index, rows_json, next_token, size_bytes, created_at, expires_at)
             VALUES (?1, ?2, 0, ?3, NULL, ?4, ?5, ?6)",
            params![token, snapshot_id, json, size as i64, now as i64, expires_at as i64],
        ).map_err(|e| ContinuationFailure::invalid(e.to_string()))?;

        tx.commit()
            .map_err(|e| ContinuationFailure::invalid(e.to_string()))?;
        Ok(token)
    }

    /// Reads a pre-rendered continuation page, verifying binding and TTL.
    pub fn read(
        &self,
        token: &str,
        requested: &ContinuationBinding,
        format_override: Option<SpilloverFormat>,
    ) -> Result<SpilloverPage, ContinuationFailure> {
        if !is_valid_continuation_token(token) {
            return Err(ContinuationFailure::invalid(
                "Malformed continuation token: must be 64-character hexadecimal",
            ));
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn
            .prepare(
                "SELECT p.snapshot_id, p.rows_json, p.next_token, p.expires_at,
                    s.workspace_id, s.tool, s.arguments_hash, s.generation, s.source_hashes,
                    s.title, s.format
             FROM continuation_pages p
             JOIN continuation_snapshots s ON p.snapshot_id = s.snapshot_id
             WHERE p.token = ?1",
            )
            .map_err(|e| ContinuationFailure::invalid(e.to_string()))?;

        let mut rows = stmt
            .query(params![token])
            .map_err(|e| ContinuationFailure::invalid(e.to_string()))?;

        if let Some(row) = rows
            .next()
            .map_err(|e| ContinuationFailure::invalid(e.to_string()))?
        {
            let snapshot_id: String = row.get(0).unwrap();
            let rows_json: String = row.get(1).unwrap();
            let next_token: Option<String> = row.get(2).unwrap();
            let expires_at: i64 = row.get(3).unwrap();
            let gen_i64: i64 = row.get(7).unwrap();
            let stored_binding = ContinuationBinding {
                workspace_id: row.get(4).unwrap(),
                tool: row.get(5).unwrap(),
                arguments_hash: row.get(6).unwrap(),
                generation: gen_i64 as u64,
                source_hashes: row.get(8).unwrap(),
            };
            let title: String = row.get(9).unwrap();
            let format_str: String = row.get(10).unwrap();
            let stored_format =
                SpilloverFormat::parse_strict(&format_str).unwrap_or(SpilloverFormat::Compact);

            if (now as i64) >= expires_at {
                drop(rows);
                drop(stmt);
                let _ = conn.execute(
                    "INSERT OR REPLACE INTO continuation_tombstones (token, workspace_id, tool, reason, created_at, expires_at)
                     VALUES (?1, ?2, ?3, 'expired', ?4, ?5)",
                    params![token, stored_binding.workspace_id, stored_binding.tool, now as i64, (now + self.default_ttl.as_secs()) as i64],
                );
                let _ = conn.execute(
                    "DELETE FROM continuation_snapshots WHERE snapshot_id = ?1",
                    params![snapshot_id],
                );
                return Err(ContinuationFailure::expired(
                    "Continuation handle has expired",
                ));
            }

            validate_handle_binding(&stored_binding, requested)?;
            let _ = conn.execute(
                "UPDATE continuation_snapshots SET last_accessed_at = ?1 WHERE snapshot_id = ?2",
                params![next_access_time(), snapshot_id],
            );

            let page_rows: Vec<String> = serde_json::from_str(&rows_json).unwrap_or_default();
            return Ok(SpilloverPage {
                title,
                rows: page_rows,
                next_handle: next_token,
                format: format_override.unwrap_or(stored_format),
            });
        }

        let mut tomb_stmt = conn
            .prepare("SELECT reason, expires_at FROM continuation_tombstones WHERE token = ?1")
            .map_err(|e| ContinuationFailure::invalid(e.to_string()))?;
        let mut tomb_rows = tomb_stmt
            .query(params![token])
            .map_err(|e| ContinuationFailure::invalid(e.to_string()))?;
        if let Some(tomb_row) = tomb_rows
            .next()
            .map_err(|e| ContinuationFailure::invalid(e.to_string()))?
        {
            let reason: String = tomb_row.get(0).unwrap();
            let expires_at: i64 = tomb_row.get(1).unwrap();
            if (now as i64) < expires_at {
                return Err(ContinuationFailure::expired(format!(
                    "Continuation handle was {reason}: handle has expired"
                )));
            }
        }

        Err(ContinuationFailure::invalid(
            "Continuation handle not found",
        ))
    }

    /// Reads raw bytes stored via `create_raw`.
    pub fn read_raw(
        &self,
        token: &str,
        requested: &ContinuationBinding,
    ) -> Result<Vec<u8>, ContinuationFailure> {
        let page = self.read(token, requested, None)?;
        if let Some(first_row) = page.rows.first() {
            let hex_str: String =
                serde_json::from_str(first_row).unwrap_or_else(|_| first_row.clone());
            hex::decode(&hex_str).map_err(|e| ContinuationFailure::invalid(e.to_string()))
        } else {
            Ok(Vec::new())
        }
    }

    /// Total count of active continuation snapshot entries.
    pub fn entry_count(&self) -> Result<usize, ContinuationError> {
        let conn = self.conn.lock().unwrap();
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM continuation_snapshots", [], |r| {
                r.get(0)
            })?;
        Ok(count as usize)
    }

    fn purge_expired_tx(
        tx: &rusqlite::Transaction,
        now: u64,
        ttl_secs: u64,
    ) -> Result<(), rusqlite::Error> {
        tx.execute(
            "INSERT OR REPLACE INTO continuation_tombstones (token, workspace_id, tool, reason, created_at, expires_at)
             SELECT p.token, s.workspace_id, s.tool, 'expired', ?1, ?2
             FROM continuation_pages p
             JOIN continuation_snapshots s ON p.snapshot_id = s.snapshot_id
             WHERE s.expires_at <= ?1",
            params![now as i64, (now + ttl_secs) as i64],
        )?;
        tx.execute(
            "DELETE FROM continuation_snapshots WHERE expires_at <= ?1",
            params![now as i64],
        )?;
        tx.execute(
            "DELETE FROM continuation_tombstones WHERE expires_at <= ?1",
            params![now as i64],
        )?;
        Ok(())
    }

    fn enforce_budget_tx(
        &self,
        tx: &rusqlite::Transaction,
        workspace_id: &str,
        new_bytes: usize,
        now: u64,
        ttl_secs: u64,
    ) -> Result<(), rusqlite::Error> {
        loop {
            let current_bytes: i64 = tx.query_row(
                "SELECT COALESCE(SUM(size_bytes), 0) FROM continuation_snapshots WHERE workspace_id = ?1",
                params![workspace_id],
                |r| r.get(0),
            )?;
            if (current_bytes as usize) + new_bytes <= self.max_budget_bytes {
                break;
            }
            let candidate: Option<(String, String)> = tx.query_row(
                "SELECT snapshot_id, tool FROM continuation_snapshots WHERE workspace_id = ?1 ORDER BY last_accessed_at ASC LIMIT 1",
                params![workspace_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            ).ok();
            let Some((oldest_id, tool)) = candidate else {
                break;
            };
            tx.execute(
                "INSERT OR REPLACE INTO continuation_tombstones (token, workspace_id, tool, reason, created_at, expires_at)
                 SELECT token, ?2, ?3, 'evicted', ?4, ?5 FROM continuation_pages WHERE snapshot_id = ?1",
                params![oldest_id, workspace_id, tool, now as i64, (now + ttl_secs) as i64],
            )?;
            tx.execute(
                "DELETE FROM continuation_snapshots WHERE snapshot_id = ?1",
                params![oldest_id],
            )?;
        }
        Ok(())
    }
}
