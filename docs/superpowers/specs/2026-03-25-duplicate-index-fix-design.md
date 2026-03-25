# Fix: Duplicate Workspace Index Directories

**Date:** 2026-03-25
**Status:** Approved
**Scope:** Daemon startup migration, upsert fix, clean command enhancement

## Problem

The v6.0.4 deep review correctly fixed `normalize_path` to only lowercase paths on
Windows (macOS/Linux paths are case-sensitive). However, the daemon database was never
migrated. This causes duplicate index directories for every workspace:

- **Old behavior:** `/Users/murphy/source/julie` lowercased to `/users/murphy/source/julie`, hash `316c0b08`
- **New behavior:** `/Users/murphy/source/julie` kept as-is, hash `528d4264`

Every daemon restart generates new workspace IDs. The `upsert_workspace` call fails
silently (`let _ =` discards the UNIQUE constraint error on `path`), but `init_workspace`
creates new directories anyway. Result: two full copies of every index, with the daemon DB
pointing to stale old-hash entries while the code actually uses new-hash directories.

Additionally, the `clean` command only checks DB entries against disk paths. It never
scans `~/.julie/indexes/` for orphan directories not tracked in the DB.

## Design

### 1. Startup Migration

**Where:** Early in `run_daemon` (src/daemon/mod.rs), after opening daemon.db and
computing `DaemonPaths` (which provides `indexes_dir`), before creating the
`WorkspacePool` or accepting sessions.

**FK constraint:** The daemon DB enables `PRAGMA foreign_keys=ON`. Child tables
(`workspace_references`, `codehealth_snapshots`, `tool_calls`) reference
`workspaces(workspace_id)` with `ON DELETE CASCADE` but no `ON UPDATE CASCADE`.
Deleting a workspace row would cascade-delete all child data. Updating workspace_id
directly would violate FK constraints. The migration must update children first.

**Logic:**

```
// Phase 1: Compute all ID mappings before touching anything
id_map: HashMap<old_id, new_id> = {}
for each workspace entry in daemon.db:
    regenerated_id = generate_workspace_id(entry.path)
    if regenerated_id != entry.workspace_id:
        id_map[entry.workspace_id] = regenerated_id

if id_map is empty:
    return  // nothing to migrate

// Phase 2: Rename/delete index directories on disk
for (old_id, new_id) in id_map:
    old_dir = indexes_dir / old_id
    new_dir = indexes_dir / new_id

    if old_dir exists AND new_dir does NOT exist:
        rename old_dir -> new_dir
    else if old_dir exists AND new_dir exists:
        // Both populated (old stale + new active). Delete the old one.
        delete old_dir
    // If disk operation fails, log warning and skip DB update for this entry.

// Phase 3: Update DB in a single transaction
//   Temporarily disable FK checks so we can update the PK freely,
//   then re-enable. This is safe because we're updating all references
//   in the same transaction.
BEGIN TRANSACTION;
PRAGMA foreign_keys = OFF;

for (old_id, new_id) in id_map:
    UPDATE workspace_references SET primary_workspace_id = new_id
        WHERE primary_workspace_id = old_id;
    UPDATE workspace_references SET reference_workspace_id = new_id
        WHERE reference_workspace_id = old_id;
    UPDATE codehealth_snapshots SET workspace_id = new_id
        WHERE workspace_id = old_id;
    UPDATE tool_calls SET workspace_id = new_id
        WHERE workspace_id = old_id;
    UPDATE workspaces SET workspace_id = new_id
        WHERE workspace_id = old_id;

PRAGMA foreign_keys = ON;
-- Run integrity check to verify FK consistency
PRAGMA foreign_key_check;
COMMIT;
```

Also clean up any root-path workspace entry where `path = "/"`, which is a stale
artifact from an erroneous initialization. Delete its DB row and disk directory.

**Failure handling:**
- If a disk rename/delete fails, log a warning and skip the DB update for that entry.
  The next daemon restart will retry.
- If the DB transaction fails, roll back entirely. Old IDs remain; the next restart
  will retry. No partial state.

**Properties:**
- Runs once per daemon startup
- Idempotent: if all IDs already match, it's a no-op
- Logs all migrations so the user can see what happened

### 2. Fix `upsert_workspace` SQL

**Current:** `ON CONFLICT(workspace_id) DO UPDATE SET ...`

This only handles workspace_id conflicts. If the same path arrives with a new
workspace_id, the UNIQUE constraint on `path` causes a hard error.

**Fix:** Change the conflict target to `path`, but do NOT update `workspace_id` in
the upsert. Updating the PK would violate FK constraints from child tables, and
the startup migration already handles ID reconciliation. The upsert just needs to
not crash:

```sql
INSERT INTO workspaces (workspace_id, path, status, session_count,
    created_at, updated_at)
VALUES (?1, ?2, ?3, 0, ?4, ?4)
ON CONFLICT(path) DO UPDATE SET
    status     = excluded.status,
    updated_at = excluded.updated_at
```

If the path already exists (even with a different workspace_id), this updates the
status without touching the ID. The startup migration is the only code path that
changes workspace IDs (with proper FK handling).

**Also:** Remove `let _ =` in `get_or_init` (workspace_pool.rs) and replace with
`if let Err(e) = ... { warn!(...) }` so upsert failures are visible in logs.

### 3. Enhanced `clean` Command

**Current behavior:** Iterates DB entries, removes rows where the project path no
longer exists on disk. Never looks at the indexes directory.

**Add orphan directory scanning:**

```
registered_ids = set of all workspace_id values from daemon.db
disk_dirs = list all directories in indexes_dir

for each dir in disk_dirs:
    if dir.name not in registered_ids:
        delete dir
        report as orphan cleanup
```

**The clean command does three passes:**
1. **Stale DB entries:** path no longer exists on disk, delete DB row (cascades to children)
2. **Orphan directories:** directory not tracked in DB, delete directory
3. Report results

This is the safety net. Startup migration prevents new orphans; clean catches
anything that slips through (old stdio-mode directories, failed registrations, etc.).

**Note:** The `clean` command is user-triggered and unlikely to race with session
startup. No special concurrency handling needed.

## Files to Modify

| File | Change |
|------|--------|
| `src/daemon/mod.rs` | Add startup migration after daemon.db opens, using `paths.indexes_dir()` for disk ops |
| `src/daemon/database.rs` | Add `migrate_workspace_ids` batch method (handles FK updates in transaction); change `upsert_workspace` conflict target to `path` |
| `src/daemon/workspace_pool.rs` | Replace `let _ =` with proper error logging on upsert |
| `src/tools/workspace/commands/registry/list_clean.rs` | Add orphan directory scanning pass; needs access to indexes_dir |
| `src/tests/daemon/database.rs` (or new) | Tests for migration and updated upsert |
| `src/tests/tools/workspace/` (or new) | Tests for orphan cleanup logic |

## Testing

- **Unit test:** `migrate_workspace_ids` correctly updates workspace_id in all tables
  (workspaces, workspace_references, codehealth_snapshots, tool_calls), preserves stats
- **Unit test:** `migrate_workspace_ids` with workspace_references rows referencing
  the migrated ID (both primary and reference columns)
- **Unit test:** `upsert_workspace` with path conflict does not crash, updates status
  without changing workspace_id
- **Unit test:** Orphan directory detection identifies dirs not in DB
- **Unit test:** Migration is idempotent (running twice produces same result)
- **Unit test:** Migration skips entries where disk rename fails (partial failure)
