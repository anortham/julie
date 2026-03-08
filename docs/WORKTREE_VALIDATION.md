# Worktree & Parallel Validation Results

**Date:** 2026-03-08
**Daemon version:** v4.0 (post-fix, commit TBD)
**Platform:** macOS (darwin aarch64)

## Scenario A: Multiple Git Worktrees

| Test | Result | Notes |
|------|--------|-------|
| A1. Create worktree | PASS | `git worktree add` works normally |
| A2. Register both in daemon | PASS | `POST /api/projects` returns 201 Created |
| A3. Separate workspace IDs | PASS | `julie_316c0b08` vs `julie-worktree-test_7388aa25` |
| A4. Own index (correct stats) | PASS | 1059 files, 33098 symbols (matches primary) |
| A5. Search scoped correctly | PASS | Concurrent search on both returns correct results |
| A6. Cleanup — no orphans | PASS | DELETE removes project, watcher stops, count drops to 1 |

### Bug Found & Fixed

**`find_workspace_root` walked past worktree `.git` into `~/.julie/`** — worktrees have a `.git` file (not directory) pointing to the main repo. The walker found `~/.julie/` (daemon home) as the workspace root, causing the DB to be created at the wrong location and stats to be wrong (2 files, 23734 symbols instead of 1059 files, 33098 symbols).

**Fix:** Check for `.git` (file or directory) as a project boundary in `find_workspace_root`. If a directory has `.git` but no `.julie/`, stop walking — it's a project root that needs a fresh `.julie/` created. (`src/workspace/mod.rs`)

### Second Bug Found & Fixed

**`create_project` didn't auto-trigger indexing** — registering a project via `POST /api/projects` left it in "registered" state with no indexing until manually triggered via `POST /api/projects/:id/index`.

**Fix:** Send an `IndexRequest` after successful registration. (`src/api/projects.rs`)

## Scenario B: Concurrent MCP Sessions

| Test | Result | Notes |
|------|--------|-------|
| B1. Concurrent search requests | PASS | Two simultaneous searches return correct results |
| B2. Concurrent force re-index | PASS | Both queued, executed sequentially via indexing lock, no deadlocks |
| B3. No crashes or corruption | PASS | Both re-indexes produced correct 1059/33098 counts |

## Scenario C: Agent Worktrees (Razorback)

| Test | Result | Notes |
|------|--------|-------|
| C1. EnterWorktree creates worktree | PASS | Worktree created at `/tmp/` |
| C2. Julie does NOT auto-index | PASS | Only explicitly registered projects are indexed |
| C3. Cleanup leaves no orphans | PASS | `git worktree remove` clean, no watchers/indexes left |

**Expected behavior confirmed:** Agent worktrees are transient and not registered with the daemon unless explicitly added via the API.

## Scenario D: Daemon Lifecycle

| Test | Result | Notes |
|------|--------|-------|
| D1. Kill daemon while session active | SKIPPED | Would kill our own test session |
| D2. Projects reload on restart | PASS | Registry persists to `registry.toml`, projects reload |
| D3. Register while running | PASS | Auto-indexes after registration |
| D4. Watcher starts for new project | PASS | `active_watchers` incremented from 1 to 2 |

## Summary

- **14/15 tests pass** (1 skipped — D1 requires external daemon kill)
- **2 bugs found and fixed** during validation
- Workspace isolation works correctly after fix
- Concurrent operations are safe (lock-serialized indexing, parallel search)
- Agent worktrees correctly excluded from auto-indexing
