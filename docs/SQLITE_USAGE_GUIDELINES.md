# SQLite Usage Guidelines for Julie

**Status**: Active Development Standards
**Last Updated**: 2025-11-07
**Priority**: CRITICAL - Prevents Database Corruption

---

## Overview

This document establishes **mandatory** SQLite usage patterns for the Julie codebase to prevent "database disk image is malformed" corruption errors that were occurring daily during development.

## The Problem We Solved

### Root Cause Identified

**Issue**: Daily "database disk image is malformed" errors
**Root Cause**: Schema migrations executing in DELETE journal mode before WAL was enabled
**Trigger**: Building new Julie with schema changes while old MCP process had database open

### The Fatal Sequence (BEFORE FIX)

1. Old MCP running with connection to schema v3
2. Build new Julie → bumps `LATEST_SCHEMA_VERSION` to v4
3. New Julie starts, opens SAME database file:
   - Opens connection (DELETE mode by default)
   - Runs migrations v3→v4 (still in DELETE mode!)
   - Sets WAL mode in `initialize_schema()` (too late)
4. **CORRUPTION**: Two processes writing in DELETE mode = guaranteed corruption

### The Fix (CURRENT IMPLEMENTATION)

WAL mode is now set **IMMEDIATELY** after connection open, **BEFORE** any other database operations including migrations.

```rust
// ✅ CORRECT (current implementation)
let conn = Connection::open(&file_path)?;

// 🚨 CRITICAL: WAL mode set FIRST
conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;

// Verify it actually worked
let journal_mode: String = conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
assert!(journal_mode.eq_ignore_ascii_case("wal"));

// NOW safe to run migrations and other operations
db.run_migrations()?;
db.initialize_schema()?;
```

---

## Mandatory SQLite Configuration

### Every Connection MUST:

1. **Set WAL mode IMMEDIATELY after opening**
   ```rust
   conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
   ```

2. **Verify WAL mode was set**
   ```rust
   let mode: String = conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
   if !mode.eq_ignore_ascii_case("wal") {
       return Err(anyhow!("Failed to enable WAL mode"));
   }
   ```

3. **Set appropriate synchronous mode**
   ```rust
   // NORMAL = safe with WAL, 2-3x faster than FULL
   conn.pragma_update(None, "synchronous", "NORMAL")?;
   ```

4. **Configure WAL autocheckpoint**
   ```rust
   // Prevent WAL from growing to 20MB+
   conn.pragma_update(None, "wal_autocheckpoint", 2000)?;
   ```

5. **Set busy timeout for concurrent access**
   ```rust
   conn.busy_timeout(std::time::Duration::from_millis(5000))?;
   ```

---

## Connection Lifecycle

### Opening Connections

**✅ CORRECT** (via `SymbolDatabase::new()`):
```rust
// src/database/mod.rs - The ONLY approved way to open connections
let db = SymbolDatabase::new(&db_path)?;
```

**❌ WRONG** (never do this):
```rust
// DON'T open raw connections - you'll miss critical WAL setup
let conn = Connection::open(&db_path)?;
```

### Closing Connections

Connections are closed automatically via `Drop` implementation which:
- Checkpoints WAL to merge changes into main database
- Truncates WAL file
- Releases file locks

```rust
impl Drop for SymbolDatabase {
    fn drop(&mut self) {
        // Best-effort WAL checkpoint
        let _ = self.checkpoint_wal();
    }
}
```

**Manual checkpointing** (optional, for explicit control):
```rust
db.checkpoint_wal()?;  // Returns (busy, log, checkpointed)
```

---

## Schema Migrations

### Migration Safety Rules

1. **WAL mode MUST be active before migrations**
   - Enforced in `SymbolDatabase::new()` (sets WAL before calling `run_migrations()`)

2. **Detect schema version mismatches**
   ```rust
   let current = db.get_schema_version()?;
   let target = LATEST_SCHEMA_VERSION;

   if current > target {
       return Err(anyhow!(
           "Database schema ({}) is NEWER than code expects ({})",
           current, target
       ));
   }
   ```

3. **Migrations run sequentially in transaction**
   ```rust
   for version in (current + 1)..=target {
       db.apply_migration(version)?;
       db.record_migration(version)?;
   }
   ```

### Development Workflow

**Problem**: Building new Julie with schema changes while old MCP running

**Solutions**:
1. **Stop old MCP before rebuilding** (recommended)
2. **Delete `.julie/indexes/` to force rebuild** (if corruption occurs)
3. **Schema version detection prevents silent corruption**

---

## Transaction Management

### Single Operations
```rust
// Auto-commit is fine for single statements
db.conn.execute("INSERT INTO symbols ...", params![...])?;
```

### Multi-Statement Operations
```rust
// ✅ CORRECT: Wrap in transaction
let tx = db.conn.transaction()?;
tx.execute("INSERT INTO symbols ...", params![...])?;
tx.execute("INSERT INTO relationships ...", params![...])?;
tx.commit()?;
```

### Bulk Operations
```rust
// For bulk inserts, use atomic incremental update
db.incremental_update_atomic(
    &files_to_clean,
    &new_files,
    &new_symbols,
    &new_relationships,
    workspace_id
)?;
```

---

## Error Handling

### Busy Timeout

With `busy_timeout(5000)`, SQLite waits up to 5 seconds for locks:

```rust
match db.conn.execute("INSERT ...", params![...]) {
    Ok(_) => { /* success */ }
    Err(rusqlite::Error::SqliteFailure(err, _))
        if err.code == rusqlite::ErrorCode::DatabaseBusy => {
        // Still busy after 5s - handle appropriately
        warn!("Database locked after 5s timeout");
    }
    Err(e) => return Err(e.into()),
}
```

### Corruption Detection

Corruption is detected at connection open via SQLite integrity checks:

```rust
// Automatic in SymbolDatabase::new()
db.check_integrity()?;
```

If corruption detected:
1. Runs SQLite integrity check
2. If database is malformed, deletes and recreates from scratch
3. Re-indexing rebuilds all data

---

## Testing Requirements

### Every Database Test MUST:

1. **Use temp directories**
   ```rust
   let temp_dir = TempDir::new()?;
   let db_path = temp_dir.path().join("test.db");
   ```

2. **Verify WAL mode if testing connection behavior**
   ```rust
   let mode: String = db.conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
   assert_eq!(mode.to_lowercase(), "wal");
   ```

3. **Clean up via Drop** (automatic with temp_dir)

### Corruption Prevention Tests

See `src/tests/core/database.rs`:
- `test_wal_mode_set_immediately_on_connection_open()` - Verifies WAL setup
- `test_database_drop_checkpoints_wal()` - Verifies cleanup
- `test_schema_version_downgrade_detection()` - Verifies version safety

---

## Reference Workspaces

Each reference workspace has **separate physical database**:
- Primary: `.julie/indexes/julie_316c0b08/db/symbols.db`
- Reference: `.julie/indexes/ref_workspace_id/db/symbols.db`

**Opening reference workspace DBs**:
```rust
// ✅ CORRECT: Via SymbolDatabase::new()
let ref_db_path = workspace.workspace_db_path(&ref_workspace_id);
let ref_db = tokio::task::spawn_blocking(
    move || SymbolDatabase::new(ref_db_path)
).await??;
```

**Workspace isolation is at FILE level**, not query level. Each database connection is locked to one workspace's physical file.

---

## Performance Considerations

### WAL Mode Benefits
- **Concurrent readers**: Multiple processes can read simultaneously
- **Better performance**: Writers don't block readers
- **Crash recovery**: WAL provides point-in-time recovery

### WAL Mode Tradeoffs
- **File system support**: Requires filesystems that support WAL (NTFS, ext4, APFS)
- **WAL file growth**: Mitigated by `wal_autocheckpoint(2000)`
- **Checkpoint overhead**: Mitigated by `synchronous=NORMAL`

### Synchronous Modes

| Mode | Safety | Speed | Use Case |
|------|--------|-------|----------|
| FULL | Maximum | Slowest | Production critical data |
| **NORMAL** | **Safe with WAL** | **Fast** | **Julie (current)** |
| OFF | Data loss risk | Fastest | Temp/cache only |

---

## Debugging Corruption

### If Corruption Occurs

1. **Check journal mode**:
   ```bash
   sqlite3 symbols.db "PRAGMA journal_mode;"
   ```
   Should return `wal`.

2. **Check integrity**:
   ```bash
   sqlite3 symbols.db "PRAGMA integrity_check;"
   ```

3. **Rebuild indexes**:
   ```bash
   rm -rf .julie/indexes/
   # Restart Julie to rebuild
   ```

4. **Check for concurrent access**:
   ```bash
   # On Linux/macOS
   lsof symbols.db

   # On Windows
   handle.exe symbols.db
   ```

### Log Analysis

Look for these indicators:
- ✅ `WAL mode enabled on database connection`
- ✅ `WAL checkpoint complete: busy=0, log=X, checkpointed=X`
- ⚠️ `Journal mode 'delete' detected` (should force WAL)
- 🚨 `Failed to enable WAL mode` (filesystem issue)

---

## Common Pitfalls

### ❌ DON'T: Open raw connections
```rust
// WRONG - misses WAL setup
let conn = Connection::open(path)?;
```

### ❌ DON'T: Set WAL mode late
```rust
// WRONG - migrations run in DELETE mode
db.run_migrations()?;
db.set_wal_mode()?;  // Too late!
```

### ❌ DON'T: Share connections across threads without synchronization
```rust
// WRONG - Connection is not Send
let conn = Connection::open(path)?;
thread::spawn(move || conn.execute(...))?;
```

### ✅ DO: Use Arc<Mutex<SymbolDatabase>>
```rust
// CORRECT
let db = Arc::new(Mutex::new(SymbolDatabase::new(path)?));
```

### ❌ DON'T: Forget to checkpoint on shutdown
```rust
// WRONG - WAL might have uncommitted changes
drop(db);  // No checkpoint!
```

### ✅ DO: Let Drop impl handle it (automatic)
```rust
// CORRECT - Drop impl checkpoints automatically
{
    let db = SymbolDatabase::new(path)?;
    // Use db...
}  // Drop checkpoints WAL here
```

---

## Checklist for New Database Code

Before merging any PR that touches database code:

- [ ] Connection opened via `SymbolDatabase::new()` (not raw `Connection::open()`)
- [ ] WAL mode verified in tests (if testing connection behavior)
- [ ] Multi-statement operations wrapped in transactions
- [ ] Busy timeout handled appropriately
- [ ] No blocking I/O in async context (use `spawn_blocking`)
- [ ] Tests use temp directories (not real `.julie/` folder)
- [ ] Schema changes include migration + version bump
- [ ] Drop impl cleanup verified for new database wrappers

---

## Further Reading

- SQLite WAL Mode: https://www.sqlite.org/wal.html
- SQLite Pragma: https://www.sqlite.org/pragma.html
- Rusqlite Documentation: https://docs.rs/rusqlite/
- Julie Architecture: `docs/ARCHITECTURE.md`
- Workspace Isolation: `docs/WORKSPACE_ARCHITECTURE.md`

---

**Remember**: SQLite corruption is **100% preventable** with proper patterns. These guidelines are not optional—they're the result of debugging daily corruption issues and implementing proven fixes.
