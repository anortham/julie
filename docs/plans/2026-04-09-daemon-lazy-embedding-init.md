# Daemon Lazy Embedding Initialization Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use razorback:executing-plans to implement this plan task-by-task.

**Goal:** Move `EmbeddingService` initialization off the daemon's critical path so daemon `ready` time drops from ~39s to <2s, eliminating MCP connection failures on cold start.

**Architecture:** Replace `EmbeddingService`'s immutable fields with a `tokio::sync::watch`-backed state machine (`Initializing` / `Ready` / `Unavailable`). `run_daemon` constructs the service in `Initializing`, spawns a background task for the Python sidecar bootstrap, and proceeds immediately to `ready` state and IPC bind. Downstream callers (`spawn_workspace_embedding`, `nl_embeddings`, watchers, dashboard) become daemon-mode aware: they wait for the service to settle with bounded timeouts instead of silently skipping embedding or spawning duplicate sidecars.

**Tech Stack:** Rust, `tokio::sync::watch`, `tokio::spawn`, `tokio::task::spawn_blocking`.

**Spec:** `docs/plans/2026-04-09-daemon-lazy-embedding-init-design.md` — read this before starting Task 1.

**Testing:** After each task, run `cargo test --lib <test_name>` for the specific test you wrote. **Do NOT run `cargo xtask test dev` between tasks** — run it once at the end of Task 9 as the final regression pass. Per project testing rules, subagents use narrow filters; the orchestrating session runs xtask tiers.

---

## Task 1: `EmbeddingService` state machine

**Files:**
- Modify: `src/daemon/embedding_service.rs` (full rewrite of struct internals and API — lines 1-113)
- Modify: `src/daemon/mod.rs` — audit `write_daemon_state` re-exports if affected (no expected change)
- Test: `src/daemon/embedding_service.rs` `#[cfg(test)] mod tests` or `src/tests/daemon/embedding_service_state.rs` (new)

**What to build:** Interior-mutable `EmbeddingService` backed by `tokio::sync::watch`. Three states: `Initializing`, `Ready { provider, runtime_status }`, `Unavailable { reason }`. Non-blocking accessors and an async `wait_until_settled` with timeout. This is the foundation every other task depends on.

**Approach:**

- Replace the struct with:
  ```rust
  pub struct EmbeddingService {
      state_tx: tokio::sync::watch::Sender<EmbeddingServiceState>,
      state_rx: tokio::sync::watch::Receiver<EmbeddingServiceState>,
  }

  #[derive(Clone)]
  pub enum EmbeddingServiceState {
      Initializing,
      Ready {
          provider: Arc<dyn EmbeddingProvider>,
          runtime_status: EmbeddingRuntimeStatus,
      },
      Unavailable {
          reason: String,
      },
  }

  pub enum EmbeddingServiceSettled {
      Ready(Arc<dyn EmbeddingProvider>),
      Unavailable(String),
      Timeout,
  }
  ```
- `EmbeddingRuntimeStatus` at `src/embeddings/mod.rs:52` already derives `Clone`, so no wrapping needed.
- Public API:
  - `initializing() -> Self` — construct with `watch::channel(Initializing)`. Holds both `Sender` and a persistent `Receiver` so `provider()`/`is_available()` work without cloning each time.
  - `publish_ready(&self, provider: Arc<dyn EmbeddingProvider>, runtime_status: EmbeddingRuntimeStatus)` — `self.state_tx.send_replace(Ready { .. })`. Ignore the returned previous state.
  - `publish_unavailable(&self, reason: String)` — `self.state_tx.send_replace(Unavailable { reason })`.
  - `provider(&self) -> Option<Arc<dyn EmbeddingProvider>>` — `self.state_rx.borrow()` and match on `Ready` to clone the `Arc`.
  - `runtime_status(&self) -> Option<EmbeddingRuntimeStatus>` — borrow and clone if `Ready`.
  - `is_available(&self) -> bool` — borrow and `matches!(*state, Ready { .. })`.
  - `is_settled(&self) -> bool` — borrow and `!matches!(*state, Initializing)`.
  - `async fn wait_until_settled(&self, timeout: Duration) -> EmbeddingServiceSettled` — clone the receiver, check `borrow()` first (fast path for already-settled), loop on `changed().await` wrapped in `tokio::time::timeout(timeout, ...)`. On settlement, return `Ready(p)` / `Unavailable(reason)`. On timeout elapsed, return `Timeout`.
- Keep `shutdown(&self)` as a no-op or log-only — the `watch::Sender` drops automatically when the service is dropped.
- Keep `initialize_for_test(provider: Option<Arc<dyn EmbeddingProvider>>)` for existing tests. It should construct a service, then if `Some(p)` provided call `publish_ready` with a synthetic `EmbeddingRuntimeStatus`, else call `publish_unavailable("test: provider disabled")`. Existing tests at lines 73-112 must still pass unchanged.

**Why watch over `RwLock + Notify`:** `Notify::notify_waiters()` is edge-triggered — a notification fired before a waiter enrolls is lost, causing the waiter to hang indefinitely. `watch::Receiver::changed().await` tracks version numbers and fires for any unseen update, including one that happened before `await`. See design doc section 1 for full rationale.

**Acceptance criteria:**
- [ ] `EmbeddingService` struct uses `watch::channel`-backed state machine as specified
- [ ] All public API methods listed above exist with correct signatures
- [ ] `wait_until_settled` returns `Ready` immediately when state is already `Ready` before call (no hang)
- [ ] `wait_until_settled` returns `Ready` after concurrent `publish_ready` even if `publish_ready` fires before the await begins
- [ ] `wait_until_settled` returns `Timeout` if state stays `Initializing` past the deadline
- [ ] `wait_until_settled` returns `Unavailable(reason)` after `publish_unavailable`
- [ ] Multiple concurrent `wait_until_settled` callers all receive the same settlement result
- [ ] Existing tests `test_embedding_service_unavailable_when_provider_none` and `test_embedding_service_initialize_with_provider_disabled` at `src/daemon/embedding_service.rs:77-112` still pass (update if API changes required)
- [ ] New unit tests for each state transition and each `wait_until_settled` outcome (4+ tests)
- [ ] `cargo test --lib embedding_service` passes
- [ ] Commit: `refactor(daemon): embedding service state machine via watch channel`

---

## Task 2: `run_daemon` background init + browser launch off critical path

**Files:**
- Modify: `src/daemon/mod.rs` — `run_daemon` function, lines 276-491
- Modify: `src/daemon/mod.rs` — imports (likely add `tokio::spawn`, already has `tokio::task::spawn_blocking`)

**What to build:** Restructure `run_daemon` to construct `EmbeddingService` in `Initializing` state synchronously, spawn a background task that performs the real provider initialization, and proceed immediately to IPC bind and `ready` state. Move the dashboard browser launch off the critical path.

**Approach:**

- Replace the current blocking init at `src/daemon/mod.rs:329-338`:
  ```rust
  // OLD (lines 329-338)
  let embedding_service = Arc::new(
      tokio::task::spawn_blocking(|| EmbeddingService::initialize())
          .await
          .context("Embedding service initialization panicked")?,
  );
  info!(available = embedding_service.is_available(), ...);
  ```
  With:
  ```rust
  // NEW
  let embedding_service = Arc::new(EmbeddingService::initializing());
  info!("Embedding service constructed in Initializing state; background init will start after IPC bind");
  ```
- Remove the `embedding_model` sync loop currently at lines 340-362. Move it into the background init task (described below).
- After `write_daemon_state("ready")` at line 491, and BEFORE the `tokio::select!` accept loop at line 494, spawn the background init task:
  ```rust
  // Clone what the background task needs
  let embedding_service_for_init = Arc::clone(&embedding_service);
  let daemon_db_for_init = daemon_db.clone();
  tokio::spawn(async move {
      let init_result = tokio::task::spawn_blocking(|| {
          crate::embeddings::init::create_embedding_provider()
      })
      .await;

      match init_result {
          Ok(Ok((provider, runtime_status))) => {
              info!("Background embedding init succeeded; publishing Ready");
              embedding_service_for_init.publish_ready(provider.clone(), runtime_status);

              // Sync embedding_model for workspaces that need it
              // (moved from run_daemon lines 340-362)
              if let Some(ref db) = daemon_db_for_init {
                  let model = provider.device_info().model_name.clone();
                  if let Ok(workspaces) = db.list_workspaces() {
                      let mut count = 0;
                      for ws in &workspaces {
                          if ws.vector_count.map_or(false, |v| v > 0)
                              && ws.embedding_model.as_deref() != Some(model.as_str())
                          {
                              let _ = db.update_embedding_model(&ws.workspace_id, &model);
                              count += 1;
                          }
                      }
                      if count > 0 {
                          info!(count, model, "Synced embedding_model for workspaces");
                      }
                  }
              }
          }
          Ok(Err(e)) => {
              warn!(error = ?e, "Background embedding init failed; publishing Unavailable");
              embedding_service_for_init.publish_unavailable(format!("init failed: {}", e));
          }
          Err(join_err) => {
              warn!(error = ?join_err, "Background embedding init task panicked; publishing Unavailable");
              embedding_service_for_init.publish_unavailable(format!(
                  "init task panicked/cancelled: {}",
                  join_err
              ));
          }
      }
  });
  ```
- **Critical:** The `Err(join_err)` arm MUST publish `Unavailable`. Without it, a panicking background task leaves the service stuck in `Initializing` forever.
- Verify `create_embedding_provider` returns the `(Arc<dyn EmbeddingProvider>, EmbeddingRuntimeStatus)` tuple. Check `src/embeddings/init.rs` and adjust the destructuring if the actual signature differs. If the signature is different from what's assumed, adapt the match arms accordingly — the key invariant is: on success, publish_ready; on any failure mode, publish_unavailable.
- Move `opener::open(&dashboard_url)` off the critical path. Change lines 464-469 from:
  ```rust
  if !no_dashboard {
      if let Err(e) = opener::open(&dashboard_url) {
          warn!("Failed to open browser: {}", e);
      }
  }
  ```
  To:
  ```rust
  if !no_dashboard {
      let url = dashboard_url.clone();
      tokio::spawn(async move {
          if let Err(e) = opener::open(&url) {
              warn!("Failed to open browser: {}", e);
          }
      });
  }
  ```
- The `WorkspacePool::new` call at lines 382-387 already takes `Some(Arc::clone(&embedding_service))`. No change needed — it receives the `Initializing` service and will see the live state via `shared_embedding_provider()` once Task 5 lands (dashboard) and watchers pick up the change (Task 6).

**Acceptance criteria:**
- [ ] `run_daemon` constructs `EmbeddingService::initializing()` synchronously
- [ ] Background init task is spawned after `write_daemon_state("ready")` using `tokio::spawn`
- [ ] Background task correctly publishes `Ready` on success with provider and runtime_status
- [ ] Background task publishes `Unavailable` on `create_embedding_provider` error
- [ ] Background task publishes `Unavailable` on `JoinError` (panic/cancel)
- [ ] `embedding_model` workspace sync loop is moved inside the background task, runs after `publish_ready`
- [ ] `opener::open` is wrapped in `tokio::spawn` and does not block IPC listener bind
- [ ] The critical path between `Daemon PID file created` log and `Daemon listening for IPC connections` log is under 2 seconds on a development machine (run `cargo build --release`, stop any running daemon, spawn `julie-server daemon` from terminal, observe log timestamps). If it's still slow, identify the blocker before moving on.
- [ ] `cargo test --lib daemon::` passes
- [ ] Commit: `feat(daemon): lazy-init embedding service in background task`

---

## Task 3: `spawn_workspace_embedding` waits for service settlement

**Files:**
- Modify: `src/tools/workspace/indexing/embeddings.rs` — `spawn_workspace_embedding` function, lines 30-40 specifically
- Modify: imports (add `std::time::Duration`, `crate::daemon::embedding_service::EmbeddingServiceSettled`)

**What to build:** Replace the current silent-skip path that bails with `return 0` when `handler.embedding_service.is_some()` but no provider is available. Instead, wait on the service to settle with a bounded 120-second timeout, then use the resulting provider or skip if unavailable.

**Approach:**

Current code at `src/tools/workspace/indexing/embeddings.rs:30-40`:
```rust
let provider = if let Some(p) = handler.embedding_provider().await {
    p
} else if handler.embedding_service.is_some() {
    debug!("Daemon mode but no embedding provider available, skipping workspace embedding");
    return 0;
} else {
    // stdio mode: initialize now
    ...
};
```

Replace the `else if handler.embedding_service.is_some()` branch with:
```rust
} else if let Some(svc) = handler.embedding_service.as_ref() {
    // Daemon mode. Wait for the service to settle (up to 120s — background
    // task, not user-facing, we'd rather wait than skip embedding).
    match svc.wait_until_settled(Duration::from_secs(120)).await {
        EmbeddingServiceSettled::Ready(p) => {
            debug!("Daemon embedding service became ready; proceeding with workspace embedding");
            p
        }
        EmbeddingServiceSettled::Unavailable(reason) => {
            debug!(%reason, "Daemon embedding service unavailable, skipping workspace embedding");
            return 0;
        }
        EmbeddingServiceSettled::Timeout => {
            warn!("Daemon embedding service did not settle within 120s, skipping workspace embedding");
            return 0;
        }
    }
} else {
    // stdio mode (unchanged)
    ...
}
```

Do not touch the stdio branch. It's only exercised when `handler.embedding_service` is `None`, which is stdio mode — this path is unchanged.

**Acceptance criteria:**
- [ ] Daemon mode path in `spawn_workspace_embedding` calls `wait_until_settled(Duration::from_secs(120))`
- [ ] `EmbeddingServiceSettled::Ready(p)` branch proceeds with embedding
- [ ] `EmbeddingServiceSettled::Unavailable` branch returns 0 with debug log
- [ ] `EmbeddingServiceSettled::Timeout` branch returns 0 with warning log
- [ ] stdio mode path (when `handler.embedding_service.is_none()`) is unchanged and still calls the per-workspace lazy init
- [ ] New test: fake handler with embedding service in `Initializing`, publishes `Ready` after 100ms, assert `spawn_workspace_embedding` waits and then proceeds. Use a test helper or a short `wait_until_settled` timeout if 120s is impractical in a test.
- [ ] New test: fake handler with embedding service publishes `Unavailable`, assert `spawn_workspace_embedding` returns 0 without hanging
- [ ] `cargo test --lib spawn_workspace_embedding` passes
- [ ] Commit: `feat(indexing): wait for embedding service settlement instead of silent skip`

---

## Task 4: `nl_embeddings` daemon-mode path split

**Files:**
- Modify: `src/tools/search/nl_embeddings.rs` — `maybe_initialize_embeddings_for_nl_definitions`, lines 41-141

**What to build:** Split daemon-mode and stdio-mode paths explicitly. In daemon mode, wait up to 3 seconds on the shared service to settle. Do NOT fall through to the per-workspace stdio init in daemon mode — that would spawn a duplicate Python sidecar.

**Approach:**

Current logic at `src/tools/search/nl_embeddings.rs:52-54`:
```rust
if handler.embedding_provider().await.is_some() {
    return;
}
// No provider yet. In daemon mode the shared service would have returned
// one, so this is either stdio mode or a transient initialization gap.
```

The comment is wrong under the new design — in daemon mode during warmup, the provider legitimately returns `None`. The fall-through to per-workspace init would spawn a second sidecar.

Replace with:
```rust
// Fast path: provider already available
if handler.embedding_provider().await.is_some() {
    return;
}

// Daemon mode: wait briefly on the shared service. NL queries are
// interactive — don't block for more than a few seconds. If the shared
// service isn't ready in time, degrade to keyword-only by returning.
// Do NOT fall through to the per-workspace stdio init path: that would
// spawn a duplicate Python sidecar alongside the daemon's shared one.
if let Some(svc) = handler.embedding_service.as_ref() {
    match svc.wait_until_settled(std::time::Duration::from_secs(3)).await {
        crate::daemon::embedding_service::EmbeddingServiceSettled::Ready(_) => {
            // Provider is now published; caller's next
            // handler.embedding_provider() will see it. Return without
            // running the stdio lazy-init path.
            return;
        }
        crate::daemon::embedding_service::EmbeddingServiceSettled::Unavailable(reason) => {
            debug!(%reason, "Daemon embedding service unavailable; NL query falls back to keyword-only");
            return;
        }
        crate::daemon::embedding_service::EmbeddingServiceSettled::Timeout => {
            debug!("Daemon embedding service did not settle within 3s for NL query; falling back to keyword-only");
            return;
        }
    }
}

// Stdio mode: no daemon service exists. Fall through to the existing
// per-workspace lazy init path (unchanged from current behavior).
```

Then leave the rest of the function (lines 56-141) unchanged — it's the stdio path and only reached when `handler.embedding_service` is `None`.

**Acceptance criteria:**
- [ ] Daemon-mode path in `maybe_initialize_embeddings_for_nl_definitions` calls `wait_until_settled(Duration::from_secs(3))`
- [ ] All three settlement branches return without falling through to stdio init
- [ ] Stdio path (when `handler.embedding_service.is_none()`) is unchanged
- [ ] `record_nl_definition_embedding_init_attempt` at line 96 is NOT called in daemon-mode paths (only reached via stdio fall-through)
- [ ] New test (daemon mode): handler with embedding service in `Initializing`, publishes `Ready` after 100ms, assert function returns without calling `record_nl_definition_embedding_init_attempt`. Use `take_nl_definition_embedding_init_attempts` at line 30 to verify count stays 0.
- [ ] New test (daemon mode): handler with embedding service stays `Initializing` past 3s, assert function returns without calling `record_nl_definition_embedding_init_attempt`.
- [ ] Existing stdio-mode test (if present in `src/tests/tools/search/`) still passes
- [ ] `cargo test --lib nl_embeddings` passes
- [ ] Commit: `fix(search): prevent duplicate sidecar spawn in daemon-mode NL queries`

---

## Task 5: `DashboardState` live `embedding_available`

**Files:**
- Modify: `src/dashboard/state.rs` — `DashboardState` struct (around line 39) and `DashboardState::new` (line 55) and `embedding_available()` accessor
- Modify: `src/daemon/mod.rs` — call site at line 424 where `DashboardState::new` is invoked
- Audit: `src/dashboard/templates/*.html` or template context structs for cached `embedding_available` values

**What to build:** Replace the static `embedding_available: bool` field on `DashboardState` with an `Arc<EmbeddingService>` reference. The `embedding_available()` accessor calls the service live on each request so the dashboard reflects real-time state as the background init completes.

**Approach:**

- In `src/dashboard/state.rs` around line 39-45 where `DashboardState` is defined, replace:
  ```rust
  // OLD
  embedding_available: bool,
  ```
  With:
  ```rust
  // NEW
  embedding_service: Arc<crate::daemon::embedding_service::EmbeddingService>,
  ```
- Change the `DashboardState::new` signature at line 55. Replace the `embedding_available: bool` parameter with `embedding_service: Arc<EmbeddingService>`:
  ```rust
  pub fn new(
      sessions: Arc<SessionTracker>,
      daemon_db: Option<Arc<DaemonDatabase>>,
      restart_pending: Arc<AtomicBool>,
      start_time: Instant,
      embedding_service: Arc<crate::daemon::embedding_service::EmbeddingService>,  // CHANGED
      workspace_pool: Option<Arc<WorkspacePool>>,
      error_buffer_capacity: usize,
  ) -> Self
  ```
- Update the body of `new` to store `embedding_service` instead of the `bool`.
- Change the accessor `pub fn embedding_available(&self) -> bool` (around line 64-66 per earlier get_symbols output):
  ```rust
  pub fn embedding_available(&self) -> bool {
      self.embedding_service.is_available()
  }
  ```
- Add a new accessor for runtime status:
  ```rust
  pub fn embedding_runtime_status(&self) -> Option<crate::embeddings::EmbeddingRuntimeStatus> {
      self.embedding_service.runtime_status()
  }
  ```
- In `src/daemon/mod.rs` at line 424-432, update the `DashboardState::new` call site:
  ```rust
  // OLD
  let dashboard_state = crate::dashboard::state::DashboardState::new(
      Arc::clone(&sessions),
      daemon_db.clone(),
      Arc::clone(&restart_pending),
      std::time::Instant::now(),
      embedding_service.is_available(),  // REMOVE
      Some(Arc::clone(&pool)),
      50,
  );
  ```
  to:
  ```rust
  // NEW
  let dashboard_state = crate::dashboard::state::DashboardState::new(
      Arc::clone(&sessions),
      daemon_db.clone(),
      Arc::clone(&restart_pending),
      std::time::Instant::now(),
      Arc::clone(&embedding_service),  // CHANGED: pass the service itself
      Some(Arc::clone(&pool)),
      50,
  );
  ```
- Audit dashboard templates and context structs for places that cache `embedding_available` into a render-time variable. Use `fast_search(query="embedding_available", file_pattern="src/dashboard/**")`. If any template context struct copies the value once at render start, change it to a live lookup — the whole point of this task is that the dashboard reflects state changes without a restart.

**Acceptance criteria:**
- [ ] `DashboardState` struct holds `Arc<EmbeddingService>` instead of `bool`
- [ ] `DashboardState::new` signature accepts `Arc<EmbeddingService>` instead of `bool`
- [ ] `embedding_available()` method re-reads `EmbeddingService::is_available()` on every call (verify by test: construct service in Initializing → assert false, publish_ready → assert true, no reconstruction needed)
- [ ] Call site in `run_daemon` passes `Arc::clone(&embedding_service)` instead of `embedding_service.is_available()`
- [ ] Dashboard templates reflect real-time state — no cached bool copies in render contexts
- [ ] New test: construct `DashboardState` with a service in `Initializing`, assert `embedding_available() == false`. Call `publish_ready` on the underlying service, assert `embedding_available() == true`.
- [ ] `cargo test --lib dashboard::state` passes
- [ ] Commit: `feat(dashboard): live embedding_available status from EmbeddingService`

---

## Task 6: Watcher provider propagation audit + fix

**Files:**
- Audit: `src/watcher/mod.rs` — specifically around `update_embedding_provider` at line 295 and its callers, plus the watcher's embedding-consuming code paths
- Audit: `src/daemon/workspace_pool.rs` — `shared_embedding_provider` (already confirmed live) and how watchers receive the provider at attach time
- Modify: depends on audit findings

**What to build:** Audit first, then fix if needed. The question is: do watchers cache a snapshot of the embedding provider at attach time (bad — a watcher created during warmup will see `None` forever) or do they re-read the provider from a shared source on each file event (good)?

**Approach:**

- Run `get_symbols(file_path="src/watcher/mod.rs", max_depth=2)` to map the watcher's structure.
- Use `deep_dive(symbol="update_embedding_provider", depth="context")` to see callers.
- Check how `WorkspacePool::attach_watcher` (or whatever the equivalent is) passes the provider into a new watcher. Trace: does it call `watcher.update_embedding_provider(svc.provider())` once, or does the watcher hold an `Arc<EmbeddingService>` and call `svc.provider()` on each event?
- **Decision tree:**
  - **If the watcher holds an `Arc<EmbeddingService>`** and re-reads on each file event: no change needed. Verify by inspecting the code path that runs on file modifications and confirming it calls `service.provider()` at event time, not from cached state.
  - **If the watcher caches an `Option<Arc<dyn EmbeddingProvider>>`** from its initial construction: fix by adding a push mechanism. In `EmbeddingService`, maintain a list of `Weak<tokio::sync::Notify>` or callback registrations that `publish_ready` walks to notify interested watchers. Simpler alternative: have `WorkspacePool` observe the service's `watch::Receiver` in a background task, and when state changes to `Ready`, iterate all active watchers and call `Watcher::update_embedding_provider(Some(provider))` to push the new provider.
- The simpler alternative is strongly preferred. Implementation sketch if needed:
  ```rust
  // In run_daemon, after spawning the background init task:
  let pool_for_watcher_sync = Arc::clone(&pool);
  let service_for_watcher_sync = Arc::clone(&embedding_service);
  tokio::spawn(async move {
      let mut rx = service_for_watcher_sync.subscribe(); // add this helper to EmbeddingService if needed
      while rx.changed().await.is_ok() {
          let provider = service_for_watcher_sync.provider();
          pool_for_watcher_sync.update_all_watcher_providers(provider).await;
      }
  });
  ```
  Where `WorkspacePool::update_all_watcher_providers` iterates its internal watcher map and calls `Watcher::update_embedding_provider` on each.
- If the audit reveals the watcher already re-reads live, skip the fix and just document the finding in the commit message for future clarity.
- If `EmbeddingService` needs a `subscribe()` helper that returns a cloned `watch::Receiver`, add it to Task 1's API surface and update the state machine doc in the design doc to match. Small change.

**Acceptance criteria:**
- [ ] Audit complete: document in commit message whether watchers cached or re-read live
- [ ] If cached: push mechanism implemented and tested (watchers created during warmup receive the provider when `publish_ready` fires)
- [ ] If re-reading live: no code change, but add a comment at the relevant line documenting the invariant so future refactors preserve it
- [ ] Manual verification: start daemon, modify a file in a workspace during the ~1-second warmup window, verify the watcher eventually processes the change with embedding after the background init completes. Confirm via dashboard or by checking `embedding_count` via `query_metrics`.
- [ ] `cargo test --lib watcher::` passes
- [ ] Commit: `fix(watcher): propagate embedding provider to watchers during daemon warmup` (if fix applied) OR `docs(watcher): document live embedding provider invariant` (if audit only)

---

## Task 7: Integration test for lazy daemon startup

**Files:**
- Create: `src/tests/integration/daemon_lazy_embedding_init.rs` OR add to existing `src/tests/integration/daemon_lifecycle.rs`
- Modify: `src/tests/integration/mod.rs` to register the new test module if a new file is created

**What to build:** End-to-end test that spawns a daemon with a mocked slow-init embedding service, asserts the daemon reaches `ready` state before the embedding service settles, and verifies that keyword-only search works immediately while semantic search waits correctly.

**Approach:**

- The test needs to be able to inject a slow provider factory. Look at how the existing `test_daemon_starts_creates_pid_and_socket_then_stops` test at `src/tests/integration/daemon_lifecycle.rs:49` constructs a daemon. If there's no existing hook for mocking `create_embedding_provider`, add one via a test-only config or a feature flag (`#[cfg(test)]`).
- Test flow:
  1. Spawn daemon via a test helper that uses a fake embedding provider factory that sleeps 2 seconds before returning a stub `EmbeddingProvider`.
  2. Immediately (before 2 seconds elapse) read the `daemon.state` file. Assert it contains `ready` — this proves the daemon didn't wait on embedding init.
  3. Connect a session over IPC. Call a tool that uses keyword search only (e.g., `fast_search` with a literal match). Assert it returns results.
  4. Wait up to 5 seconds for the embedding service to settle. Assert `embedding_service.is_available()` transitions to `true`.
  5. Call a tool that uses semantic search. Assert it returns results (proves the provider was published and picked up correctly).
  6. Shut down the daemon cleanly.
- The stub `EmbeddingProvider` can return deterministic fake vectors — it doesn't need real embeddings, just a non-panicking implementation of the trait.
- This test is expensive (2+ seconds of wall time). Put it in `system` or `integration` tier, not `dev`, so it doesn't slow down the default test loop.

**Acceptance criteria:**
- [ ] Integration test exists and is wired into the test module tree
- [ ] Test spawns a daemon with a mocked 2-second slow provider factory
- [ ] Test asserts `daemon.state == "ready"` within 1 second of PID file creation (before provider finishes)
- [ ] Test asserts keyword-only search works during the warmup window
- [ ] Test asserts embedding service transitions to `Ready` after the factory completes
- [ ] Test asserts semantic search works after the transition
- [ ] Test cleans up the daemon on finish (no leaked PID or state files)
- [ ] `cargo xtask test system` passes (this is the tier that covers workspace init and daemon lifecycle)
- [ ] Commit: `test(daemon): integration coverage for lazy embedding init`

---

## Task 8: Full regression pass and manual verification

**Files:** None — verification only.

**What to build:** Nothing. This task is the final sweep before shipping.

**Approach:**

1. Run `cargo xtask test dev` — assert green.
2. Run `cargo xtask test system` — assert green (this is the tier that covers daemon lifecycle).
3. Run `cargo xtask test dogfood` only if you touched any search scoring or tokenization (you shouldn't have — this task is off the critical path for this plan). Skip otherwise.
4. **Manual reboot test** — this is the acceptance criterion the whole plan exists for:
   a. Ask the user to exit all Claude Code sessions (Windows binary lock constraint — see CLAUDE.md).
   b. Run `cargo build --release`.
   c. Ask the user to reboot the machine.
   d. User opens Claude Code.
   e. User runs `/mcp` and confirms Julie shows as connected on the first attempt, not `failed`.
   f. User pastes `~/.julie/daemon.log.$(date +%Y-%m-%d)` timestamps for the cold start. Assert the gap between `Daemon PID file created` and `Daemon listening for IPC connections` is under 2 seconds.
5. If the reboot test passes, report success and prepare for Task 9.
6. If it fails, diagnose from the log. Likely suspects: `create_embedding_provider` signature mismatch in Task 2, watcher not receiving the provider (Task 6), dashboard template caching (Task 5). Fix and re-run.

**Acceptance criteria:**
- [ ] `cargo xtask test dev` passes
- [ ] `cargo xtask test system` passes
- [ ] Manual reboot test: Claude Code connects to Julie on first attempt
- [ ] Manual reboot test: daemon log shows <2 second cold-start
- [ ] No `failed` status, no manual reconnect needed

---

## Task 9: Version bump and release prep

**Files:**
- Modify: `Cargo.toml` — version `6.6.10` → `6.6.11`
- Modify: gh-pages site source (per CLAUDE.md "Version bumps" guidance — all three must stay in sync: `Cargo.toml`, `plugin.json` via CI, and the gh-pages site)

**What to build:** Version bump, commit, and prepare for release tagging.

**Approach:**

- Bump `Cargo.toml` version to `6.6.11`.
- Locate the gh-pages site source (look in `docs/` — per CLAUDE.md, the version is displayed there). Use `fast_search(query="6.6.10", file_pattern="docs/**")` to find references.
- Update any hardcoded version strings to `6.6.11`.
- `plugin.json` updates automatically via CI per CLAUDE.md, so don't touch it.
- Commit: `chore: bump version to 6.6.11`
- Inform the user that the release is ready to tag. Do NOT tag or push without explicit user authorization — per user guidance in CLAUDE.md global instructions, release steps are destructive and need approval.

**Acceptance criteria:**
- [ ] `Cargo.toml` version is `6.6.11`
- [ ] gh-pages site version reference updated to `6.6.11`
- [ ] `cargo build --release` succeeds with new version
- [ ] Commit: `chore: bump version to 6.6.11`
- [ ] User informed release is ready; no unauthorized tag/push

---

## Summary

**Task order:** 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9. Sequential. Task 1 is the foundation; Tasks 2-5 depend on it; Task 6 is an audit-first task that may or may not need changes; Task 7 ties it all together; Task 8 is the final sweep; Task 9 ships it.

**Parallelization potential:** Tasks 2, 3, 4, 5, 6 are independent once Task 1 lands (they touch different files and don't share state). If we later decide to parallelize via team-driven-development, Task 1 is the bottleneck; everything after can fan out. For now, sequential is simpler and the total change size is small enough that parallelism overhead isn't worth it.

**Rollback plan:** Each task commits independently. If Task 8 manual reboot test fails, revert the problem commit via `git revert` and re-diagnose. No destructive operations.
