# File Watcher Architecture

**Last Updated:** 2026-03-12
**Status:** Production (v6)

## Overview

Julie uses OS-native file watchers (via the [`notify`](https://docs.rs/notify) crate) to detect file changes and trigger incremental re-indexing. This keeps the symbol database and Tantivy search index up to date without requiring full re-indexes.

In stdio mode, `IncrementalIndexer` in `src/watcher/mod.rs` manages a single watcher for the connected workspace. It lives for the duration of the MCP session.

In daemon mode, `WatcherPool` in `src/daemon/watcher_pool.rs` manages one `IncrementalIndexer` per registered workspace, sharing watchers across all connected MCP sessions.

### Catch-up Indexing on Session Connect

File watchers only detect changes that happen while they're running. If files change while the daemon is down (during a restart, between sessions, or after a rebuild), the watcher misses them. To close this gap, a catch-up check runs automatically when a session connects to an already-indexed workspace:

1. `on_initialized` spawns a background `run_auto_indexing` task
2. `check_if_indexing_needed` (startup.rs) compares the newest file mtime against the symbols.db mtime
3. If stale, `handle_index_command` falls through to `index_workspace_files` (instead of returning "already indexed")
4. `filter_changed_files` (incremental.rs) computes blake3 hashes for each file and compares against stored hashes
5. Only files with changed hashes are re-indexed; unchanged files are skipped

This runs in the background and doesn't block the session. On a typical daemon restart where ~100 files changed, the catch-up completes in under 5 seconds.

---

## How It Works

### Platform Backends

The `notify` crate selects the best available backend per platform:

| Platform | Backend | Mechanism | Idle Cost |
|----------|---------|-----------|-----------|
| **macOS** | FSEvents | Kernel-level, event-driven | Negligible — no polling, no per-directory handles |
| **Windows** | ReadDirectoryChangesW | Kernel-level, event-driven | Negligible — one handle per watched root |
| **Linux** | inotify | Per-directory watch descriptor | Low for most projects. Large projects with many directories can approach the system limit (default ~65K watches, tunable via `fs.inotify.max_user_watches`) |

All backends use recursive watching from the project root — Julie calls `watcher.watch(project_root, RecursiveMode::Recursive)` once per project.

### Event Pipeline

```
File system event (OS)
  -> notify crate (platform backend)
    -> mpsc channel (unbounded)
      -> Filter: extension check + ignore patterns
        -> Debounce: 1s per-file deduplication
          -> Handler: re-extract symbols, update SQLite + Tantivy
```

1. **Filtering** (`src/watcher/filtering.rs`): Events are checked against supported file extensions (33 languages) and ignore patterns (`.git/`, `node_modules/`, `target/`, etc.). Unrecognized extensions and ignored paths are dropped immediately.

2. **Debouncing**: Per-file deduplication prevents redundant re-indexing when editors write the same file multiple times in quick succession (e.g., auto-save, format-on-save). If a file was processed within the last 1 second, subsequent events for that file are skipped.

3. **Handling** (`src/watcher/handlers.rs`): Three event types are processed:
   - **Created/Modified**: Re-extract symbols from the file, update SQLite rows, update Tantivy index
   - **Deleted**: Remove the file's symbols from SQLite (Tantivy handles deletions on next commit)
   - **Renamed**: Delete old path's symbols, extract and index at the new path
   - **Atomic save guard**: If a DELETE event fires but the file still exists on disk (common with editors that write to a temp file then rename), the delete is skipped — the follow-up Create/Modify event will handle it.

---

## Future Considerations

- **Linux inotify limits**: Users with very large projects (deep directory trees) on Linux may need to increase `fs.inotify.max_user_watches` via sysctl. macOS and Windows do not have this limitation.

---

## Related Files

- `src/watcher/mod.rs` — `IncrementalIndexer` (single-workspace watcher)
- `src/watcher/filtering.rs` — Extension and ignore-pattern filtering
- `src/watcher/handlers.rs` — File change handlers (create/modify/delete/rename)
- `src/watcher/events.rs` — Notify event to `FileChangeEvent` conversion
