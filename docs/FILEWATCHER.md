# File Watcher Architecture

**Last Updated:** 2026-03-08
**Status:** Production

## Overview

Julie uses OS-native file watchers (via the [`notify`](https://docs.rs/notify) crate) to detect file changes and trigger incremental re-indexing. This keeps the symbol database and Tantivy search index up to date without requiring full re-indexes.

There are two watcher layers:

| Layer | Module | Used In | Scope |
|-------|--------|---------|-------|
| `IncrementalIndexer` | `src/watcher/mod.rs` | stdio MCP mode | Single workspace |
| `DaemonWatcherManager` | `src/daemon_watcher.rs` | Daemon (HTTP) mode | All registered projects |

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

1. **Filtering** (`src/watcher/filtering.rs`): Events are checked against supported file extensions (30 languages) and ignore patterns (`.git/`, `node_modules/`, `target/`, etc.). Unrecognized extensions and ignored paths are dropped immediately.

2. **Debouncing**: Per-file deduplication prevents redundant re-indexing when editors write the same file multiple times in quick succession (e.g., auto-save, format-on-save). If a file was processed within the last 1 second, subsequent events for that file are skipped.

3. **Handling** (`src/watcher/handlers.rs`): Three event types are processed:
   - **Created/Modified**: Re-extract symbols from the file, update SQLite rows, update Tantivy index
   - **Deleted**: Remove the file's symbols from SQLite (Tantivy handles deletions on next commit)
   - **Renamed**: Delete old path's symbols, extract and index at the new path
   - **Atomic save guard**: If a DELETE event fires but the file still exists on disk (common with editors that write to a temp file then rename), the delete is skipped — the follow-up Create/Modify event will handle it.

---

## Daemon Mode (`DaemonWatcherManager`)

In daemon mode, `DaemonWatcherManager` manages one `notify::RecommendedWatcher` per project.

### When Watchers Start

- **On daemon startup**: `start_watchers_for_ready_projects()` is called after loading all registered projects. Only projects with status `Ready` (both database and search index loaded) get watchers.
- **On project registration**: When a new project is added via the API and reaches `Ready` status, `start_watcher_if_ready()` creates a watcher for it.

### When Watchers Stop

- **Project removal**: `remove_workspace()` calls `stop_watching()` to cancel the background task and drop the watcher handle.
- **Daemon shutdown**: `stop_all()` cancels all watchers and their background tasks.

### Current Behavior

All registered and indexed (`Ready`) projects are watched, regardless of whether any MCP client is actively connected for that project. This is a deliberate simplicity-first design — the idle cost on macOS and Windows is negligible.

### Dashboard Integration

The dashboard stats endpoint (`GET /api/dashboard/stats`) exposes an `active_watchers` count showing how many projects currently have active file watchers. This is sourced from `DaemonWatcherManager::active_watchers()`.

---

## Stdio Mode (`IncrementalIndexer`)

In single-workspace stdio mode, `IncrementalIndexer` in `src/watcher/mod.rs` manages a single watcher for the connected workspace. It uses the same filtering, debouncing, and handler logic but is scoped to one project and lives for the duration of the MCP session.

---

## Future Considerations

- **Session-aware watching**: Currently all `Ready` projects are watched even when no MCP client is connected. A future optimization (deferred to v4.1) could start/stop watchers based on active MCP sessions, reducing resource usage for users with many registered projects on Linux (where inotify watches are a finite resource).
- **Linux inotify limits**: Users with very large projects (deep directory trees) on Linux may need to increase `fs.inotify.max_user_watches` via sysctl. macOS and Windows do not have this limitation.

---

## Related Files

- `src/daemon_watcher.rs` — `DaemonWatcherManager` (daemon-mode multi-project watcher)
- `src/watcher/mod.rs` — `IncrementalIndexer` (stdio-mode single-workspace watcher)
- `src/watcher/filtering.rs` — Extension and ignore-pattern filtering
- `src/watcher/handlers.rs` — File change handlers (create/modify/delete/rename)
- `src/watcher/events.rs` — Notify event to `FileChangeEvent` conversion
- `src/api/dashboard.rs` — Dashboard stats endpoint (exposes `active_watchers`)
