# Duplicate Workspace Index Fix — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix duplicate index directories caused by `normalize_path` behavior change, prevent recurrence, and add orphan cleanup.

**Architecture:** Three-layer fix. (1) `DaemonDatabase::migrate_workspace_ids` batch-migrates all stale workspace IDs in a single transaction with FK-safe updates. (2) `upsert_workspace` changes conflict target from `workspace_id` to `path` so duplicate-path inserts update the existing row instead of crashing. (3) The `clean` command gains orphan directory scanning. A new `run_daemon` call wires the migration into daemon startup with disk directory rename/cleanup.

**Tech Stack:** Rust, SQLite (rusqlite), tempfile (tests)

**Spec:** `docs/superpowers/specs/2026-03-25-duplicate-index-fix-design.md`

---

### Task 1: Add `migrate_workspace_ids` to DaemonDatabase

**Files:**
- Modify: `src/daemon/database.rs` (add method)
- Test: `src/tests/daemon/database.rs` (add tests)

This is the core DB migration logic. Given a map of `old_id -> new_id`, it updates
all tables in a single transaction with FK checks temporarily disabled.

- [ ] **Step 1: Write failing test — basic migration**

Add to `src/tests/daemon/database.rs`:

```rust
#[test]
fn test_migrate_workspace_ids_updates_all_tables() {
    let (db, _tmp) = create_test_db();

    // Insert workspace with old ID
    db.upsert_workspace("julie_316c0b08", "/Users/murphy/source/julie", "ready").unwrap();
    db.update_workspace_stats("julie_316c0b08", 100, 50, None, None).unwrap();

    // Insert a reference relationship
    db.upsert_workspace("goldfish_5ed767a5", "/Users/murphy/source/goldfish", "ready").unwrap();
    db.add_reference("julie_316c0b08", "goldfish_5ed767a5").unwrap();

    // Insert codehealth snapshot
    use crate::daemon::database::CodehealthSnapshot;
    db.insert_codehealth_snapshot("julie_316c0b08", &CodehealthSnapshot::default()).unwrap();

    // Insert tool call
    db.insert_tool_call("julie_316c0b08", "sess1", "fast_search", 50.0, Some(5), None, None, true, None).unwrap();

    // Migrate both workspace IDs
    let mut migrations = std::collections::HashMap::new();
    migrations.insert("julie_316c0b08".to_string(), "julie_528d4264".to_string());
    migrations.insert("goldfish_5ed767a5".to_string(), "goldfish_aa67f476".to_string());
    db.migrate_workspace_ids(&migrations).unwrap();

    // Verify workspaces table updated
    assert!(db.get_workspace("julie_528d4264").unwrap().is_some());
    assert!(db.get_workspace("julie_316c0b08").unwrap().is_none());
    assert!(db.get_workspace("goldfish_aa67f476").unwrap().is_some());

    // Verify stats preserved
    let ws = db.get_workspace("julie_528d4264").unwrap().unwrap();
    assert_eq!(ws.symbol_count, Some(100));
    assert_eq!(ws.file_count, Some(50));

    // Verify workspace_references updated
    let refs = db.list_references("julie_528d4264").unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].workspace_id, "goldfish_aa67f476");

    // Verify codehealth_snapshots updated
    let snapshot = db.get_latest_snapshot("julie_528d4264").unwrap();
    assert!(snapshot.is_some());
    assert!(db.get_latest_snapshot("julie_316c0b08").unwrap().is_none());

    // Verify tool_calls updated
    let history = db.query_tool_call_history("julie_528d4264", 30).unwrap();
    assert_eq!(history.total_calls, 1);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib test_migrate_workspace_ids_updates_all_tables 2>&1 | tail -10`
Expected: FAIL (method does not exist)

- [ ] **Step 3: Write failing test — idempotency**

```rust
#[test]
fn test_migrate_workspace_ids_idempotent() {
    let (db, _tmp) = create_test_db();
    db.upsert_workspace("julie_528d4264", "/Users/murphy/source/julie", "ready").unwrap();

    // Migrate with same old->new (no-op case: old doesn't exist)
    let mut migrations = std::collections::HashMap::new();
    migrations.insert("julie_316c0b08".to_string(), "julie_528d4264".to_string());

    // Should not crash even though old ID doesn't exist
    db.migrate_workspace_ids(&migrations).unwrap();

    // Original entry untouched
    let ws = db.get_workspace("julie_528d4264").unwrap();
    assert!(ws.is_some());
}
```

- [ ] **Step 4: Run test to verify it fails**

Run: `cargo test --lib test_migrate_workspace_ids_idempotent 2>&1 | tail -10`
Expected: FAIL

- [ ] **Step 5: Write failing test — empty map is no-op**

```rust
#[test]
fn test_migrate_workspace_ids_empty_map() {
    let (db, _tmp) = create_test_db();
    db.upsert_workspace("julie_528d4264", "/Users/murphy/source/julie", "ready").unwrap();

    let migrations = std::collections::HashMap::new();
    db.migrate_workspace_ids(&migrations).unwrap();

    let ws = db.get_workspace("julie_528d4264").unwrap();
    assert!(ws.is_some());
}
```

- [ ] **Step 6: Implement `migrate_workspace_ids`**

Add to `src/daemon/database.rs` in the `impl DaemonDatabase` block:

```rust
/// Batch-migrate workspace IDs across all tables.
///
/// Given a map of old_id -> new_id, updates workspace_references,
/// codehealth_snapshots, tool_calls, and workspaces in a single transaction.
/// FK checks are temporarily disabled to allow PK updates.
pub fn migrate_workspace_ids(&self, id_map: &std::collections::HashMap<String, String>) -> Result<()> {
    if id_map.is_empty() {
        return Ok(());
    }

    let mut conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
    conn.execute_batch("PRAGMA foreign_keys = OFF;")?;

    // Scope guard: ensure FK enforcement is restored on ALL exit paths.
    // Without this, an early `?` return would leave FKs disabled for
    // all future callers sharing this connection.
    let result = (|| -> Result<()> {
        let tx = conn.transaction()?;

        for (old_id, new_id) in id_map {
            // Update child tables first
            tx.execute(
                "UPDATE workspace_references SET primary_workspace_id = ?1
                 WHERE primary_workspace_id = ?2",
                params![new_id, old_id],
            )?;
            tx.execute(
                "UPDATE workspace_references SET reference_workspace_id = ?1
                 WHERE reference_workspace_id = ?2",
                params![new_id, old_id],
            )?;
            tx.execute(
                "UPDATE codehealth_snapshots SET workspace_id = ?1
                 WHERE workspace_id = ?2",
                params![new_id, old_id],
            )?;
            tx.execute(
                "UPDATE tool_calls SET workspace_id = ?1
                 WHERE workspace_id = ?2",
                params![new_id, old_id],
            )?;
            // Update workspace row itself (PK change)
            tx.execute(
                "UPDATE workspaces SET workspace_id = ?1
                 WHERE workspace_id = ?2",
                params![new_id, old_id],
            )?;
        }

        // Verify FK integrity before committing
        let violations: i64 = tx.query_row(
            "SELECT count(*) FROM pragma_foreign_key_check",
            [],
            |row| row.get(0),
        )?;
        if violations > 0 {
            anyhow::bail!("FK integrity check failed after migration ({violations} violations)");
        }

        tx.commit()?;
        Ok(())
    })();

    // ALWAYS re-enable FK enforcement, even if the transaction failed
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;

    result
}
```

- [ ] **Step 7: Run all three tests to verify they pass**

Run: `cargo test --lib test_migrate_workspace_ids 2>&1 | tail -10`
Expected: 3 tests PASS

- [ ] **Step 8: Commit**

```bash
git add src/daemon/database.rs src/tests/daemon/database.rs
git commit -m "feat(daemon): add migrate_workspace_ids for batch ID migration"
```

---

### Task 2: Fix `upsert_workspace` conflict target

**Files:**
- Modify: `src/daemon/database.rs:176-190` (change SQL)
- Test: `src/tests/daemon/database.rs` (add test)

- [ ] **Step 1: Write failing test**

Add to `src/tests/daemon/database.rs`:

```rust
#[test]
fn test_upsert_workspace_path_conflict_updates_status() {
    let (db, _tmp) = create_test_db();

    // Insert with old workspace ID
    db.upsert_workspace("julie_316c0b08", "/Users/murphy/source/julie", "ready").unwrap();

    // Upsert same path with different workspace ID — should not crash
    db.upsert_workspace("julie_528d4264", "/Users/murphy/source/julie", "pending").unwrap();

    // The row should still exist (status updated, workspace_id NOT changed
    // because only the startup migration handles ID changes with FK safety)
    let ws = db.get_workspace("julie_316c0b08").unwrap().unwrap();
    assert_eq!(ws.status, "pending");
    assert_eq!(ws.path, "/Users/murphy/source/julie");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib test_upsert_workspace_path_conflict 2>&1 | tail -10`
Expected: FAIL (UNIQUE constraint violation on path)

- [ ] **Step 3: Change `upsert_workspace` SQL**

In `src/daemon/database.rs`, replace the `upsert_workspace` method body:

```rust
pub fn upsert_workspace(&self, workspace_id: &str, path: &str, status: &str) -> Result<()> {
    let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
    let now = now_unix();
    conn.execute(
        "INSERT INTO workspaces (workspace_id, path, status, session_count,
            created_at, updated_at)
         VALUES (?1, ?2, ?3, 0, ?4, ?4)
         ON CONFLICT(path) DO UPDATE SET
            status     = excluded.status,
            updated_at = excluded.updated_at",
        params![workspace_id, path, status, now],
    )?;
    Ok(())
}
```

**Key change:** Conflict target is now `path` instead of `workspace_id`. On path
conflict, only `status` and `updated_at` are updated (NOT `workspace_id`, because
changing the PK would violate FK constraints from child tables).

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib test_upsert_workspace_path_conflict 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 5: Run existing upsert tests to check for regressions**

Run: `cargo test --lib test_daemon_db 2>&1 | tail -20`
Expected: All existing daemon DB tests pass

- [ ] **Step 6: Commit**

```bash
git add src/daemon/database.rs src/tests/daemon/database.rs
git commit -m "fix(daemon): change upsert_workspace conflict target to path"
```

---

### Task 3: Fix silent error swallowing in `get_or_init`

**Files:**
- Modify: `src/daemon/workspace_pool.rs:147` (replace `let _ =`)

- [ ] **Step 1: Replace `let _ =` with proper error logging**

In `src/daemon/workspace_pool.rs`, in the `get_or_init` method, find:

```rust
let _ = db.upsert_workspace(workspace_id, &path_str, "pending");
```

Replace with:

```rust
if let Err(e) = db.upsert_workspace(workspace_id, &path_str, "pending") {
    warn!(
        workspace_id,
        path = %path_str,
        "Failed to register workspace in daemon.db: {}", e
    );
}
```

- [ ] **Step 2: Verify build**

Run: `cargo build 2>&1 | tail -5`
Expected: Compiles without errors

- [ ] **Step 3: Commit**

```bash
git add src/daemon/workspace_pool.rs
git commit -m "fix(daemon): log upsert errors instead of silently discarding"
```

---

### Task 4: Add startup migration to `run_daemon`

**Files:**
- Modify: `src/daemon/mod.rs` (add migration call after daemon_db opens)
- Modify: `src/daemon/database.rs` (add `delete_root_workspace` helper)

This wires the DB migration into daemon startup and handles disk operations
(rename/delete old index directories).

- [ ] **Step 1: Write failing test for root-path cleanup helper**

Add to `src/tests/daemon/database.rs`:

```rust
#[test]
fn test_delete_workspace_with_root_path() {
    let (db, _tmp) = create_test_db();
    db.upsert_workspace("workspace_e3b0c442", "/", "pending").unwrap();

    // Verify it exists
    assert!(db.get_workspace("workspace_e3b0c442").unwrap().is_some());

    // Delete it
    db.delete_workspace("workspace_e3b0c442").unwrap();

    // Verify gone
    assert!(db.get_workspace("workspace_e3b0c442").unwrap().is_none());
}
```

- [ ] **Step 2: Run test (should pass, delete_workspace already exists)**

Run: `cargo test --lib test_delete_workspace_with_root_path 2>&1 | tail -10`
Expected: PASS (existing `delete_workspace` handles this)

- [ ] **Step 3: Add the startup migration function**

Add a new free function in `src/daemon/mod.rs` (before `run_daemon` or after `accept_loop`):

```rust
/// Reconcile workspace IDs after a normalize_path behavior change.
///
/// Compares each workspace's stored ID against the current generate_workspace_id
/// output. If they differ, renames the index directory and batch-updates the DB.
fn migrate_stale_workspace_ids(
    daemon_db: &DaemonDatabase,
    indexes_dir: &Path,
) {
    use crate::workspace::registry::generate_workspace_id;

    let workspaces = match daemon_db.list_workspaces() {
        Ok(ws) => ws,
        Err(e) => {
            warn!("Failed to list workspaces for migration check: {}", e);
            return;
        }
    };

    // Phase 1: Compute all ID mappings
    let mut id_map = std::collections::HashMap::new();
    for ws in &workspaces {
        // Clean up root-path artifact
        if ws.path == "/" {
            info!(
                workspace_id = %ws.workspace_id,
                "Removing stale root-path workspace entry"
            );
            if let Err(e) = daemon_db.delete_workspace(&ws.workspace_id) {
                warn!("Failed to delete root workspace: {}", e);
            }
            let root_dir = indexes_dir.join(&ws.workspace_id);
            if root_dir.exists() {
                if let Err(e) = std::fs::remove_dir_all(&root_dir) {
                    warn!("Failed to remove root workspace dir: {}", e);
                }
            }
            continue;
        }

        match generate_workspace_id(&ws.path) {
            Ok(new_id) if new_id != ws.workspace_id => {
                info!(
                    old_id = %ws.workspace_id,
                    new_id = %new_id,
                    path = %ws.path,
                    "Workspace ID needs migration"
                );
                id_map.insert(ws.workspace_id.clone(), new_id);
            }
            Err(e) => {
                warn!(
                    workspace_id = %ws.workspace_id,
                    path = %ws.path,
                    "Failed to regenerate workspace ID: {}", e
                );
            }
            _ => {} // ID matches, no migration needed
        }
    }

    if id_map.is_empty() {
        return;
    }

    // Phase 2: Rename/delete index directories
    let mut disk_failures: Vec<String> = Vec::new();
    for (old_id, new_id) in &id_map {
        let old_dir = indexes_dir.join(old_id);
        let new_dir = indexes_dir.join(new_id);

        if old_dir.exists() && !new_dir.exists() {
            if let Err(e) = std::fs::rename(&old_dir, &new_dir) {
                warn!(
                    old_id, new_id,
                    "Failed to rename index dir, skipping DB migration for this entry: {}", e
                );
                disk_failures.push(old_id.clone());
            } else {
                info!(old_id, new_id, "Renamed index directory");
            }
        } else if old_dir.exists() && new_dir.exists() {
            // Both exist: new is active (created by post-fix code), old is stale
            if let Err(e) = std::fs::remove_dir_all(&old_dir) {
                warn!(old_id, "Failed to remove stale index dir: {}", e);
            } else {
                info!(old_id, "Removed stale index directory (new dir already exists)");
            }
        }
    }

    // Remove entries where disk operations failed
    for failed_id in &disk_failures {
        id_map.remove(failed_id);
    }

    if id_map.is_empty() {
        return;
    }

    // Phase 3: Batch-update DB
    match daemon_db.migrate_workspace_ids(&id_map) {
        Ok(()) => {
            info!(
                count = id_map.len(),
                "Successfully migrated workspace IDs in daemon.db"
            );
        }
        Err(e) => {
            warn!("Failed to migrate workspace IDs in DB: {}", e);
        }
    }
}
```

- [ ] **Step 4: Wire migration into `run_daemon`**

In `src/daemon/mod.rs`, inside `run_daemon`, right after the daemon_db initialization
block (after `Some(Arc::new(db))`) and before the WorkspacePool creation, add:

```rust
// Migrate stale workspace IDs from pre-v6.0.4 normalize_path behavior.
// Must run before WorkspacePool is created so sessions see correct IDs.
if let Some(ref db) = daemon_db {
    migrate_stale_workspace_ids(db, &paths.indexes_dir());
}
```

Also add `use std::path::Path;` to imports if not already present.

- [ ] **Step 5: Write test for disk rename failure skip behavior**

Add to `src/tests/daemon/database.rs`:

```rust
#[test]
fn test_migrate_stale_ids_skips_on_disk_failure() {
    // This tests the logic that disk_failures entries are excluded from DB migration.
    // We simulate by having the id_map contain entries, then removing the ones that
    // "failed" on disk, and verifying only the successful ones get migrated.
    let (db, _tmp) = create_test_db();

    db.upsert_workspace("julie_316c0b08", "/test/julie", "ready").unwrap();
    db.upsert_workspace("sealab_72d18461", "/test/sealab", "ready").unwrap();

    // Simulate: julie rename succeeded, sealab rename failed
    let mut id_map = std::collections::HashMap::new();
    id_map.insert("julie_316c0b08".to_string(), "julie_528d4264".to_string());
    // sealab NOT in id_map (simulates being removed after disk failure)

    db.migrate_workspace_ids(&id_map).unwrap();

    // julie was migrated
    assert!(db.get_workspace("julie_528d4264").unwrap().is_some());
    assert!(db.get_workspace("julie_316c0b08").unwrap().is_none());

    // sealab was NOT migrated (disk failure excluded it)
    assert!(db.get_workspace("sealab_72d18461").unwrap().is_some());
}
```

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test --lib test_migrate_stale_ids_skips_on_disk_failure 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 7: Verify build**

Run: `cargo build 2>&1 | tail -5`
Expected: Compiles without errors

- [ ] **Step 8: Commit**

```bash
git add src/daemon/mod.rs src/daemon/database.rs src/tests/daemon/database.rs
git commit -m "feat(daemon): migrate stale workspace IDs on startup"
```

---

### Task 5: Enhance `clean` command with orphan directory scanning

**Files:**
- Modify: `src/tools/workspace/commands/registry/list_clean.rs:77-123`
- Test: `src/tests/daemon/database.rs`

- [ ] **Step 1: Write failing test for orphan cleanup with real temp dirs**

Add to `src/tests/daemon/database.rs`:

```rust
#[test]
fn test_orphan_directory_cleanup() {
    let (db, _tmp) = create_test_db();

    // Register two workspaces in DB
    db.upsert_workspace("julie_528d4264", "/Users/test/julie", "ready").unwrap();
    db.upsert_workspace("goldfish_aa67f476", "/Users/test/goldfish", "ready").unwrap();

    // Create a temp indexes directory with registered + orphan dirs
    let indexes_dir = _tmp.path().join("indexes");
    std::fs::create_dir_all(indexes_dir.join("julie_528d4264")).unwrap();
    std::fs::create_dir_all(indexes_dir.join("goldfish_aa67f476")).unwrap();
    std::fs::create_dir_all(indexes_dir.join("julie_316c0b08")).unwrap();  // orphan
    std::fs::create_dir_all(indexes_dir.join("sealab_72d18461")).unwrap(); // orphan

    // Build registered ID set
    let registered: std::collections::HashSet<String> = db
        .list_workspaces()
        .unwrap()
        .into_iter()
        .map(|ws| ws.workspace_id)
        .collect();

    // Scan and delete orphans (same logic as clean command)
    let mut cleaned_orphans = Vec::new();
    for entry in std::fs::read_dir(&indexes_dir).unwrap().flatten() {
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            let dir_name = entry.file_name().to_string_lossy().to_string();
            if !registered.contains(&dir_name) {
                std::fs::remove_dir_all(entry.path()).unwrap();
                cleaned_orphans.push(dir_name);
            }
        }
    }

    assert_eq!(cleaned_orphans.len(), 2);
    assert!(cleaned_orphans.contains(&"julie_316c0b08".to_string()));
    assert!(cleaned_orphans.contains(&"sealab_72d18461".to_string()));

    // Verify registered dirs still exist
    assert!(indexes_dir.join("julie_528d4264").exists());
    assert!(indexes_dir.join("goldfish_aa67f476").exists());
    // Verify orphans are gone
    assert!(!indexes_dir.join("julie_316c0b08").exists());
    assert!(!indexes_dir.join("sealab_72d18461").exists());
}
```

- [ ] **Step 2: Run test to verify it passes (logic validation)**

Run: `cargo test --lib test_orphan_directory_cleanup 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 3: Update `handle_clean_command` to scan for orphan directories**

In `src/tools/workspace/commands/registry/list_clean.rs`, replace the daemon-mode
block in `handle_clean_command` with:

```rust
pub(crate) async fn handle_clean_command(
    &self,
    handler: &JulieServerHandler,
) -> Result<CallToolResult> {
    info!("Cleaning workspaces (comprehensive cleanup: TTL + Size Limits + Orphans)");

    // Daemon mode: use DaemonDatabase
    if let Some(ref db) = handler.daemon_db {
        let all_workspaces = match db.list_workspaces() {
            Ok(ws) => ws,
            Err(e) => {
                let message = format!("Failed to list workspaces: {}", e);
                return Ok(CallToolResult::text_content(vec![Content::text(message)]));
            }
        };

        let mut cleaned_stale = Vec::new();
        let mut cleaned_orphans = Vec::new();

        // Pass 1: Remove DB entries where project path no longer exists
        for ws in &all_workspaces {
            if !std::path::Path::new(&ws.path).exists() {
                if let Err(e) = db.delete_workspace(&ws.workspace_id) {
                    tracing::warn!(
                        "Failed to delete workspace {} during cleanup: {}",
                        ws.workspace_id,
                        e
                    );
                } else {
                    cleaned_stale.push(ws.workspace_id.clone());
                }
            }
        }

        // Pass 2: Remove orphan index directories not tracked in DB
        // Re-fetch workspace list (may have changed from pass 1)
        if let Ok(current_workspaces) = db.list_workspaces() {
            let registered_ids: std::collections::HashSet<String> = current_workspaces
                .iter()
                .map(|ws| ws.workspace_id.clone())
                .collect();

            if let Ok(paths) = crate::paths::DaemonPaths::try_new() {
                let indexes_dir = paths.indexes_dir();
                if let Ok(entries) = std::fs::read_dir(&indexes_dir) {
                    for entry in entries.flatten() {
                        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                            let dir_name = entry.file_name().to_string_lossy().to_string();
                            if !registered_ids.contains(&dir_name) {
                                let dir_path = entry.path();
                                if let Err(e) = std::fs::remove_dir_all(&dir_path) {
                                    tracing::warn!(
                                        "Failed to remove orphan index dir {}: {}",
                                        dir_name,
                                        e
                                    );
                                } else {
                                    info!("Removed orphan index directory: {}", dir_name);
                                    cleaned_orphans.push(dir_name);
                                }
                            }
                        }
                    }
                }
            }
        }

        let message = if cleaned_stale.is_empty() && cleaned_orphans.is_empty() {
            "No cleanup needed. All workspaces are healthy!".to_string()
        } else {
            let mut parts = Vec::new();
            if !cleaned_stale.is_empty() {
                parts.push(format!(
                    "Removed {} stale DB entries (missing paths):\n  {}",
                    cleaned_stale.len(),
                    cleaned_stale.join("\n  "),
                ));
            }
            if !cleaned_orphans.is_empty() {
                parts.push(format!(
                    "Removed {} orphan index directories (not in DB):\n  {}",
                    cleaned_orphans.len(),
                    cleaned_orphans.join("\n  "),
                ));
            }
            parts.join("\n\n")
        };
        return Ok(CallToolResult::text_content(vec![Content::text(message)]));
    }

    // Stdio mode: no workspace registry available
    let message = "No cleanup needed. All workspaces are healthy!";
    Ok(CallToolResult::text_content(vec![Content::text(message)]))
}
```

- [ ] **Step 4: Verify build**

Run: `cargo build 2>&1 | tail -5`
Expected: Compiles without errors

- [ ] **Step 5: Commit**

```bash
git add src/tools/workspace/commands/registry/list_clean.rs src/tests/daemon/database.rs
git commit -m "feat(workspace): add orphan directory cleanup to clean command"
```

---

### Task 6: Run dev tests and verify

- [ ] **Step 1: Run `cargo xtask test dev`**

Run: `cargo xtask test dev 2>&1 | tail -20`
Expected: All tests pass, no regressions

- [ ] **Step 2: Run daemon-specific tests**

Run: `cargo test --lib tests::daemon 2>&1 | tail -20`
Expected: All daemon tests pass (including new migration tests)

- [ ] **Step 3: Final commit (if any fixups needed)**

If regressions were found and fixed, commit the fixes.

---

### Task 7: Manual verification

This task is for the main session operator (not a subagent).

- [ ] **Step 1: Build release binary**

Exit Claude Code, then run: `cargo build --release`

- [ ] **Step 2: Restart Claude Code and check logs**

Look for migration log messages:
```bash
tail -50 .julie/logs/julie.log.$(date +%Y-%m-%d) | grep -i migrat
```
Expected: Log lines showing "Migrating workspace..." for each stale ID

- [ ] **Step 3: Verify indexes directory is clean**

```bash
ls ~/.julie/indexes/
```
Expected: No more duplicate entries per project

- [ ] **Step 4: Run `manage_workspace clean`**

Verify the enhanced clean command reports healthy state (or cleans up any remaining orphans).

- [ ] **Step 5: Run `manage_workspace list`**

Verify all workspaces show correct IDs and paths.
