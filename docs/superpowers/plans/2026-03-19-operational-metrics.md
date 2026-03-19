# Operational Metrics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add per-tool-call instrumentation, SQLite storage, and query_metrics category expansion so users can see how Julie is performing and how much context it's saving them.

**Architecture:** Thin timing wrapper in handler.rs around each tool dispatch. Each tool builds a `ToolCallReport` with result-aware metadata. In-memory `Arc<SessionMetrics>` with atomic counters for session stats. Per-call records written to SQLite `tool_calls` table via `tokio::spawn_blocking`. `query_metrics` gains `category` param for session/history views.

**Tech Stack:** Rust, SQLite (rusqlite), tokio, serde_json, std::sync::atomic, uuid

**Spec:** `docs/superpowers/specs/2026-03-19-operational-metrics-design.md`

**Critical implementation notes (from plan review):**
1. `Content.as_text()` returns `Option<&RawTextContent>`, so output_bytes extraction is: `result.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.len() as u64).sum()`
2. `SymbolDatabase` field is `pub(crate) conn`, NOT a method. Use `db.conn` not `db.conn()` in tests.
3. `FileInfo` is constructed in many places beyond `files.rs` (indexing processor, watcher, tests). Adding `line_count` requires updating ALL construction sites. Use `grep -rn "FileInfo {" src/` to find them all.
4. The `record_tool_call` async SQLite write must properly await the `RwLock` read guard. Pattern: `let guard = workspace.read().await; if let Some(ws) = guard.as_ref() { ... }`
5. `uuid` crate is already in Cargo.toml; skip the "add dependency" step.
6. Migration v13 should use `&self` (not `&mut self`) to match newer migration convention (v6+).
7. `source_bytes` and `result_count` are deferred to None in Task 6 for v1 simplicity. A follow-up task can thread these through once the instrumentation skeleton is working.

---

### Task 1: Types and SessionMetrics Foundation

**Files:**
- Create: `src/tools/metrics/session.rs`
- Modify: `src/tools/metrics/mod.rs` (add `pub mod session;`)
- Create: `src/tests/tools/metrics/session_metrics_tests.rs`
- Modify: `src/tests/tools/metrics/mod.rs` (register test module)

This task creates the core types with no side effects on existing code.

- [ ] **Step 1: Write failing tests for ToolKind, ToolCounters, SessionMetrics**

```rust
// src/tests/tools/metrics/session_metrics_tests.rs
use crate::tools::metrics::session::{SessionMetrics, ToolCallReport, ToolKind};
use std::sync::Arc;

#[test]
fn test_tool_kind_ordinal_covers_all_tools() {
    // All 8 tools have distinct ordinals 0..7
    assert_eq!(ToolKind::FastSearch as u8, 0);
    assert_eq!(ToolKind::FastRefs as u8, 1);
    assert_eq!(ToolKind::GetSymbols as u8, 2);
    assert_eq!(ToolKind::DeepDive as u8, 3);
    assert_eq!(ToolKind::GetContext as u8, 4);
    assert_eq!(ToolKind::RenameSymbol as u8, 5);
    assert_eq!(ToolKind::ManageWorkspace as u8, 6);
    assert_eq!(ToolKind::QueryMetrics as u8, 7);
}

#[test]
fn test_session_metrics_new_starts_at_zero() {
    let metrics = SessionMetrics::new();
    assert_eq!(metrics.total_calls(), 0);
    assert_eq!(metrics.total_output_bytes(), 0);
    assert_eq!(metrics.total_source_bytes(), 0);
    assert!(!metrics.session_id.is_empty());
}

#[test]
fn test_session_metrics_record_increments_atomics() {
    let metrics = Arc::new(SessionMetrics::new());
    metrics.record(ToolKind::FastSearch, 1500, 200, 5000); // 1500us, 200 source bytes, 5000 output bytes

    assert_eq!(metrics.total_calls(), 1);
    assert_eq!(metrics.total_source_bytes(), 200);
    assert_eq!(metrics.total_output_bytes(), 5000);

    let tool = &metrics.per_tool[ToolKind::FastSearch as usize];
    assert_eq!(tool.calls.load(std::sync::atomic::Ordering::Relaxed), 1);
    assert_eq!(tool.output_bytes.load(std::sync::atomic::Ordering::Relaxed), 5000);
}

#[test]
fn test_session_metrics_multiple_tools() {
    let metrics = Arc::new(SessionMetrics::new());
    metrics.record(ToolKind::FastSearch, 1000, 100, 500);
    metrics.record(ToolKind::FastSearch, 2000, 300, 800);
    metrics.record(ToolKind::DeepDive, 5000, 1000, 2000);

    assert_eq!(metrics.total_calls(), 3);
    assert_eq!(metrics.total_source_bytes(), 1400);
    assert_eq!(metrics.total_output_bytes(), 3300);

    let search = &metrics.per_tool[ToolKind::FastSearch as usize];
    assert_eq!(search.calls.load(std::sync::atomic::Ordering::Relaxed), 2);

    let dive = &metrics.per_tool[ToolKind::DeepDive as usize];
    assert_eq!(dive.calls.load(std::sync::atomic::Ordering::Relaxed), 1);
}

#[test]
fn test_tool_call_report_default() {
    let report = ToolCallReport::empty();
    assert_eq!(report.result_count, None);
    assert_eq!(report.source_bytes, None);
    assert_eq!(report.output_bytes, 0);
    assert_eq!(report.metadata, serde_json::Value::Null);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib tests::tools::metrics::session_metrics_tests 2>&1 | tail -10`
Expected: FAIL (module doesn't exist)

- [ ] **Step 3: Implement SessionMetrics types**

```rust
// src/tools/metrics/session.rs
//! In-memory session metrics with atomic counters.
//!
//! Pre-allocated at handler construction, zero-allocation on the hot path.
//! Indexed by ToolKind ordinal for O(1) per-tool counter access.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// Per-tool atomic counters. Default-initialized to zero.
pub struct ToolCounters {
    pub calls: AtomicU64,
    pub duration_us: AtomicU64,
    pub output_bytes: AtomicU64,
}

impl Default for ToolCounters {
    fn default() -> Self {
        Self {
            calls: AtomicU64::new(0),
            duration_us: AtomicU64::new(0),
            output_bytes: AtomicU64::new(0),
        }
    }
}

/// Maps tool names to array indices. Known at compile time.
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum ToolKind {
    FastSearch = 0,
    FastRefs = 1,
    GetSymbols = 2,
    DeepDive = 3,
    GetContext = 4,
    RenameSymbol = 5,
    ManageWorkspace = 6,
    QueryMetrics = 7,
}

impl ToolKind {
    pub const COUNT: usize = 8;

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "fast_search" => Some(Self::FastSearch),
            "fast_refs" => Some(Self::FastRefs),
            "get_symbols" => Some(Self::GetSymbols),
            "deep_dive" => Some(Self::DeepDive),
            "get_context" => Some(Self::GetContext),
            "rename_symbol" => Some(Self::RenameSymbol),
            "manage_workspace" => Some(Self::ManageWorkspace),
            "query_metrics" => Some(Self::QueryMetrics),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::FastSearch => "fast_search",
            Self::FastRefs => "fast_refs",
            Self::GetSymbols => "get_symbols",
            Self::DeepDive => "deep_dive",
            Self::GetContext => "get_context",
            Self::RenameSymbol => "rename_symbol",
            Self::ManageWorkspace => "manage_workspace",
            Self::QueryMetrics => "query_metrics",
        }
    }
}

/// Metrics captured from inside a tool's call_tool method.
/// Built where both inputs and outputs are available.
pub struct ToolCallReport {
    pub result_count: Option<u32>,
    pub source_bytes: Option<u64>,
    pub output_bytes: u64,
    pub metadata: serde_json::Value,
}

impl ToolCallReport {
    pub fn empty() -> Self {
        Self {
            result_count: None,
            source_bytes: None,
            output_bytes: 0,
            metadata: serde_json::Value::Null,
        }
    }
}

/// Session-wide metrics. Wrapped in Arc on the handler.
/// All fields use atomics for lock-free concurrent access.
pub struct SessionMetrics {
    pub session_id: String,
    pub session_start: Instant,
    pub total_calls: AtomicU64,
    pub total_duration_us: AtomicU64,
    pub total_source_bytes: AtomicU64,
    pub total_output_bytes: AtomicU64,
    pub per_tool: [ToolCounters; ToolKind::COUNT],
}

impl SessionMetrics {
    pub fn new() -> Self {
        Self {
            session_id: uuid::Uuid::new_v4().to_string(),
            session_start: Instant::now(),
            total_calls: AtomicU64::new(0),
            total_duration_us: AtomicU64::new(0),
            total_source_bytes: AtomicU64::new(0),
            total_output_bytes: AtomicU64::new(0),
            per_tool: std::array::from_fn(|_| ToolCounters::default()),
        }
    }

    /// Record a completed tool call. Called synchronously from the handler.
    pub fn record(&self, tool: ToolKind, duration_us: u64, source_bytes: u64, output_bytes: u64) {
        self.total_calls.fetch_add(1, Ordering::Relaxed);
        self.total_duration_us.fetch_add(duration_us, Ordering::Relaxed);
        self.total_source_bytes.fetch_add(source_bytes, Ordering::Relaxed);
        self.total_output_bytes.fetch_add(output_bytes, Ordering::Relaxed);

        let counters = &self.per_tool[tool as usize];
        counters.calls.fetch_add(1, Ordering::Relaxed);
        counters.duration_us.fetch_add(duration_us, Ordering::Relaxed);
        counters.output_bytes.fetch_add(output_bytes, Ordering::Relaxed);
    }

    // Convenience readers
    pub fn total_calls(&self) -> u64 { self.total_calls.load(Ordering::Relaxed) }
    pub fn total_source_bytes(&self) -> u64 { self.total_source_bytes.load(Ordering::Relaxed) }
    pub fn total_output_bytes(&self) -> u64 { self.total_output_bytes.load(Ordering::Relaxed) }
}
```

- [ ] **Step 4: Register module and tests**

Add to `src/tools/metrics/mod.rs`:
```rust
pub mod session;
```

Add to `src/tests/tools/metrics/mod.rs`:
```rust
pub mod session_metrics_tests;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib tests::tools::metrics::session_metrics_tests 2>&1 | tail -10`
Expected: PASS (all 4 tests)

- [ ] **Step 6: Add `uuid` dependency if not already present**

Check `Cargo.toml` for `uuid`. If missing:
```bash
cargo add uuid --features v4
```

- [ ] **Step 7: Commit**

```bash
git add src/tools/metrics/session.rs src/tests/tools/metrics/session_metrics_tests.rs src/tools/metrics/mod.rs src/tests/tools/metrics/mod.rs
git commit -m "feat(metrics): add SessionMetrics and ToolCallReport types"
```

---

### Task 2: Migration v13 - tool_calls Table + line_count Column

**Files:**
- Modify: `src/database/migrations.rs` (bump version, add migration fn)
- Modify: `src/database/schema.rs:63-93` (add line_count to CREATE TABLE)
- Modify: `src/database/types.rs` (add line_count to FileInfo)
- Modify: `src/database/files.rs:19-40,98-114` (add line_count to INSERT statements)
- Create: `src/tests/tools/metrics/migration_tests.rs`
- Modify: `src/tests/tools/metrics/mod.rs` (register test module)

- [ ] **Step 1: Write failing test for migration v13**

```rust
// src/tests/tools/metrics/migration_tests.rs
use crate::database::SymbolDatabase;
use tempfile::TempDir;

#[test]
fn test_migration_013_creates_tool_calls_table() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    // Verify tool_calls table exists
    let count: i32 = db.conn().query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='tool_calls'",
        [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(count, 1, "tool_calls table should exist after migration");

    // Verify expected columns
    let col_count: i32 = db.conn().query_row(
        "SELECT COUNT(*) FROM pragma_table_info('tool_calls')",
        [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(col_count, 9, "tool_calls should have 9 columns");
}

#[test]
fn test_migration_013_adds_line_count_to_files() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    assert!(db.has_column("files", "line_count").unwrap(), "files table should have line_count column");
}

#[test]
fn test_migration_013_tool_calls_indexes_exist() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    let indexes: Vec<String> = {
        let mut stmt = db.conn().prepare(
            "SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='tool_calls'"
        ).unwrap();
        stmt.query_map([], |row| row.get(0)).unwrap()
            .collect::<Result<Vec<_>, _>>().unwrap()
    };
    assert!(indexes.iter().any(|n| n == "idx_tool_calls_timestamp"));
    assert!(indexes.iter().any(|n| n == "idx_tool_calls_tool_name"));
    assert!(indexes.iter().any(|n| n == "idx_tool_calls_session"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib tests::tools::metrics::migration_tests 2>&1 | tail -10`
Expected: FAIL

- [ ] **Step 3: Implement migration v13**

In `src/database/migrations.rs`:

1. Change `LATEST_SCHEMA_VERSION` from `12` to `13`
2. Add match arm in `apply_migration`: `13 => self.migration_013_add_tool_calls_and_line_count()?`
3. Add match arm in `record_migration`: `11 => "Add embedding config table"`, `12 => "Add memory vectors table"`, `13 => "Add tool_calls table and line_count column"`
4. Add the migration function:

```rust
fn migration_013_add_tool_calls_and_line_count(&mut self) -> Result<()> {
    info!("Migration 013: Adding tool_calls table and line_count column");

    // Create tool_calls table
    self.conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS tool_calls (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL,
            timestamp INTEGER NOT NULL,
            tool_name TEXT NOT NULL,
            duration_ms REAL NOT NULL,
            result_count INTEGER,
            source_bytes INTEGER,
            output_bytes INTEGER,
            success INTEGER NOT NULL DEFAULT 1,
            metadata TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_tool_calls_timestamp ON tool_calls(timestamp);
        CREATE INDEX IF NOT EXISTS idx_tool_calls_tool_name ON tool_calls(tool_name);
        CREATE INDEX IF NOT EXISTS idx_tool_calls_session ON tool_calls(session_id);
        "
    )?;

    // Add line_count to files table (idempotent)
    if !self.has_column("files", "line_count")? {
        self.conn.execute(
            "ALTER TABLE files ADD COLUMN line_count INTEGER DEFAULT 0",
            [],
        )?;
    }

    info!("✅ tool_calls table and line_count column added");
    Ok(())
}
```

5. In `src/database/schema.rs`, update `create_files_table` to include `line_count INTEGER DEFAULT 0` in the CREATE TABLE statement (for fresh databases).

6. In `src/database/types.rs`, add `pub line_count: i32` to `FileInfo`.

7. In `src/database/files.rs`, update `store_file_info` and `bulk_store_files` INSERT statements to include the `line_count` column (9th param). Use `file_info.line_count` as the value.

- [ ] **Step 4: Register test module**

Add to `src/tests/tools/metrics/mod.rs`:
```rust
pub mod migration_tests;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib tests::tools::metrics::migration_tests 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/database/migrations.rs src/database/schema.rs src/database/types.rs src/database/files.rs src/tests/tools/metrics/migration_tests.rs src/tests/tools/metrics/mod.rs
git commit -m "feat(metrics): migration v13 - tool_calls table and line_count column"
```

---

### Task 3: tool_calls Database CRUD

**Files:**
- Create: `src/database/tool_calls.rs`
- Modify: `src/database/mod.rs` (add `mod tool_calls;`)
- Create: `src/tests/tools/metrics/tool_calls_db_tests.rs`
- Modify: `src/tests/tools/metrics/mod.rs` (register test module)

- [ ] **Step 1: Write failing tests for insert + query**

```rust
// src/tests/tools/metrics/tool_calls_db_tests.rs
use crate::database::SymbolDatabase;
use tempfile::TempDir;

fn test_db() -> (TempDir, SymbolDatabase) {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();
    (tmp, db)
}

#[test]
fn test_insert_tool_call() {
    let (_tmp, db) = test_db();
    db.insert_tool_call(
        "session-123",
        "fast_search",
        4.2,
        Some(5),
        Some(52000),
        Some(1200),
        true,
        Some(r#"{"query":"UserService"}"#),
    ).unwrap();

    let count: i32 = db.conn().query_row(
        "SELECT COUNT(*) FROM tool_calls", [], |row| row.get(0)
    ).unwrap();
    assert_eq!(count, 1);
}

#[test]
fn test_query_session_summary() {
    let (_tmp, db) = test_db();
    let session = "sess-abc";

    // Insert several calls
    db.insert_tool_call(session, "fast_search", 3.0, Some(5), Some(10000), Some(500), true, None).unwrap();
    db.insert_tool_call(session, "fast_search", 5.0, Some(3), Some(20000), Some(800), true, None).unwrap();
    db.insert_tool_call(session, "deep_dive", 8.0, Some(1), Some(15000), Some(2000), true, None).unwrap();
    db.insert_tool_call("other-session", "fast_search", 2.0, Some(1), Some(5000), Some(100), true, None).unwrap();

    let summary = db.query_session_summary(session).unwrap();
    assert_eq!(summary.len(), 2); // fast_search and deep_dive
    let search = summary.iter().find(|s| s.tool_name == "fast_search").unwrap();
    assert_eq!(search.call_count, 2);
    assert_eq!(search.total_source_bytes, 30000);
    assert_eq!(search.total_output_bytes, 1300);
}

#[test]
fn test_query_history_summary() {
    let (_tmp, db) = test_db();

    db.insert_tool_call("s1", "fast_search", 3.0, Some(5), Some(10000), Some(500), true, None).unwrap();
    db.insert_tool_call("s2", "fast_search", 5.0, Some(3), Some(20000), Some(800), true, None).unwrap();
    db.insert_tool_call("s1", "deep_dive", 8.0, Some(1), Some(15000), Some(2000), true, None).unwrap();

    let history = db.query_history_summary(7).unwrap(); // last 7 days
    assert_eq!(history.session_count, 2);
    assert_eq!(history.total_calls, 3);
    assert!(history.total_source_bytes > 0);
    assert!(history.per_tool.len() == 2);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib tests::tools::metrics::tool_calls_db_tests 2>&1 | tail -10`
Expected: FAIL

- [ ] **Step 3: Implement tool_calls CRUD**

```rust
// src/database/tool_calls.rs
//! CRUD operations for the tool_calls metrics table.

use super::SymbolDatabase;
use anyhow::Result;
use rusqlite::params;

/// Per-tool summary for a session or time window.
pub struct ToolCallSummary {
    pub tool_name: String,
    pub call_count: u64,
    pub avg_duration_ms: f64,
    pub total_source_bytes: u64,
    pub total_output_bytes: u64,
}

/// Aggregated history across sessions.
pub struct HistorySummary {
    pub session_count: u64,
    pub total_calls: u64,
    pub total_source_bytes: u64,
    pub total_output_bytes: u64,
    pub per_tool: Vec<ToolCallSummary>,
    pub durations_by_tool: std::collections::HashMap<String, Vec<f64>>,
}

impl SymbolDatabase {
    pub fn insert_tool_call(
        &self,
        session_id: &str,
        tool_name: &str,
        duration_ms: f64,
        result_count: Option<u32>,
        source_bytes: Option<u64>,
        output_bytes: Option<u64>,
        success: bool,
        metadata: Option<&str>,
    ) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs() as i64;

        self.conn.execute(
            "INSERT INTO tool_calls (session_id, timestamp, tool_name, duration_ms, result_count, source_bytes, output_bytes, success, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                session_id,
                now,
                tool_name,
                duration_ms,
                result_count.map(|v| v as i64),
                source_bytes.map(|v| v as i64),
                output_bytes.map(|v| v as i64),
                if success { 1 } else { 0 },
                metadata,
            ],
        )?;
        Ok(())
    }

    pub fn query_session_summary(&self, session_id: &str) -> Result<Vec<ToolCallSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT tool_name, COUNT(*), AVG(duration_ms), COALESCE(SUM(source_bytes), 0), COALESCE(SUM(output_bytes), 0)
             FROM tool_calls
             WHERE session_id = ?1
             GROUP BY tool_name
             ORDER BY COUNT(*) DESC"
        )?;

        let results = stmt.query_map(params![session_id], |row| {
            Ok(ToolCallSummary {
                tool_name: row.get(0)?,
                call_count: row.get::<_, i64>(1)? as u64,
                avg_duration_ms: row.get(2)?,
                total_source_bytes: row.get::<_, i64>(3)? as u64,
                total_output_bytes: row.get::<_, i64>(4)? as u64,
            })
        })?.collect::<Result<Vec<_>, _>>()?;

        Ok(results)
    }

    pub fn query_history_summary(&self, days: u32) -> Result<HistorySummary> {
        let cutoff = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs() as i64 - (days as i64 * 86400);

        let session_count: i64 = self.conn.query_row(
            "SELECT COUNT(DISTINCT session_id) FROM tool_calls WHERE timestamp >= ?1",
            params![cutoff],
            |row| row.get(0),
        )?;

        let total_calls: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM tool_calls WHERE timestamp >= ?1",
            params![cutoff],
            |row| row.get(0),
        )?;

        let (total_source, total_output): (i64, i64) = self.conn.query_row(
            "SELECT COALESCE(SUM(source_bytes), 0), COALESCE(SUM(output_bytes), 0) FROM tool_calls WHERE timestamp >= ?1",
            params![cutoff],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        let mut stmt = self.conn.prepare(
            "SELECT tool_name, COUNT(*), AVG(duration_ms), COALESCE(SUM(source_bytes), 0), COALESCE(SUM(output_bytes), 0)
             FROM tool_calls WHERE timestamp >= ?1
             GROUP BY tool_name ORDER BY COUNT(*) DESC"
        )?;
        let per_tool = stmt.query_map(params![cutoff], |row| {
            Ok(ToolCallSummary {
                tool_name: row.get(0)?,
                call_count: row.get::<_, i64>(1)? as u64,
                avg_duration_ms: row.get(2)?,
                total_source_bytes: row.get::<_, i64>(3)? as u64,
                total_output_bytes: row.get::<_, i64>(4)? as u64,
            })
        })?.collect::<Result<Vec<_>, _>>()?;

        // Fetch durations per tool for p95 calculation
        let mut dur_stmt = self.conn.prepare(
            "SELECT tool_name, duration_ms FROM tool_calls WHERE timestamp >= ?1 ORDER BY tool_name"
        )?;
        let mut durations_by_tool: std::collections::HashMap<String, Vec<f64>> = std::collections::HashMap::new();
        let rows = dur_stmt.query_map(params![cutoff], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
        })?;
        for row in rows {
            let (name, dur) = row?;
            durations_by_tool.entry(name).or_default().push(dur);
        }

        Ok(HistorySummary {
            session_count: session_count as u64,
            total_calls: total_calls as u64,
            total_source_bytes: total_source as u64,
            total_output_bytes: total_output as u64,
            per_tool,
            durations_by_tool,
        })
    }
}
```

- [ ] **Step 4: Register module**

Add to `src/database/mod.rs`:
```rust
mod tool_calls;
pub use tool_calls::{ToolCallSummary, HistorySummary};
```

Add to `src/tests/tools/metrics/mod.rs`:
```rust
pub mod tool_calls_db_tests;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib tests::tools::metrics::tool_calls_db_tests 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/database/tool_calls.rs src/database/mod.rs src/tests/tools/metrics/tool_calls_db_tests.rs src/tests/tools/metrics/mod.rs
git commit -m "feat(metrics): tool_calls CRUD operations for insert and aggregation queries"
```

---

### Task 4: Wire SessionMetrics into Handler + record_tool_call

**Files:**
- Modify: `src/handler.rs` (add `session_metrics` field, `record_tool_call` method)

This task adds the plumbing without changing any tool's behavior yet.

- [ ] **Step 1: Write failing test for handler record_tool_call**

```rust
// Add to src/tests/tools/metrics/session_metrics_tests.rs

#[tokio::test]
async fn test_handler_has_session_metrics() {
    let handler = crate::handler::JulieServerHandler::new_for_test().await.unwrap();
    // session_metrics should be accessible and start at zero
    assert_eq!(handler.session_metrics.total_calls(), 0);
    assert!(!handler.session_metrics.session_id.is_empty());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib tests::tools::metrics::session_metrics_tests::test_handler_has_session_metrics 2>&1 | tail -10`
Expected: FAIL (no `session_metrics` field)

- [ ] **Step 3: Add SessionMetrics to JulieServerHandler**

In `src/handler.rs`:

1. Add import: `use crate::tools::metrics::session::{SessionMetrics, ToolCallReport, ToolKind};`
2. Add field to `JulieServerHandler`:
   ```rust
   pub session_metrics: Arc<SessionMetrics>,
   ```
3. Initialize in `new()`:
   ```rust
   session_metrics: Arc::new(SessionMetrics::new()),
   ```
4. Add `record_tool_call` method to `impl JulieServerHandler`:
   ```rust
   /// Record a completed tool call. Bumps in-memory atomics and spawns async SQLite write.
   pub(crate) fn record_tool_call(
       &self,
       tool_name: &str,
       duration: std::time::Duration,
       report: &ToolCallReport,
   ) {
       let duration_us = duration.as_micros() as u64;
       let source_bytes = report.source_bytes.unwrap_or(0);

       // Bump in-memory atomics (synchronous, ~50ns)
       if let Some(kind) = ToolKind::from_name(tool_name) {
           self.session_metrics.record(kind, duration_us, source_bytes, report.output_bytes);
       }

       // Async SQLite write (fire-and-forget)
       let workspace = self.workspace.clone();
       let session_id = self.session_metrics.session_id.clone();
       let tool_name = tool_name.to_string();
       let duration_ms = duration.as_secs_f64() * 1000.0;
       let result_count = report.result_count;
       let source_bytes_opt = report.source_bytes;
       let output_bytes = report.output_bytes;
       let success = true;
       let metadata = report.metadata.to_string();
       let metadata_str = if metadata == "null" { None } else { Some(metadata) };

       tokio::spawn(async move {
           if let Ok(Some(ws)) = tokio::sync::RwLock::read(&workspace).await.as_ref()
               .map(|guard| guard.as_ref())
           {
               if let Some(db_arc) = &ws.db {
                   if let Ok(db) = db_arc.lock() {
                       let _ = db.insert_tool_call(
                           &session_id,
                           &tool_name,
                           duration_ms,
                           result_count,
                           source_bytes_opt,
                           Some(output_bytes),
                           success,
                           metadata_str.as_deref(),
                       );
                   }
               }
           }
       });
   }
   ```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib tests::tools::metrics::session_metrics_tests::test_handler_has_session_metrics 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/handler.rs
git commit -m "feat(metrics): wire SessionMetrics into handler with record_tool_call"
```

---

### Task 5: Batch File Size Query Helper

**Files:**
- Modify: `src/database/files.rs` (add `get_total_file_sizes`)
- Create: `src/tests/tools/metrics/file_size_query_tests.rs`
- Modify: `src/tests/tools/metrics/mod.rs`

Tools need to look up total source bytes for the files they examined. This adds a batch helper.

- [ ] **Step 1: Write failing test**

```rust
// src/tests/tools/metrics/file_size_query_tests.rs
use crate::database::SymbolDatabase;
use crate::database::types::FileInfo;
use tempfile::TempDir;

#[test]
fn test_get_total_file_sizes() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    // Store some files
    db.store_file_info(&FileInfo {
        path: "src/main.rs".to_string(),
        language: "rust".to_string(),
        hash: "abc".to_string(),
        size: 1000,
        last_modified: 0,
        last_indexed: 0,
        symbol_count: 5,
        line_count: 50,
        content: None,
    }).unwrap();
    db.store_file_info(&FileInfo {
        path: "src/lib.rs".to_string(),
        language: "rust".to_string(),
        hash: "def".to_string(),
        size: 2000,
        last_modified: 0,
        last_indexed: 0,
        symbol_count: 10,
        line_count: 100,
        content: None,
    }).unwrap();

    let total = db.get_total_file_sizes(&["src/main.rs", "src/lib.rs"]).unwrap();
    assert_eq!(total, 3000);

    let partial = db.get_total_file_sizes(&["src/main.rs"]).unwrap();
    assert_eq!(partial, 1000);

    let empty = db.get_total_file_sizes(&[]).unwrap();
    assert_eq!(empty, 0);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib tests::tools::metrics::file_size_query_tests 2>&1 | tail -10`
Expected: FAIL

- [ ] **Step 3: Implement get_total_file_sizes**

Add to `src/database/files.rs`:

```rust
/// Get total file sizes for a set of paths. Used by metrics collection.
pub fn get_total_file_sizes(&self, paths: &[&str]) -> Result<u64> {
    if paths.is_empty() {
        return Ok(0);
    }
    let placeholders: Vec<String> = (1..=paths.len()).map(|i| format!("?{}", i)).collect();
    let sql = format!(
        "SELECT COALESCE(SUM(size), 0) FROM files WHERE path IN ({})",
        placeholders.join(", ")
    );
    let params: Vec<&dyn rusqlite::types::ToSql> = paths.iter().map(|p| p as &dyn rusqlite::types::ToSql).collect();
    let total: i64 = self.conn.query_row(&sql, params.as_slice(), |row| row.get(0))?;
    Ok(total as u64)
}
```

- [ ] **Step 4: Register test module**

Add to `src/tests/tools/metrics/mod.rs`:
```rust
pub mod file_size_query_tests;
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --lib tests::tools::metrics::file_size_query_tests 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/database/files.rs src/tests/tools/metrics/file_size_query_tests.rs src/tests/tools/metrics/mod.rs
git commit -m "feat(metrics): batch file size query helper for source_bytes collection"
```

---

### Task 6: Instrument All 8 Tool Handlers

**Files:**
- Modify: `src/handler.rs` (wrap each tool handler with timing + report)
- Modify: `src/tools/search/mod.rs` (add `call_tool_with_metrics` to FastSearchTool)
- Modify: `src/tools/deep_dive/mod.rs` (add `call_tool_with_metrics` to DeepDiveTool)
- Modify: `src/tools/navigation/fast_refs.rs` (add `call_tool_with_metrics` to FastRefsTool)
- Modify: `src/tools/symbols/mod.rs` (add `call_tool_with_metrics` to GetSymbolsTool)
- Modify: `src/tools/get_context/mod.rs` (add `call_tool_with_metrics` to GetContextTool)
- Modify: `src/tools/refactoring/mod.rs` (add `call_tool_with_metrics` to RenameSymbolTool)
- Modify: `src/tools/workspace/commands/mod.rs` (add `call_tool_with_metrics` to ManageWorkspaceTool)
- Modify: `src/tools/metrics/mod.rs` (add `call_tool_with_metrics` to QueryMetricsTool)

This is the largest task. Each tool needs a `call_tool_with_metrics` method that wraps the existing `call_tool` and returns a `(CallToolResult, ToolCallReport)`. Start with `fast_search` as the pattern, then replicate.

- [ ] **Step 1: Define the instrumented handler pattern**

Update each handler method in `src/handler.rs` from:
```rust
async fn fast_search(&self, Parameters(params): Parameters<FastSearchTool>) -> Result<CallToolResult, McpError> {
    debug!("⚡ Fast search: {:?}", params);
    params.call_tool(self).await
        .map_err(|e| McpError::internal_error(format!("fast_search failed: {}", e), None))
}
```
To:
```rust
async fn fast_search(&self, Parameters(params): Parameters<FastSearchTool>) -> Result<CallToolResult, McpError> {
    debug!("⚡ Fast search: {:?}", params);
    let start = std::time::Instant::now();
    let (result, report) = params.call_tool_with_metrics(self).await
        .map_err(|e| McpError::internal_error(format!("fast_search failed: {}", e), None))?;
    self.record_tool_call("fast_search", start.elapsed(), &report);
    Ok(result)
}
```

- [ ] **Step 2: Add call_tool_with_metrics to FastSearchTool**

In `src/tools/search/mod.rs`, add alongside the existing `call_tool`:

```rust
pub async fn call_tool_with_metrics(&self, handler: &JulieServerHandler) -> Result<(CallToolResult, super::metrics::session::ToolCallReport)> {
    use super::metrics::session::ToolCallReport;
    let result = self.call_tool(handler).await?;
    let output_bytes = result.content.iter()
        .filter_map(|c| c.as_text())
        .map(|t| t.len() as u64)
        .sum();
    let report = ToolCallReport {
        result_count: None, // Populated in later iteration
        source_bytes: None, // Populated in later iteration
        output_bytes,
        metadata: serde_json::json!({
            "query": self.query,
            "target": self.search_target,
        }),
    };
    Ok((result, report))
}
```

Note: `result_count` and `source_bytes` start as `None`. These will be threaded through in a follow-up. The critical path (timing, output_bytes, metadata) is captured immediately.

- [ ] **Step 3: Add call_tool_with_metrics to all remaining tools**

Apply the same pattern to each tool. The metadata JSON varies per tool:

- **DeepDiveTool:** `{"symbol": self.symbol, "depth": self.depth}`
- **FastRefsTool:** `{"symbol": self.symbol}`
- **GetSymbolsTool:** `{"file": self.file_path, "mode": self.mode, "target": self.target}`
- **GetContextTool:** `{"query": self.query}`
- **RenameSymbolTool:** `{"old": self.old_name, "new": self.new_name, "dry_run": self.dry_run}`
- **ManageWorkspaceTool:** `{"operation": self.operation}` (use existing `call_tool`, not `call_tool_with_options`)
- **QueryMetricsTool:** `{"category": "code_health"}` (will be updated in Task 7)

For **ManageWorkspaceTool**, note it has two call patterns (`call_tool` and `call_tool_with_options`). The `call_tool_with_metrics` should wrap `call_tool`. The `run_auto_indexing` path in handler.rs (line 321) calls `call_tool_with_options` directly; that path does NOT need metrics (it's background auto-indexing, not a user tool call).

- [ ] **Step 4: Update all 8 handler methods**

Apply the timing wrapper pattern from Step 1 to: `fast_search`, `fast_refs`, `get_symbols`, `deep_dive`, `get_context`, `rename_symbol`, `manage_workspace`, `query_metrics`.

For `manage_workspace`, the handler currently calls `params.call_tool(self)`, not `call_tool_with_options`. Keep it that way; `call_tool_with_metrics` wraps `call_tool`.

- [ ] **Step 5: Build and verify no compilation errors**

Run: `cargo build 2>&1 | tail -20`
Expected: BUILD SUCCESS

- [ ] **Step 6: Run existing test suite to verify no regressions**

Run: `cargo xtask test dev 2>&1 | tail -20`
Expected: No new failures beyond known pre-existing ones

- [ ] **Step 7: Commit**

```bash
git add src/handler.rs src/tools/search/mod.rs src/tools/deep_dive/mod.rs src/tools/navigation/fast_refs.rs src/tools/symbols/mod.rs src/tools/get_context/mod.rs src/tools/refactoring/mod.rs src/tools/workspace/commands/mod.rs src/tools/metrics/mod.rs
git commit -m "feat(metrics): instrument all 8 tool handlers with timing and ToolCallReport"
```

---

### Task 7: query_metrics Category Expansion (session + history)

**Files:**
- Modify: `src/tools/metrics/mod.rs` (add `category` param, route to handlers)
- Create: `src/tools/metrics/operational.rs` (session + history query + formatting)
- Create: `src/tests/tools/metrics/operational_metrics_tests.rs`
- Modify: `src/tests/tools/metrics/mod.rs` (register test module)

- [ ] **Step 1: Write failing tests for category routing**

```rust
// src/tests/tools/metrics/operational_metrics_tests.rs
use crate::tools::metrics::operational;

#[test]
fn test_format_session_output_empty() {
    let output = operational::format_session_output(
        std::time::Duration::from_secs(300),
        &[],
        0, 0, 0, 0,
    );
    assert!(output.contains("Session Metrics"));
    assert!(output.contains("uptime: 5m"));
    assert!(output.contains("0 calls"));
}

#[test]
fn test_format_session_output_with_data() {
    use crate::tools::metrics::session::{SessionMetrics, ToolKind};
    use std::sync::Arc;

    let metrics = Arc::new(SessionMetrics::new());
    metrics.record(ToolKind::FastSearch, 4100, 52000, 1200);
    metrics.record(ToolKind::FastSearch, 3200, 48000, 800);
    metrics.record(ToolKind::DeepDive, 8300, 15000, 2000);

    let output = operational::format_session_from_metrics(&metrics);
    assert!(output.contains("fast_search"));
    assert!(output.contains("2 calls"));
    assert!(output.contains("deep_dive"));
    assert!(output.contains("NOT injected"));
}

#[test]
fn test_format_bytes_human_readable() {
    assert_eq!(operational::format_bytes(500), "500B");
    assert_eq!(operational::format_bytes(1024), "1.0KB");
    assert_eq!(operational::format_bytes(1_048_576), "1.0MB");
    assert_eq!(operational::format_bytes(52_000), "50.8KB");
}

#[test]
fn test_p95_calculation() {
    let mut durations = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0,
                             11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0, 18.0, 19.0, 20.0];
    let p95 = operational::percentile_95(&mut durations);
    assert!(p95 >= 19.0 && p95 <= 20.0);
}

#[test]
fn test_format_history_output() {
    use crate::database::tool_calls::{ToolCallSummary, HistorySummary};
    use std::collections::HashMap;

    let history = HistorySummary {
        session_count: 5,
        total_calls: 100,
        total_source_bytes: 2_000_000,
        total_output_bytes: 50_000,
        per_tool: vec![
            ToolCallSummary {
                tool_name: "fast_search".to_string(),
                call_count: 60,
                avg_duration_ms: 4.2,
                total_source_bytes: 1_200_000,
                total_output_bytes: 30_000,
            },
        ],
        durations_by_tool: HashMap::from([
            ("fast_search".to_string(), vec![3.0, 4.0, 5.0, 6.0, 12.0]),
        ]),
    };
    let output = operational::format_history_output(&history);
    assert!(output.contains("Historical Metrics"));
    assert!(output.contains("5 sessions"));
    assert!(output.contains("fast_search"));
    assert!(output.contains("NOT injected"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib tests::tools::metrics::operational_metrics_tests 2>&1 | tail -10`
Expected: FAIL

- [ ] **Step 3: Implement operational.rs (formatting + query)**

Create `src/tools/metrics/operational.rs` with:
- `format_bytes(bytes: u64) -> String`
- `percentile_95(durations: &mut [f64]) -> f64`
- `format_session_from_metrics(metrics: &SessionMetrics) -> String`
- `format_session_output(uptime, summaries, total_calls, total_source, total_output, total_files) -> String`
- `format_history_output(history: &HistorySummary) -> String`

The session formatter reads atomic counters from `SessionMetrics`, iterates `per_tool[0..8]`, and builds the output with the "NOT injected into context" headline.

The history formatter takes a `HistorySummary` from the database, computes p95 from `durations_by_tool`, and formats similarly.

- [ ] **Step 4: Add `category` param to QueryMetricsTool**

In `src/tools/metrics/mod.rs`:

1. Add default function: `fn default_category() -> String { "code_health".to_string() }`
2. Add field to `QueryMetricsTool`:
   ```rust
   /// Metrics category: "code_health" (default), "session", or "history"
   #[serde(default = "default_category")]
   pub category: String,
   ```
3. Update `call_tool` to route by category:
   ```rust
   pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
       match self.category.as_str() {
           "session" => {
               let output = operational::format_session_from_metrics(&handler.session_metrics);
               Ok(CallToolResult::text_content(vec![Content::text(output)]))
           }
           "history" => {
               // ... get db, query history, format
           }
           _ => {
               // Existing code_health logic (unchanged)
           }
       }
   }
   ```
4. Update the tool description in `handler.rs` to mention the new categories.

- [ ] **Step 5: Register module**

Add to `src/tools/metrics/mod.rs`:
```rust
pub(crate) mod operational;
```

Add to `src/tests/tools/metrics/mod.rs`:
```rust
pub mod operational_metrics_tests;
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --lib tests::tools::metrics::operational_metrics_tests 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 7: Run xtask dev to check for regressions**

Run: `cargo xtask test dev 2>&1 | tail -20`
Expected: No new failures

- [ ] **Step 8: Commit**

```bash
git add src/tools/metrics/operational.rs src/tools/metrics/mod.rs src/handler.rs src/tests/tools/metrics/operational_metrics_tests.rs src/tests/tools/metrics/mod.rs
git commit -m "feat(metrics): query_metrics category expansion with session and history views"
```

---

### Task 8: Populate line_count During Indexing

**Files:**
- Modify: `src/tools/workspace/indexing/processor.rs` (compute line_count during file processing)

- [ ] **Step 1: Write failing test**

```rust
// Add to src/tests/tools/metrics/migration_tests.rs

#[test]
fn test_line_count_stored_in_file_info() {
    use crate::database::types::FileInfo;

    let info = FileInfo {
        path: "test.rs".to_string(),
        language: "rust".to_string(),
        hash: "abc".to_string(),
        size: 100,
        last_modified: 0,
        last_indexed: 0,
        symbol_count: 0,
        line_count: 0,
        content: Some("line1\nline2\nline3\n".to_string()),
    };

    // line_count should be computable from content
    let computed = info.content.as_ref().map(|c| c.lines().count() as i32).unwrap_or(0);
    assert_eq!(computed, 3);
}
```

- [ ] **Step 2: Run test to verify it passes (this is a sanity check)**

Run: `cargo test --lib tests::tools::metrics::migration_tests::test_line_count_stored_in_file_info 2>&1 | tail -10`
Expected: PASS (it's a pure computation test)

- [ ] **Step 3: Update indexing processor to compute line_count**

In `src/tools/workspace/indexing/processor.rs`, find where `FileInfo` structs are built (look for `FileInfo { path:` patterns). Set `line_count` to `content.lines().count() as i32` when content is available, or `0` otherwise.

There are typically two places:
1. Initial indexing (bulk path) - where files are read and parsed
2. Incremental updates (watcher path) - where changed files are re-indexed

Both need the `line_count` computation.

- [ ] **Step 4: Build and verify**

Run: `cargo build 2>&1 | tail -10`
Expected: BUILD SUCCESS

- [ ] **Step 5: Commit**

```bash
git add src/tools/workspace/indexing/processor.rs src/tests/tools/metrics/migration_tests.rs
git commit -m "feat(metrics): compute line_count during indexing"
```

---

### Task 9: Add Metrics to xtask Test Tier + Final Verification

**Files:**
- Modify: `xtask/test_tiers.toml` (add metrics tests to tools-misc bucket)

- [ ] **Step 1: Add metrics tests to tools-misc bucket**

In `xtask/test_tiers.toml`, add to the `[buckets.tools-misc]` commands array:
```toml
"cargo test --lib tests::tools::metrics -- --skip search_quality",
```

- [ ] **Step 2: Run full dev tier to verify everything passes**

Run: `cargo xtask test dev 2>&1 | tail -30`
Expected: No new failures beyond known pre-existing ones

- [ ] **Step 3: Commit**

```bash
git add xtask/test_tiers.toml
git commit -m "test(metrics): add operational metrics tests to xtask dev tier"
```

---

### Task 10: /metrics Skill

**Files:**
- Create: `.claude/skills/metrics.md`

This is a Claude Code skill, not Rust code. It's a markdown file that teaches Claude how to call `query_metrics` with the right category and present results.

- [ ] **Step 1: Create the skill file**

```markdown
---
name: metrics
description: Show Julie operational metrics - session stats, tool usage, context efficiency, and historical trends. Use when the user asks about Julie's performance, how much context was saved, or wants a metrics report.
---

# Julie Metrics Report

Call the `query_metrics` tool with `category: "session"` to get current session metrics.

Present the results to the user. Lead with the "NOT injected into context" headline number.

If the user asks for history or trends, also call `query_metrics` with `category: "history"` and present both.

Do NOT editorialize or make value claims beyond what the numbers show. Present the data and let the user draw conclusions.
```

- [ ] **Step 2: Commit**

```bash
git add .claude/skills/metrics.md
git commit -m "feat(metrics): add /metrics skill for formatted session reports"
```

---

## File Map Summary

| File | Action | Purpose |
|------|--------|---------|
| `src/tools/metrics/session.rs` | Create | ToolKind, ToolCounters, SessionMetrics, ToolCallReport |
| `src/tools/metrics/operational.rs` | Create | Session + history formatting, p95, bytes formatting |
| `src/tools/metrics/mod.rs` | Modify | Add category param, route session/history, register modules |
| `src/database/tool_calls.rs` | Create | Insert + query aggregation for tool_calls table |
| `src/database/mod.rs` | Modify | Register tool_calls module |
| `src/database/migrations.rs` | Modify | Migration v13 |
| `src/database/schema.rs` | Modify | Add line_count to files CREATE TABLE |
| `src/database/types.rs` | Modify | Add line_count to FileInfo |
| `src/database/files.rs` | Modify | line_count in INSERT, get_total_file_sizes helper |
| `src/handler.rs` | Modify | Arc\<SessionMetrics\> field, record_tool_call, timing wrappers |
| `src/tools/search/mod.rs` | Modify | call_tool_with_metrics |
| `src/tools/deep_dive/mod.rs` | Modify | call_tool_with_metrics |
| `src/tools/navigation/fast_refs.rs` | Modify | call_tool_with_metrics |
| `src/tools/symbols/mod.rs` | Modify | call_tool_with_metrics |
| `src/tools/get_context/mod.rs` | Modify | call_tool_with_metrics |
| `src/tools/refactoring/mod.rs` | Modify | call_tool_with_metrics |
| `src/tools/workspace/commands/mod.rs` | Modify | call_tool_with_metrics |
| `src/tools/workspace/indexing/processor.rs` | Modify | Compute line_count during indexing |
| `src/tests/tools/metrics/session_metrics_tests.rs` | Create | SessionMetrics unit tests |
| `src/tests/tools/metrics/migration_tests.rs` | Create | Migration v13 tests |
| `src/tests/tools/metrics/tool_calls_db_tests.rs` | Create | CRUD tests |
| `src/tests/tools/metrics/file_size_query_tests.rs` | Create | Batch file size query tests |
| `src/tests/tools/metrics/operational_metrics_tests.rs` | Create | Formatting + aggregation tests |
| `src/tests/tools/metrics/mod.rs` | Modify | Register all new test modules |
| `xtask/test_tiers.toml` | Modify | Add metrics to dev tier |
| `.claude/skills/metrics.md` | Create | /metrics skill |
