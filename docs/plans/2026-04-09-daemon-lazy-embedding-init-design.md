# Daemon Lazy Embedding Initialization

**Status:** Design
**Author:** Claude (with Codex second opinion)
**Date:** 2026-04-09
**Area:** `src/daemon/`, `src/tools/workspace/indexing/`, `src/tools/search/`, `src/dashboard/`

---

## Problem

After a machine reboot, Claude Code marks Julie's MCP server as `failed` on first connection. A manual `/mcp reconnect` succeeds instantly. This happens on every daemon cold start, not just post-reboot; version bumps, crashes, stale-binary restarts, and manual stops are all affected.

## Diagnosis

The daemon's cold-start takes ~39 seconds from PID file creation to `ready` state. The dominant cost is `EmbeddingService::initialize()` at `src/daemon/mod.rs:330-334`, which blocks the startup path while the Python sidecar spawns, PyTorch loads CUDA, and the CodeRankEmbed 768d model loads.

Claude Code's MCP client has a 30 second startup timeout for `initialize` responses, confirmed via inspection of `claude.exe v2.1.98` (fallback of `MCP_TIMEOUT || 30000`). The flow on a cold start:

1. Claude Code spawns `julie-server` (adapter) and sends an MCP `initialize` request on its stdin.
2. `run_adapter` at `src/adapter/mod.rs:47` calls `ensure_daemon_ready` **before** reading stdin. This blocks for 39 seconds waiting for the daemon.
3. Claude Code times out at 30 seconds and kills the adapter.
4. The daemon, spawned with stdio detached at `src/adapter/launcher.rs:268`, continues warming up and eventually writes `ready` at 23:13:34.753.
5. The user runs `/mcp reconnect`. A fresh adapter takes the `daemon_readiness() == Ready` fast path at `src/adapter/launcher.rs:107` and connects instantly.

Evidence from `~/.julie/daemon.log.2026-04-09`:

```
23:12:55.397  Starting Julie daemon v6.6.10
23:12:55.399  Daemon PID file created pid=6108
23:12:55.440  Initializing shared embedding service...
23:13:34.512  Embedding provider initialized (cuda, 768d)   ← 39.07s later
23:13:34.753  Daemon listening for IPC connections          ← READY
23:14:33.508  IPC headers received                          ← manual reconnect
```

Cold vs. warm delta is only ~3 seconds (36.18s on warm start 2026-04-08), so the model load time dominates, not disk I/O. The fix must target the critical path, not disk caching.

## Goals

- Daemon `ready` state is written within **2 seconds** of PID file creation on cold start.
- Claude Code connects to Julie on the first attempt after a reboot (no `failed` status, no manual reconnect).
- `spawn_workspace_embedding` correctly waits for the embedding provider when called during the warmup window, instead of silently skipping embedding.
- `nl_embeddings` definition-query path does not spawn a second provider via the stdio fallback when the daemon service is still initializing.
- Watchers created during the warmup window correctly use the provider once it becomes available.
- Dashboard accurately reports `embedding_available` transitioning from `false` to `true` as the background init completes.
- Existing behavior on machines where embedding init genuinely fails is preserved — indexing falls through to keyword-only without hanging.

## Non-Goals

- Having the adapter speak MCP protocol itself for the `initialize` handshake (Fix #2 from the investigation). Not needed once daemon `ready` time drops below Claude Code's 30 second deadline.
- A user-level `MCP_TIMEOUT=60000` workaround. Fixes the symptom without fixing the latency; rejected.
- Pre-warming the Python sidecar across daemon restarts (e.g., a persistent sidecar process). Too invasive for this fix; reconsider if model load time grows significantly.
- Reducing model load time itself. Orthogonal; separate investigation.

## Design

### 1. `EmbeddingService` State Machine

**File:** `src/daemon/embedding_service.rs`

Current `EmbeddingService` holds `provider: Option<Arc<dyn EmbeddingProvider>>` and `runtime_status: Option<EmbeddingRuntimeStatus>` as plain fields. These are written once in `initialize()` and never mutated. This prevents background initialization because there is no way to publish the result after `EmbeddingService` has been constructed.

Replace the internals with a `tokio::sync::watch` channel. `watch` is designed precisely for the "publish a latest value to many consumers" pattern and naturally handles the "already-settled before wait" case, avoiding the TOCTOU hazard in a `Notify`-based approach.

```rust
pub struct EmbeddingService {
    state_tx: tokio::sync::watch::Sender<EmbeddingServiceState>,
    state_rx: tokio::sync::watch::Receiver<EmbeddingServiceState>,
}

#[derive(Clone)]
enum EmbeddingServiceState {
    Initializing,
    Ready {
        provider: Arc<dyn EmbeddingProvider>,
        runtime_status: EmbeddingRuntimeStatus,
    },
    Unavailable {
        reason: String,
    },
}
```

`EmbeddingServiceState` must be `Clone`. `Arc<dyn EmbeddingProvider>` is already `Clone`. If `EmbeddingRuntimeStatus` is not `Clone`, derive it or wrap in `Arc`.

Public API:

- `EmbeddingService::initializing() -> Self` — constructs a `watch::channel` with initial value `Initializing`. Used by daemon startup before the background task runs.
- `EmbeddingService::publish_ready(&self, provider, runtime_status)` — calls `state_tx.send(Ready { .. })`. Called by the background init task on success.
- `EmbeddingService::publish_unavailable(&self, reason)` — calls `state_tx.send(Unavailable { .. })`. Called on init failure OR if the background task is cancelled / panics.
- `EmbeddingService::provider(&self) -> Option<Arc<dyn EmbeddingProvider>>` — non-blocking, calls `state_rx.borrow()` and returns `Some(provider.clone())` only if state is `Ready`.
- `EmbeddingService::runtime_status(&self) -> Option<EmbeddingRuntimeStatus>` — non-blocking.
- `EmbeddingService::is_available(&self) -> bool` — non-blocking, `matches!(*state_rx.borrow(), Ready { .. })`.
- `EmbeddingService::is_settled(&self) -> bool` — non-blocking, `true` for Ready or Unavailable.
- `async fn wait_until_settled(&self, timeout: Duration) -> EmbeddingServiceSettled` — clones the `Receiver` and loops `changed().await` with `tokio::time::timeout(deadline, ..)`, returning on first Ready/Unavailable observation. On the first iteration it checks `borrow()` without awaiting, so a state that settled before the call returns immediately (no TOCTOU).

```rust
pub enum EmbeddingServiceSettled {
    Ready(Arc<dyn EmbeddingProvider>),
    Unavailable(String),
    Timeout,
}
```

**Why watch over RwLock+Notify:** `tokio::sync::Notify::notify_waiters()` is edge-triggered — it wakes only currently-waiting futures, and notifications fire-and-forget if nobody is waiting. A naive `check state → if not settled, await notify` pattern has a race: publisher can settle + notify between the check and the wait, and the waiter hangs. `watch::Receiver` tracks seen-versus-unseen values, so `changed().await` fires immediately for any update the receiver hasn't yet observed, including one that happened before the `await`.

Keep `initialize_for_test` for unit tests; have it construct a service and publish directly to a specified state.

### 2. Daemon Startup Sequence

**File:** `src/daemon/mod.rs` (`run_daemon`)

**Current flow (lines 276-491):**

1. ensure_dirs, create PID file, write `starting` state
2. Open daemon.db, reset session counts, prune tool calls
3. Migrate stale workspace IDs, normalize workspace paths, backfill vector counts
4. **Block on `EmbeddingService::initialize()`** (~39s)
5. Sync embedding_model for workspaces that need it
6. Create WatcherPool, WorkspacePool, SessionTracker
7. Bind dashboard HTTP listener
8. **Block on `opener::open(&dashboard_url)`** (1-3s on cold Windows)
9. Bind IPC listener
10. Write `ready` state

**New flow:**

1. ensure_dirs, create PID file, write `starting` state
2. Open daemon.db, reset session counts, prune tool calls
3. Migrate stale workspace IDs, normalize workspace paths, backfill vector counts
4. Construct `EmbeddingService::initializing()` (instant)
5. Create WatcherPool, WorkspacePool (with the service in `Initializing` state), SessionTracker
6. Bind dashboard HTTP listener
7. Launch `opener::open` as a background `tokio::spawn` (non-blocking)
8. Bind IPC listener
9. Write `ready` state
10. Spawn background initialization task (runs concurrently with accept loop):
    - Outer task: `tokio::spawn(async move { ... })`
    - Inside, call `tokio::task::spawn_blocking(|| create_embedding_provider()).await`
    - On success: call `embedding_service.publish_ready(provider, runtime_status)`, then run the `embedding_model` sync loop (currently at lines 340-362) against `daemon_db` using the newly-available provider.
    - On `create_embedding_provider` returning `Err`: call `embedding_service.publish_unavailable(reason)` with the error string and log.
    - On `spawn_blocking` returning `JoinError` (panic or cancellation): call `embedding_service.publish_unavailable("init task panicked/cancelled")` and log. **This is critical** — without this, a panicking init task leaves the service stuck in `Initializing` forever, and every `wait_until_settled` caller hits `Timeout` instead of the actual failure state.
    - Consider a `tokio::select!` against a shutdown signal so the outer task can publish `Unavailable("shutdown")` if the daemon stops mid-init. Not strictly required for correctness (daemon exit drops everything anyway) but tidier shutdown behavior.

The background task holds an `Arc<EmbeddingService>` and an `Arc<DaemonDatabase>` clone. It does not block anything on the critical path.

**Expected `ready` time:** <2 seconds. Critical path is now:
- Daemon.db open + session reset + tool-call prune: ~50ms
- Workspace migrations and vector-count backfill: scales with workspace count, typically <500ms
- WatcherPool + WorkspacePool construction: in-memory, microseconds
- Dashboard router + HTTP listener bind: <100ms
- IPC listener bind: <10ms

### 3. Workspace Indexing Embedding Path

**File:** `src/tools/workspace/indexing/embeddings.rs` (`spawn_workspace_embedding`)

**Current behavior (lines 30-40):**

```rust
let provider = if let Some(p) = handler.embedding_provider().await {
    p
} else if handler.embedding_service.is_some() {
    // Daemon mode but no provider available. Don't attempt the stdio lazy-init
    // path; just skip embeddings.
    debug!("Daemon mode but no embedding provider available, skipping workspace embedding");
    return 0;
} else {
    // Stdio mode: provider not yet initialized. Do it now.
    // ... stdio lazy-init path ...
};
```

**Problem:** Under the new design, `embedding_service.provider()` returns `None` during the warmup window. Any workspace that auto-indexes during that window — which is very likely on cold start — gets its embedding step silently skipped and never re-queued.

**Fix:**

```rust
let provider = if let Some(p) = handler.embedding_provider().await {
    p
} else if let Some(svc) = handler.embedding_service.as_ref() {
    // Daemon mode. Wait for the service to settle (ready or unavailable).
    // Bounded at 120s — if the sidecar hasn't initialized by then, something
    // is genuinely wrong and we degrade to keyword-only.
    match svc.wait_until_settled(Duration::from_secs(120)).await {
        EmbeddingServiceSettled::Ready(p) => p,
        EmbeddingServiceSettled::Unavailable(reason) => {
            debug!(reason = %reason, "Embedding service unavailable, skipping workspace embedding");
            return 0;
        }
        EmbeddingServiceSettled::Timeout => {
            warn!("Embedding service did not settle within 120s, skipping workspace embedding");
            return 0;
        }
    }
} else {
    // Stdio mode: initialize now (unchanged from current behavior)
    // ... existing stdio lazy-init path ...
};
```

The 120s timeout is intentionally generous — it's a background task, not user-facing, and we'd rather wait than skip embedding for a freshly-indexed workspace.

### 4. NL Definition Query Path

**File:** `src/tools/search/nl_embeddings.rs` (`maybe_initialize_embeddings_for_nl_definitions`)

**Current behavior (lines 41-54):**

```rust
if handler.embedding_provider().await.is_some() {
    return;
}
// No provider yet. In daemon mode the shared service would have returned
// one, so this is either stdio mode or a transient initialization gap.
// [falls through to workspace-level stdio init]
```

**Problem:** The comment is wrong under the new design. In daemon mode during warmup, `embedding_provider()` returns `None` and this function falls through to a workspace-level stdio init via `workspace.initialize_embedding_provider()`. That path spawns a **second** Python sidecar, masks the real daemon provider, and wastes resources.

**Fix:** Split the daemon-mode and stdio-mode paths explicitly.

```rust
// If a provider is already available (daemon Ready or stdio cached), done.
if handler.embedding_provider().await.is_some() {
    return;
}

// Daemon mode: wait briefly on the service. NL queries are interactive, so
// use a short timeout — users won't wait 60s for definition search.
if let Some(svc) = handler.embedding_service.as_ref() {
    match svc.wait_until_settled(Duration::from_secs(3)).await {
        EmbeddingServiceSettled::Ready(_) => {
            // Provider is now published; the caller's next fetch via
            // handler.embedding_provider() will see it.
            return;
        }
        EmbeddingServiceSettled::Unavailable(_) | EmbeddingServiceSettled::Timeout => {
            debug!("Daemon embedding service not ready in time for NL query, falling through to keyword-only");
            return;
        }
    }
}

// Stdio mode: proceed with the per-workspace lazy init (unchanged).
// [existing code continues]
```

The `return` in the `Timeout` branch is important — we do NOT want to trigger the per-workspace stdio init in daemon mode even if the shared service is still warming up. That would create a duplicate sidecar.

### 5. Watcher Provider Propagation

**File:** `src/daemon/workspace_pool.rs` (`shared_embedding_provider`)

**Current behavior:**

```rust
fn shared_embedding_provider(&self) -> Option<Arc<dyn crate::embeddings::EmbeddingProvider>> {
    self.embedding_service
        .as_ref()
        .and_then(|svc| svc.provider().cloned())
}
```

This already reads the service live on each call — it clones the current `Option<Arc>` rather than caching it. Under the new state machine, `svc.provider()` will return `None` during `Initializing` and `Some(...)` after `publish_ready`. No code change needed to the accessor itself.

**However:** watchers that are *attached* to a workspace may cache the provider they receive at attach time. Need to audit `WorkspacePool::attach_watcher` and `Watcher::update_embedding_provider` to verify:

1. A watcher attached while the service is `Initializing` starts with `provider = None` and correctly handles that state for incremental updates (skip, queue, or keyword-only).
2. When the background init completes, watchers either re-read the live provider on their next cycle OR receive an explicit push via `update_embedding_provider`.

**Plan:** Audit during implementation. If watchers cache and don't re-read, add a hook to `publish_ready` that walks `WorkspacePool` and calls `update_embedding_provider` on each active watcher. If they already re-read via `shared_embedding_provider` every cycle, no change needed.

This is the one area of the design where I'm committing to "verify during implementation, fix if broken" rather than specifying the fix up-front. The file list already includes `workspace_pool.rs` and `watcher/mod.rs` as touch points if a fix is needed.

### 6. Dashboard Observability

**File:** `src/dashboard/state.rs`

**Current behavior:** `DashboardState` stores `embedding_available: bool` as a static field, set once at construction from `embedding_service.is_available()` in `run_daemon` at line 429.

**Problem:** With lazy init, the dashboard's constructor runs before the embedding service is ready. It would permanently display `embedding_available: false`.

**Fix:** Replace the `bool` field with an `Arc<EmbeddingService>` reference (or a subset trait that exposes only `is_available()` / `runtime_status()`). The accessor method `embedding_available()` re-reads the service state on each call:

```rust
pub struct DashboardState {
    // ... other fields ...
    embedding_service: Arc<EmbeddingService>,
    // embedding_available: bool,  // REMOVED
}

impl DashboardState {
    pub fn embedding_available(&self) -> bool {
        self.embedding_service.is_available()
    }

    pub fn embedding_runtime_status(&self) -> Option<EmbeddingRuntimeStatus> {
        self.embedding_service.runtime_status()
    }
}
```

Dashboard template callers need to be checked to confirm `embedding_available()` is always called fresh (not cached into a template variable at render-start). If it is cached in a template context struct, also make that lookup re-read the service.

## File Changes Summary

| File | Change |
|---|---|
| `src/daemon/embedding_service.rs` | Replace immutable struct with state-machine version; add `wait_until_settled`, `publish_ready`, `publish_unavailable`; update `initialize_for_test` |
| `src/daemon/mod.rs` | `run_daemon`: construct service in `Initializing`, spawn background init task, move `embedding_model` sync into the background task, wrap `opener::open` in `tokio::spawn` |
| `src/tools/workspace/indexing/embeddings.rs` | `spawn_workspace_embedding`: replace daemon-mode silent-skip with `wait_until_settled(120s)` |
| `src/tools/search/nl_embeddings.rs` | `maybe_initialize_embeddings_for_nl_definitions`: split daemon-mode (short wait) and stdio-mode (unchanged) paths; prevent duplicate sidecar |
| `src/daemon/workspace_pool.rs` | Audit `shared_embedding_provider` for liveness; no fix expected but verify |
| `src/watcher/mod.rs` | Audit watcher provider caching; fix if watchers cache a `None` at attach time |
| `src/dashboard/state.rs` | Replace `embedding_available: bool` with an `Arc<EmbeddingService>` reference; make accessor re-read live |
| `src/dashboard/templates/*` (if any) | Audit for cached `embedding_available` values |

## Testing Strategy

**Unit tests (new):**

1. `EmbeddingService` state transitions:
   - `initializing() → provider() == None`
   - `initializing() → publish_ready(p) → provider() == Some(p)`
   - `initializing() → publish_unavailable(reason) → provider() == None && runtime_status() reflects reason`
   - `wait_until_settled` returns `Ready` after `publish_ready`, `Unavailable` after `publish_unavailable`, `Timeout` if the deadline passes with no publish.
   - Multiple concurrent `wait_until_settled` callers all receive the notify.

2. `spawn_workspace_embedding` with a fake service:
   - Service in `Initializing` for 1s then `publish_ready`: workspace embedding proceeds after the wait.
   - Service stays in `Initializing` past the 120s timeout: workspace embedding returns 0 with a warning (simulate with shorter test timeout).
   - Service in `Unavailable`: workspace embedding returns 0 immediately.

3. `maybe_initialize_embeddings_for_nl_definitions` daemon-mode behavior:
   - Daemon service in `Initializing`, publishes `Ready` within 1s: returns without triggering stdio init, subsequent `handler.embedding_provider()` returns the published provider.
   - Daemon service in `Initializing`, stays past the 3s timeout: returns without triggering stdio init.
   - `handler.embedding_service` is `None` (stdio mode): existing stdio init path runs.
   - Counter from `take_nl_definition_embedding_init_attempts` stays at 0 for daemon-mode cases.

**Integration test (new):**

4. Daemon startup with a mocked slow-init embedding service:
   - Spawn daemon with a fake provider factory that sleeps 2s before returning.
   - Assert `Daemon listening for IPC connections` logged within 1s of `Daemon PID file created`.
   - Connect a session and call `fast_search(search_target="content")` — must succeed immediately.
   - Wait for the background init to complete, then call `fast_search(search_target="definitions")` — must use the now-ready provider.

**Regression tests (existing, must still pass):**

5. `test_daemon_starts_and_creates_pid_file` at `src/tests/daemon/server.rs:35`
6. `test_daemon_starts_creates_pid_and_socket_then_stops` at `src/tests/integration/daemon_lifecycle.rs:49`
7. Existing embedding service tests (`test_embedding_service_unavailable_when_provider_none`, `test_embedding_service_initialize_with_provider_disabled`)

**Manual test:**

8. Reboot the machine. Open Claude Code. Verify `/mcp` shows Julie as connected on the first attempt with no manual reconnect.
9. Check `~/.julie/daemon.log.$(date +%Y-%m-%d)` and confirm the gap between `Daemon PID file created` and `Daemon listening for IPC connections` is under 2 seconds.

## Acceptance Criteria

- [ ] `EmbeddingService` has a working state machine with `Initializing` / `Ready` / `Unavailable` transitions
- [ ] `wait_until_settled` is implemented and unit-tested (Ready, Unavailable, Timeout cases)
- [ ] `run_daemon` constructs the service in `Initializing` and spawns a background init task
- [ ] `run_daemon` reaches `ready` state in <2 seconds from PID file creation (verified in `~/.julie/daemon.log.*` on a cold boot)
- [ ] `opener::open` is moved off the critical path via `tokio::spawn`
- [ ] `embedding_model` sync loop runs inside the background init task, not on the critical path
- [ ] `spawn_workspace_embedding` waits for the service to settle during the warmup window and successfully embeds workspaces that are auto-indexed during warmup
- [ ] `maybe_initialize_embeddings_for_nl_definitions` does not trigger the stdio fallback in daemon mode; NL queries during warmup correctly wait for the daemon service or degrade to keyword-only
- [ ] Watchers created during the warmup window correctly embed incremental changes after the provider is published (verified manually by modifying a file during the warmup window)
- [ ] `DashboardState::embedding_available()` correctly flips from `false` to `true` when the background init completes (verified by hitting `/` on the dashboard during and after warmup)
- [ ] Existing behavior for machines where the embedding provider genuinely fails to initialize is preserved (keyword-only fallback, no hang)
- [ ] Claude Code connects to Julie on the first attempt after a machine reboot (no `failed` status, no manual reconnect)
- [ ] All existing tests pass (`cargo xtask test dev` for dev tier, plus `cargo xtask test system` for daemon/startup changes)
- [ ] New unit and integration tests are added and passing

## Rollout

1. Implement behind a feature flag? **No.** This is a correctness fix with no user-visible API surface. Straight land on main.
2. Version bump to `6.6.11` per project convention (Cargo.toml + gh-pages site in sync — see CLAUDE.md "Version bumps").
3. Verify dogfooding before release: reboot the dev machine and confirm Claude Code connects cleanly.

## Appendix: Log Evidence

Post-reboot cold start, `~/.julie/daemon.log.2026-04-09`:

```
23:12:55.397  Starting Julie daemon v6.6.10                                 (main.rs:44)
23:12:55.399  Daemon PID file created pid=6108                              (mod.rs:286)
23:12:55.432  Daemon database ready
23:12:55.440  Initializing shared embedding service...
23:13:34.512  Embedding provider initialized (cuda, 768d)                   (init.rs:104)
23:13:34.513  Shared embedding service initialized
23:13:34.531  Dashboard HTTP server started port=7890
23:13:34.753  Daemon listening for IPC connections                          (mod.rs:487) READY
23:14:33.508  IPC headers received workspace=\\?\C:\source\julie            manual /mcp reconnect
```

Cold-start critical path breakdown:
- PID creation → embedding service start: 41 ms
- **Embedding service initialization: 39.07 s**
- Embedding service ready → dashboard up: 18 ms
- Dashboard up → IPC listener bound: 222 ms
- **Total PID → ready: 39.35 s**

Warm start for comparison, `~/.julie/daemon.log.2026-04-08`:

```
22:21:00.264  Daemon PID file created pid=28140
22:21:36.447  Shared embedding service initialized                          (36.18 s later)
22:21:36.694  Daemon listening for IPC connections
```

Cold vs warm: 39.35s vs 36.43s. Only a 3 second delta — model load time dominates, not disk caching.

## Related

- Daemon lifecycle robustness plan: `docs/superpowers/plans/2026-04-08-daemon-lifecycle.md`
- Daemon lifecycle design: `docs/superpowers/specs/2026-04-08-daemon-lifecycle-design.md`
- Adapter launcher: `src/adapter/launcher.rs`
- Previous fix in this area: commit `a92e61c2` (adapter IPC probe fallback), `2ba6de2c` (`daemon_readiness` refactor)
