# Daemon Reliability: FD Leak, Cold Index Blocking, Drain Timeout

**Date:** 2026-05-15
**Status:** Design approved, pending implementation plan review

## Context

Three daemon reliability issues were observed during Eros head-to-head benchmarking
(2026-05-15) and dev-time stale-binary restarts (2026-05-08). All three share a theme:
the daemon was designed for interactive MCP sessions with a handful of workspaces, and
breaks down under eval-corpus workloads (many short-lived workspaces) and rapid
dev-rebuild cycles.

## Issue 1: FD Leak via Idle Workspace Accumulation

### Problem

`WorkspacePool` holds `Arc<JulieWorkspace>` entries indefinitely. Each workspace keeps:
- Tantivy index files (mmap'd segments, IndexReader, IndexWriter)
- SQLite connection(s)
- File watcher handles (via WatcherPool)

During eval runs across N workspaces, FDs accumulate without bound. Observed: daemon
reached 1000+ open FDs, Tantivy failed with `Too many open files (os error 24)` on
`meta.json`.

### Root Cause

`WorkspacePool::get_or_init` inserts entries but nothing ever removes them. The pool
has `shutdown()` (drains everything at daemon exit) but no idle eviction.

### Design

Add LRU-style idle eviction to `WorkspacePool`:

1. **Track access time:** Add `last_accessed: Instant` to `WorkspaceEntry`. Update on
   every `get_or_init` hit.

2. **Background sweep task:** Spawn a tokio task in `WorkspacePool::new` that runs
   every 60s. For each entry where `last_accessed` is older than the idle threshold
   (default 5 minutes, configurable via `JULIE_WORKSPACE_IDLE_TIMEOUT_SECS`):
   - Call `SearchIndex::shutdown()` on the workspace's search index
   - Remove the entry from the HashMap
   - Log the eviction with workspace_id and FD savings estimate

3. **Watcher cleanup coordination:** When evicting a workspace from the pool, also
   remove its watcher from `WatcherPool` (if present). The watcher holds inotify/FSEvents
   handles that contribute to FD pressure.

4. **Re-acquisition:** `get_or_init` already handles the "not in pool" case by
   initializing a fresh workspace. No changes needed there.

5. **Graceful Arc handling:** The pool drops its `Arc<JulieWorkspace>` reference. If a
   session still holds a reference (active tool call), the workspace stays alive until
   that session's Arc drops. The pool just stops being the one keeping it alive.

### Files to Modify

- `src/daemon/workspace_pool.rs` — Add `last_accessed`, sweep task, eviction logic
- `src/daemon/watcher_pool.rs` — Add `remove_watcher(workspace_id)` if not already present
- `src/daemon/mod.rs` — Wire sweep task into daemon startup

### Acceptance Criteria

- [ ] Idle workspaces are evicted after 5 minutes (configurable)
- [ ] Eviction logs workspace_id and is visible in daemon.log
- [ ] Tantivy SearchIndex::shutdown() is called before dropping workspace
- [ ] WatcherPool entry is cleaned up on eviction
- [ ] get_or_init still works after eviction (re-initializes)
- [ ] Sweep task doesn't hold the write lock longer than the eviction batch
- [ ] Unit test: workspace evicted after idle timeout
- [ ] Unit test: recently-accessed workspace survives sweep

## Issue 2: Cold Index Blocks on Embedding Provider Startup

### Problem

`spawn_workspace_embedding` calls `svc.wait_until_settled(Duration::from_secs(120))`
*inline* before spawning the background embedding task. On a cold daemon start, the
embedding sidecar takes ~36-50s to bootstrap (Python venv, torch import, model load).
During this window, `handle_index_command_internal` blocks waiting for settlement,
causing the CLI `workspace index --force` to time out (Eros uses a 30s timeout).

### Root Cause

The `wait_until_settled` call is in the synchronous path of `spawn_workspace_embedding`,
which is awaited by `handle_index_command_internal` before returning the index result.
The canonical indexing (tree-sitter extraction + Tantivy) finishes in ~2s, but the
response is held hostage by embedding readiness.

### Design

Decouple embedding readiness from index command response:

1. **Non-blocking provider check:** In `spawn_workspace_embedding`, replace the inline
   `wait_until_settled(120s)` with an immediate readiness check (poll the current state
   without waiting).

2. **Deferred embedding task:** If the provider is not yet settled:
   - Spawn a background task that:
     a. Calls `wait_until_settled(120s)` (existing timeout)
     b. On Ready: runs the embedding pipeline
     c. On Unavailable/Timeout: logs and exits cleanly
   - Return 0 from `spawn_workspace_embedding` immediately
   - Append to the index response message: "Embedding deferred until provider ready."

3. **Already-settled fast path:** If provider is Ready at check time, proceed exactly
   as today (spawn background pipeline, return symbol count).

4. **Task slot management:** The deferred task still registers in
   `handler.embedding_tasks` so duplicate detection works and force-reindex can cancel it.

### Files to Modify

- `src/tools/workspace/indexing/embeddings.rs` — Restructure `spawn_workspace_embedding`
- `src/daemon/embedding_service.rs` — Add `try_settled()` or `is_settled()` non-blocking check

### Acceptance Criteria

- [ ] `workspace index --force` returns in <5s on cold daemon (embedding deferred)
- [ ] Embedding pipeline still runs after provider settles
- [ ] Response message indicates embedding status (immediate vs deferred)
- [ ] Force-reindex cancellation still works for deferred tasks
- [ ] Unit test: cold provider -> spawn returns 0, background task runs after settlement
- [ ] Unit test: warm provider -> spawn returns symbol count immediately

## Issue 3: Drain Timeout + Adapter Resilience

### Problem (a): Drain Timeout

`DEFAULT_DRAIN_TIMEOUT_SECS = 10` is too aggressive. In-flight heavy operations
(embedding pipelines, large workspace indexing, heavy search queries) can't complete
in 10s. Observed: forced shutdowns during dev rebuilds and eval cleanup, with log
messages "Session drain timeout exceeded, forcing shutdown".

### Problem (b): Adapter Permanent Death

When the daemon restarts (stale-binary or manual stop), the stdio adapter's HTTP
connection to the daemon dies. The adapter does not retry, so MCP tools become
permanently unavailable (`Transport closed` errors) until the entire Claude/Codex
session restarts.

### Design (a): Bump Drain Timeout

- Change `DEFAULT_DRAIN_TIMEOUT_SECS` from 10 to 60 in `src/daemon/mod.rs`
- Update the doc comment to reflect the new default
- The env var override (`JULIE_DAEMON_DRAIN_TIMEOUT_SECS`) and [1,120] range are unchanged
- Update TODO.md to reflect the fix

### Design (b): Adapter HTTP Retry

Add connection-loss retry to the adapter's HTTP client:

1. **Detection:** When an HTTP request to the daemon fails with connection refused,
   connection reset, or broken pipe, enter retry mode.

2. **Backoff schedule:** Exponential backoff: 1s, 2s, 4s, 8s, 16s (5 attempts, ~31s
   total). This covers the typical daemon restart window.

3. **Daemon re-launch:** On first connection failure, attempt to re-launch the daemon
   (same logic as initial adapter startup). The adapter already knows the daemon binary
   path and port.

4. **Scope:** Only retry on transport-level failures (TCP). Do not retry on HTTP-level
   errors (4xx, 5xx) or MCP-level errors — those indicate the daemon is running but
   rejecting the request.

5. **Logging:** Log each retry attempt at warn level so users can see what's happening.

### Files to Modify

- `src/daemon/mod.rs` — Change `DEFAULT_DRAIN_TIMEOUT_SECS` to 60, update comment
- `src/adapter/` — Add retry logic to HTTP client, daemon re-launch on connection loss

### Acceptance Criteria

- [ ] Default drain timeout is 60s
- [ ] Existing env var override still works
- [ ] Adapter retries on connection loss with exponential backoff
- [ ] Adapter re-launches daemon on persistent connection failure
- [ ] MCP session survives a daemon restart (tools recover within ~30s)
- [ ] Unit test: drain_timeout returns 60s by default
- [ ] Integration concept: adapter reconnects after daemon restart
