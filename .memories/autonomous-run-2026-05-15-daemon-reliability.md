# Autonomous Execution Report -- Daemon Reliability

**Status:** Complete
**Plan:** docs/plans/2026-05-15-daemon-reliability-plan.md
**Branch:** main (direct commits, no feature branch)
**Duration:** ~1h 30m
**Tasks:** 4/4 complete (grouped into 3 teammates + 3 review fixes)

## What shipped

- **Idle workspace eviction:** LRU-style sweep in WorkspacePool evicts workspaces idle >5min (configurable via `JULIE_WORKSPACE_IDLE_TIMEOUT_SECS`). Remove-first-then-shutdown protocol prevents half-dead workspaces. Watcher cleanup coordinated via `remove_if_inactive`. Existing `evict_workspace()` now also shuts down SearchIndex. (commit 6fb5f760)
- **Drain timeout 10s->60s:** `DEFAULT_DRAIN_TIMEOUT_SECS` bumped from 10 to 60. `stop_daemon()` aligned to use `drain_timeout()` instead of hardcoded 10s. (commit 6fb5f760)
- **Decouple embedding wait from index response:** `spawn_workspace_embedding` no longer blocks on cold embedding provider. Non-blocking `try_settled()` check; deferred task waits in background. New `EmbeddingOutcome { symbols, deferred }` return type. All callers updated (index, register, refresh). (commit 3ece3b29)
- **Adapter retry on pre-output transport errors:** `AdapterError` enum distinguishes transport vs stdin errors. Pre-output transport errors trigger retry (MAX_RETRIES=5, exponential backoff 1-16s). Post-output and stdin errors are terminal. (commit b3e0c3cc)

## Review fixes (from Codex adversarial review)

- **Preserve lost request line on send failure:** `send_client_line` now returns the line on transport error so it can be requeued for retry. (commit 9811af54)
- **Sync vector count on deferred embedding terminal paths:** New `sync_vector_count_on_terminal` helper updates `daemon.db.vector_count` when deferred embedding fails (Unavailable, Timeout, missing DB, DB open error). (commit f0025e4d)
- **Skip malformed client JSON instead of retrying:** Parse errors in `send_client_line` now log a warning and skip the line instead of wrapping as `AdapterError::Transport` and burning the retry budget. (commit 043800b3)

## Judgment calls

- `src/daemon/workspace_pool.rs` -- Used watcher ref_count as session guard instead of SessionTracker. Watcher refs are incremented on session attach, so ref_count > 0 reliably indicates an active session. Simpler than threading SessionTracker through the sweep.
- `src/adapter/http_stdio.rs` -- Scoped retry to pre-output transport errors only. Mid-session replay requires MCP initialize handshake and risks double-applying non-idempotent tools (edit_file, rewrite_symbol). Longer drain timeout (Task 3) is the correct defense against mid-session daemon death.

## External review (codex, adversarial)

### Round 1 (pre-merge)
- **Findings:** 4
- **Verified real, fixed:** 2 (commits: 9811af54, f0025e4d)
  - Send failure drops request line -- fixed by returning line in error tuple
  - Terminal paths preserve stale vector counts -- fixed by adding sync helper
- **Dismissed:** 2
  - Idle eviction with live session but no watcher -- false-positive, watcher ref_count IS the session guard
  - Deferred task stale handle race -- false-positive, tokio::spawn queues future, parent inserts slot before yielding

### Round 2 (final review)
- **Findings:** 6
- **Verified real, fixed:** 1 (commit: 043800b3)
  - Malformed client JSON retried as transport error -- fixed by skipping malformed lines
- **Dismissed:** 2
  - Retry replays lost_line before in-flight -- false-positive, in pre-output scenario lost_line is the first request
  - Deferred task slot race -- duplicate of round 1 dismissal
- **Out of scope:** 3
  - Sweep during background indexing -- 5-min idle timeout provides generous buffer
  - Force reindex + unavailable service stale vector_count -- pre-existing issue, not introduced by this branch
  - Eviction returns true on shutdown timeout -- same timeout pattern as existing code
- **Flagged for your review:** 0

## Tests
- 28 new tests across 4 test modules, all passing
  - `workspace_pool_eviction`: 5 tests (eviction, access refresh, shutdown, sweep)
  - `drain_timeout`: 3 tests (default, env var, clamping)
  - `retry_resilience`: 12 tests (classification, backoff, ordering, lost line)
  - `embedding_deferred`: 8 tests (try_settled states, deferred flag, vector count sync)

## Blockers hit
- None

## Files changed
- 19 files, +1757 / -164 lines
- Key files: `workspace_pool.rs` (+259), `http_stdio.rs` (+230), `embeddings.rs` (+498), `retry_resilience.rs` (+377), `embedding_deferred.rs` (+216), `workspace_pool_eviction.rs` (+164)

## Next steps
- Push to origin/main
- Update TODO.md to mark drain timeout issue as resolved
- Monitor daemon.log for idle eviction behavior under real workloads
- Consider adding active-indexing lease check to sweep (deferred from Codex finding, low priority)
