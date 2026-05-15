# Daemon Reliability Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use razorback:team-driven-development (on Claude Code) or razorback:subagent-driven-development (elsewhere) to implement this plan. Fall back to razorback:executing-plans for single-task or tightly-sequential plans.

**Goal:** Fix three daemon reliability issues: FD leak via idle workspace eviction, cold index blocking on embedding startup, and drain timeout + adapter resilience.

**Architecture:** Four independent tasks touching separate subsystems. Task 1 adds LRU eviction to `WorkspacePool`. Task 2 decouples embedding `wait_until_settled` from the index command response path. Task 3 bumps drain timeout and aligns `stop_daemon` wait. Task 4 hardens adapter retry for pre-output transport errors.

**Tech Stack:** Rust, tokio, Tantivy, rmcp

**Design doc:** `docs/plans/2026-05-15-daemon-reliability-design.md`

**Codex review:** Adversarial review completed. Key revisions: Task 1 replaces Arc::strong_count with session-attachment tracking; Task 3 now includes stop_daemon timeout alignment; Task 4 scoped down to pre-output retry only (mid-session replay is unsafe due to MCP initialize handshake requirements and non-idempotent tools).

---

### Task 1: Idle Workspace Eviction in WorkspacePool

**Files:**
- Modify: `src/daemon/workspace_pool.rs:18-27` (WorkspacePool struct, WorkspaceEntry struct)
- Modify: `src/daemon/workspace_pool.rs:61-64` (get method -- update last_accessed)
- Modify: `src/daemon/workspace_pool.rs:74-165` (get_or_init -- update last_accessed)
- Modify: `src/daemon/workspace_pool.rs:186-189` (existing evict_workspace -- add shutdown)
- Modify: `src/daemon/mod.rs:284-778` (run_daemon -- spawn sweep task)
- Test: `src/tests/daemon/workspace_pool_eviction.rs` (new)

**What to build:** LRU-style idle eviction for `WorkspacePool` to prevent FD accumulation across many workspaces. Each workspace holds Tantivy index files + SQLite connections; without eviction the daemon hits EMFILE under eval workloads.

**Approach:**

*Tracking access:*
- Add `last_accessed: Instant` to `WorkspaceEntry` (line 25-27). Initialize to `Instant::now()` on insert.
- In `get()` (line 61) and `get_or_init()` (line 74) -- on cache hit, update `last_accessed`. `get()` currently takes a read lock; change to upgradable-read or take write lock briefly for the timestamp update.

*Active-session protection (NOT Arc::strong_count):*
- `Arc::strong_count` is unreliable because handlers clone `JulieWorkspace` by value and keep inner `db`/`search_index` Arcs independently. Instead, use the existing `SessionTracker` to check if any active session is attached to the workspace. The sweep task takes a reference to `SessionTracker` and skips workspaces that have active session attachments.
- Check via the workspace-session attachment module (`src/daemon/workspace_session_attachment.rs`) which tracks which session IDs are bound to which workspace IDs. If any session is attached, skip eviction.

*Eviction protocol (remove-first, then shutdown):*
- The sweep must **remove the entry from the HashMap first**, then call `SearchIndex::shutdown()` on the removed workspace outside the lock. This prevents a concurrent `get()` from returning a half-shutdown workspace.
- Same pattern as `WorkspacePool::shutdown()` (line 249-307): drain entries under write lock, then iterate and shut down outside the lock.

*Watcher cleanup ordering:*
- Check `watcher_pool.remove_if_inactive(workspace_id)` BEFORE removing from the workspace pool. If the watcher still has refs (returns false), skip eviction entirely for this workspace.
- This prevents orphaning a watcher that still expects its workspace.

*Fix existing `evict_workspace()`:*
- The existing method (line 186-189) just removes without calling `SearchIndex::shutdown()`. Add shutdown to it, following the same remove-first-then-shutdown protocol. Callers (dashboard/cleanup) currently leak Tantivy handles.

*Sweep task:*
- `pub fn spawn_idle_sweep(self: &Arc<Self>, watcher_pool: Arc<WatcherPool>, sessions: Arc<SessionTracker>, interval: Duration, idle_threshold: Duration) -> JoinHandle<()>`
- Runs every `interval` (60s default). Collects eviction candidates under read lock, then processes under write lock.
- Idle threshold configurable via `JULIE_WORKSPACE_IDLE_TIMEOUT_SECS` env var (default 300, min 60, max 3600).
- In `run_daemon()`, spawn after creating workspace pool, watcher pool, and session tracker. Abort handle during shutdown.

**Acceptance criteria:**
- [ ] `WorkspaceEntry` tracks `last_accessed`
- [ ] `get()` and `get_or_init()` update `last_accessed` on hit
- [ ] Sweep task evicts workspaces idle > threshold
- [ ] Eviction uses remove-first-then-shutdown protocol (no half-dead workspace visible to get())
- [ ] Watcher remove_if_inactive checked before eviction; skip if watcher has refs
- [ ] Workspaces with active session attachments are skipped (not Arc::strong_count)
- [ ] Existing `evict_workspace()` also shuts down SearchIndex before dropping
- [ ] `get_or_init` reinitializes after eviction (already works -- evicted = not in map)
- [ ] Env var `JULIE_WORKSPACE_IDLE_TIMEOUT_SECS` configurable
- [ ] Test: workspace evicted after idle timeout
- [ ] Test: recently-accessed workspace survives sweep
- [ ] Test: workspace with active session attachment survives sweep
- [ ] Test: concurrent get() during sweep does not return shutdown workspace
- [ ] Tests pass, committed

---

### Task 2: Decouple Embedding Wait from Index Response

**Files:**
- Modify: `src/tools/workspace/indexing/embeddings.rs:23-267` (spawn_workspace_embedding)
- Modify: `src/daemon/embedding_service.rs:257-259` (expose snapshot_settled as try_settled)
- Modify: `src/tools/workspace/commands/index.rs:419-428,467-482` (index response message)
- Modify: `src/tools/workspace/commands/registry/register_remove.rs:103` (register caller)
- Modify: `src/tools/workspace/commands/registry/refresh_stats.rs:93,246` (refresh caller + message)
- Test: `src/tests/tools/workspace/embedding_deferred.rs` (new)

**What to build:** Make `spawn_workspace_embedding` non-blocking on cold embedding provider. Currently it awaits `wait_until_settled(120s)` inline, which blocks the index command response for up to 120s while the sidecar bootstraps. Canonical indexing finishes in ~2s -- the response shouldn't wait for embeddings.

**Approach:**

*Non-blocking readiness check:*
- Expose `snapshot_settled()` (line 257-259, currently private) as `pub fn try_settled(&self) -> Option<EmbeddingServiceSettled>` on `EmbeddingService`. Zero-cost: just reads the watch channel's current value.

*Restructure daemon-mode branch:*
- In `spawn_workspace_embedding` (lines 30-63), replace the inline `wait_until_settled(120s)` with:
  1. `svc.try_settled()` (non-blocking poll)
  2. If `Some(Ready)` -- use provider immediately (existing fast path)
  3. If `Some(Unavailable)` -- return immediately (existing degraded path)
  4. If `None` (still initializing) -- spawn a **deferred embedding task**

*Deferred task design:*
- Extract the embedding pipeline logic (lines 130-258) into a helper function that takes a provider, so both the immediate and deferred paths can call it without re-entering `spawn_workspace_embedding` (avoids the re-entrancy risk where a deferred task could cancel itself).
- The deferred task:
  1. Registers in `handler.embedding_tasks` BEFORE returning (so cancellation/duplicate-detection work from the start)
  2. Inside the spawned future: calls `wait_until_settled(120s)`
  3. On Ready: calls the extracted helper to run the pipeline
  4. On Unavailable/Timeout: logs and exits, cleans up task slot

*Return type change:*
- Change return type from `usize` to `pub struct EmbeddingOutcome { pub symbols: usize, pub deferred: bool }`.
- Update ALL callers:
  - `handle_index_command_internal` (lines 419-428, 467-482) -- already the primary consumer
  - `handle_register_command` (`register_remove.rs:103`) -- uses embed_count for response message
  - `refresh_workspace_internal` (`refresh_stats.rs:93,246`) -- uses embed_count for response message
- When `deferred=true`, append "Embedding queued while provider initializes." to response messages.

**Acceptance criteria:**
- [ ] `EmbeddingService::try_settled()` is a non-blocking readiness check
- [ ] `spawn_workspace_embedding` returns immediately when provider is initializing
- [ ] Embedding pipeline logic extracted into reusable helper (no re-entrancy)
- [ ] Deferred task registers in `embedding_tasks` before `spawn_workspace_embedding` returns
- [ ] ALL callers updated for `EmbeddingOutcome` return type (index, register, refresh)
- [ ] Response messages distinguish immediate vs deferred embedding
- [ ] Force-reindex cancellation works for deferred tasks (cancels before provider ready)
- [ ] Test: cold provider -> spawn returns immediately, deferred=true
- [ ] Test: warm provider -> spawn returns symbol count, deferred=false
- [ ] Test: unavailable provider -> spawn returns 0, deferred=false
- [ ] Test: force-reindex cancels deferred task before provider settles
- [ ] Test: duplicate cold spawn for same workspace deduplicates via task slot
- [ ] Tests pass, committed

---

### Task 3: Bump Default Drain Timeout + Align stop_daemon

**Files:**
- Modify: `src/daemon/mod.rs:69` (DEFAULT_DRAIN_TIMEOUT_SECS constant)
- Modify: `src/daemon/mod.rs:77-79` (doc comment)
- Modify: `src/daemon/lifecycle.rs:397` (stop_daemon hardcoded 10s deadline)
- Modify: `src/tests/daemon/drain_timeout.rs` (update expected default)
- Test: verify existing lifecycle tests still pass

**What to build:** Change the default drain timeout from 10s to 60s. Also align `stop_daemon()` which has its own hardcoded 10s wait (line 397 of lifecycle.rs) -- if drain is 60s, stop must wait at least that long.

**Approach:**

*Drain timeout:*
- Change `const DEFAULT_DRAIN_TIMEOUT_SECS: u64 = 10` to `60` on line 69 of `mod.rs`.
- Update the doc comment (lines 77-79) to say "default (60 s)".

*stop_daemon alignment:*
- `stop_daemon()` at `lifecycle.rs:397` has `Duration::from_secs(10)` hardcoded. This must match or exceed the drain timeout. Change it to read `drain_timeout()` (the same function that `run_daemon` uses, which respects the env var). Import `super::drain_timeout` in lifecycle.rs.
- Update the bail message at line 406-410 to reference the actual timeout value, not hardcoded "10s".

*Test updates:*
- `src/tests/daemon/drain_timeout.rs` -- update default-value assertions from 10s to 60s.
- Check for any other tests that assert on the 10s value (Codex flagged lines 64, 75).
- Existing lifecycle tests (`test_stop_daemon_*`) should still pass since they mock/fake the daemon process.

*Cross-task note:* Task 4 adapter retry budget (MAX_RETRIES * backoff) should be less than or equal to drain timeout. With Task 4's 5 retries at 1+2+4+8+16=31s total backoff, this fits within 60s drain.

**Acceptance criteria:**
- [ ] `DEFAULT_DRAIN_TIMEOUT_SECS` is 60
- [ ] Doc comment reflects new default
- [ ] `stop_daemon()` uses `drain_timeout()` instead of hardcoded 10s
- [ ] stop_daemon bail message shows actual timeout value
- [ ] All drain_timeout tests pass with new default
- [ ] Existing lifecycle tests still pass
- [ ] Tests pass, committed

---

### Task 4: Adapter Retry on Pre-Output Transport Errors

**Files:**
- Modify: `src/adapter/http_stdio.rs:17-125` (run_http_adapter -- Err handling, backoff)
- Modify: `src/adapter/http_stdio.rs:175-246` (forward -- error classification)
- Test: `src/tests/adapter/retry_resilience.rs` (new)

**What to build:** The adapter exits permanently on transport errors (`Err` case, line 117-119). Transport errors before any output was written should trigger retry, same as `ImmediateDaemonDisconnect`.

**Scope reduction (per Codex review):** Mid-session retry (after output written) is **explicitly out of scope**. New MCP transports require initialize handshake, and non-idempotent tools (edit_file, rewrite_symbol) could double-apply on replay. The correct defense against mid-session daemon death is the longer drain timeout (Task 3), not adapter replay. The existing `SessionEnded` return for post-output disconnect is unchanged.

**Approach:**

**(a) Typed adapter errors:**
- Create `enum AdapterError { Transport(anyhow::Error), Stdin(std::io::Error) }` in `src/adapter/http_stdio.rs`.
- In `forward_http_stdio_transport_with_pending`, wrap errors from `send_client_line` (transport) and stdin reads differently. `send_client_line` errors become `AdapterError::Transport`. Stdin errors become `AdapterError::Stdin`.
- Change `forward_http_stdio_transport_with_pending` return type to `Result<ForwardOutcome, AdapterError>`.

**(b) Retry on transport errors before output:**
- In `run_http_adapter`, match on the error type:
  - `AdapterError::Stdin(_)` -- terminal, exit immediately (MCP client gone)
  - `AdapterError::Transport(e)` -- use `restart_handoff_action` to decide retry vs exhaust, same pattern as `ImmediateDaemonDisconnect` arm (lines 94-116)
- Track `wrote_any_output` state: if a transport error occurs after output was written, treat as terminal (don't retry). The `forward_http_stdio_transport_with_pending` function can include `wrote_any_output` in the error or outcome to inform the caller.

**(c) Increase MAX_RETRIES and add backoff:**
- Bump `MAX_RETRIES` from 2 to 5.
- Add exponential backoff between retries: `tokio::time::sleep(Duration::from_secs(1 << attempt.min(4)))` before `continue`. This gives 1s, 2s, 4s, 8s, 16s -- ~31s total, within Task 3's 60s drain window.

**(d) Ensure daemon re-launch on retry:**
- The existing `ensure_daemon_ready()` at the top of the retry loop already handles re-launch (checks PID, spawns if dead, waits for ready state). Verify it correctly detects a stopped daemon (stale PID file) and doesn't fast-path on a dead process.
- `ensure_daemon_ready()` calls `daemon_readiness()` which checks PID + state file + transport probe (lines 85-116 of launcher.rs). Dead daemon -> `Dead` -> spawns new one. This should work, but add a test that confirms retry after daemon death re-launches successfully.

**Acceptance criteria:**
- [ ] `AdapterError` enum distinguishes transport vs stdin errors
- [ ] Transport errors before output trigger retry with restart_handoff_action
- [ ] Stdin errors cause immediate exit (no retry)
- [ ] Transport errors after output written cause immediate exit (no retry)
- [ ] MAX_RETRIES increased to 5
- [ ] Exponential backoff between retries (1s, 2s, 4s, 8s, 16s)
- [ ] Total retry window (~31s) fits within drain timeout (60s)
- [ ] Test: pre-output transport error triggers retry
- [ ] Test: stdin error does not retry
- [ ] Test: post-output transport error does not retry
- [ ] Test: retry re-launches daemon via ensure_daemon_ready
- [ ] Tests pass, committed

---

## Task Dependencies

```
Task 1 (WorkspacePool eviction)  ─┐
Task 2 (Embedding decouple)      ─┤─ all independent, can run in parallel
Task 3 (Drain timeout)           ─┤
Task 4 (Adapter retry)           ─┘
```

All four tasks touch different files with no overlap. Fully parallelizable.

**Cross-task note:** Task 3's drain timeout (60s) and Task 4's retry budget (~31s) are designed to be compatible. Task 1's eviction and Task 2's deferred embedding interact only through the shared `EmbeddingService`, which is read-only from Task 1's perspective.

## Verification

After all tasks complete:
- `cargo xtask test dev` -- batch regression gate
- `cargo xtask test reliability` -- daemon/workspace/integration tier
- Manual: rebuild release binary while a session is active, verify adapter retries and MCP tools recover within ~30s
