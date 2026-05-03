# Daemon Lifecycle Controller Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Refactor daemon startup, readiness, draining, restart handoff, and shutdown ownership into one lifecycle controller so `run_daemon` and `accept_loop` stop sharing lifecycle state by loose parameters.

**Architecture:** Introduce a focused controller in `src/daemon/lifecycle.rs` that owns `LifecyclePhase`, `restart_pending`, restart notification, state-file publication, and session-drain decisions. Keep `SessionTracker` as the source of active session count and keep `WorkspacePool` ownership where it already belongs, passing it only where session handling needs workspace access. `src/daemon/mod.rs::run_daemon` should become orchestration glue: construct dependencies, bind transports, start background services, call the controller, and perform final cleanup.

**Tech Stack:** Rust, Tokio, `std::sync::{Arc, RwLock}`, `AtomicBool`, `Notify`, existing daemon IPC, existing `SessionTracker`, existing `WorkspacePool`, existing `DaemonPaths`.

---

## File Structure

- Modify `src/daemon/lifecycle.rs`: add the lifecycle controller type and keep pure decision functions such as `transition`, `stale_binary_accept_action`, `version_gate_action`, and `stale_binary_disconnect_action` as testable helpers. This file should own phase publication and restart-pending mutation.
- Modify `src/daemon/mod.rs:276-733`: shrink `run_daemon` by moving lifecycle state construction, startup-ready publication, restart drain decisions, and shutdown phase transitions behind the controller.
- Modify `src/daemon/mod.rs:740-1034`: reduce `accept_loop` parameters by passing the controller instead of `daemon_phase`, `daemon_state_path`, `restart_pending`, and `restart_notify` separately.
- Modify `src/adapter/launcher.rs:71-191`: update readiness assumptions only if the state-file values or draining semantics change. Preserve current behavior where `DaemonReadiness::Ready` accepts `ready` and treats `draining` as connectable for restart handoff.
- Modify `src/adapter/mod.rs:81-186`: only adjust retry expectations if controller responses alter immediate disconnect behavior.
- Test `src/tests/daemon/lifecycle.rs`: add controller unit tests for phase publication, restart-pending idempotence, and sessions-drained transition.
- Test `src/tests/integration/daemon_lifecycle.rs`: preserve existing integration coverage around PID/socket creation, ready-before-embedding-init, and graceful stop. Add one integration test only if unit tests cannot prove a drain or restart invariant.
- Test `src/tests/adapter/launcher.rs` and `src/tests/adapter/retry.rs`: update only when readiness or handoff observable behavior changes.

## Implementation Tasks

### Task 1: Controller API and State Ownership

**Files:**
- Modify: `src/daemon/lifecycle.rs:22-285`
- Test: `src/tests/daemon/lifecycle.rs:108-202`

**What to build:** Add a `DaemonLifecycleController` or similarly named type that owns the current `LifecyclePhase`, `restart_pending: AtomicBool`, `restart_notify: Notify`, and daemon state-file path. It should expose small methods for `startup_complete`, `request_shutdown`, `sessions_drained`, `incoming_session_action`, `mark_restart_pending`, and read-only phase snapshots for dashboard state.

**Approach:** Keep the existing pure transition functions. The controller wraps them and handles side effects: `publish_phase`, `store_phase`, atomic restart flag updates, and restart notification. Do not move PID file handling, database migration, embedding service bootstrap, watcher pool creation, or workspace pool creation into the controller.

**Acceptance criteria:**
- [ ] `run_daemon` no longer constructs or mutates `Arc<StdRwLock<LifecyclePhase>>`, `Arc<AtomicBool>`, and `Arc<Notify>` as separate lifecycle ownership primitives.
- [ ] Existing decision helpers stay individually testable.
- [ ] Controller tests prove restart-pending is idempotent and publishes the correct phase for zero and nonzero active sessions.
- [ ] Worker-scope verification passes.

### Task 2: Refactor `run_daemon` Around Controller

**Files:**
- Modify: `src/daemon/mod.rs:276-733`
- Modify: `src/dashboard/state.rs:177-186` only if the dashboard state constructor needs a controller-derived phase handle instead of the raw phase lock
- Test: `src/tests/integration/daemon_lifecycle.rs:102-267`

**What to build:** Replace the hand-rolled phase variable and scattered `publish_phase` calls in `run_daemon` with controller calls. `run_daemon` should still own process-level resources: `PidFile`, `DaemonDatabase`, `EmbeddingService`, `WorkspacePool`, `WatcherPool`, dashboard server task, cleanup sweep task, and final cleanup.

**Approach:** Preserve current startup order: create PID, publish `starting`, open daemon database, migrate/backfill, create embedding service, bind IPC listener, publish `ready` before slow embedding init, then start accepting sessions. Keep the "ready before embedding init" invariant because `src/tests/integration/daemon_lifecycle.rs::test_daemon_reaches_ready_before_slow_embedding_init_completes` exists for that exact behavior.

**Acceptance criteria:**
- [ ] State-file transitions still publish `starting`, `ready`, `draining`, and `stopping` using the same string values consumed by `DaemonLauncher::daemon_readiness`.
- [ ] Shutdown cleanup still aborts reaper and cleanup sweep tasks, shuts down embedding service, cleans listener, removes port and state files, and cleans the PID file.
- [ ] Existing integration tests for daemon start and lazy embedding readiness pass.
- [ ] Worker-scope verification passes.

### Task 3: Refactor `accept_loop` Lifecycle Decisions

**Files:**
- Modify: `src/daemon/mod.rs:740-1034`
- Modify: `src/daemon/ipc_session.rs:245-400` only if the session lifecycle handle needs a controller-owned callback
- Test: `src/tests/daemon/lifecycle.rs:149-182`
- Test: `src/tests/daemon/ipc_session.rs:104-206` only if session cleanup hooks change

**What to build:** Replace the large lifecycle parameter cluster in `accept_loop` with the controller plus the real session and workspace dependencies. The loop should ask the controller how to handle stale binary, version mismatch, and disconnect events, then keep the existing stream/session behavior.

**Approach:** Keep `SessionTracker` responsible for `add_session`, `remove_session`, `active_count`, and `lifecycle_handle`. Keep `WorkspacePool` passed to `handle_ipc_session` unchanged unless a test proves lifecycle cleanup is coupled to workspace ownership. The controller should receive active counts from `SessionTracker`; it should not own the tracker.

**Acceptance criteria:**
- [ ] `accept_loop` has fewer lifecycle-only parameters and no direct `restart_pending.store` or phase lock writes.
- [ ] Version mismatch and stale binary behavior still follow `version_gate_action`, `stale_binary_accept_action`, and `stale_binary_disconnect_action`.
- [ ] Last-session disconnect after restart-pending still triggers daemon shutdown.
- [ ] Worker-scope verification passes.

### Task 4: Adapter Readiness Contract Check

**Files:**
- Review: `src/adapter/launcher.rs:71-191`
- Review: `src/adapter/mod.rs:96-186`
- Test: `src/tests/adapter/launcher.rs:246-293`
- Test: `src/tests/adapter/retry.rs:12-159`

**What to build:** Confirm adapter behavior still matches the controller state model. This task should be a test-first contract check: update production adapter code only when a failing adapter test proves the controller changed an observable readiness or handoff behavior.

**Approach:** `DaemonLauncher::daemon_readiness` is the external contract for adapter startup. Do not invent a second readiness format. If the controller changes state publication timing, update the adapter tests first and then the adapter.

**Acceptance criteria:**
- [ ] `DaemonReadiness::Ready` still covers the states the adapter may connect to safely.
- [ ] `run_adapter_with` still retries initial connection failures and immediate restart handoffs without fixed sleeps.
- [ ] Worker-scope verification passes.

## Verification Strategy

**Project source of truth:** `AGENTS.md`, `RAZORBACK.md`, and `docs/TESTING_GUIDE.md`.

**Worker red/green scope:** Workers run only the exact test they add or modify, for example `cargo nextest run --lib test_controller_marks_restart_pending_once 2>&1 | tail -10`, `cargo nextest run --lib test_daemon_reaches_ready_before_slow_embedding_init_completes 2>&1 | tail -10`, or `cargo nextest run --lib test_run_adapter_with_retries_immediate_disconnect_without_fixed_sleep 2>&1 | tail -10`.

**Worker ceiling:** `cargo nextest run --lib <exact_test_name> 2>&1 | tail -10`. Workers may run at most one RED and one GREEN command per fix cycle. Workers must not run `cargo xtask test changed`, `cargo xtask test dev`, `cargo xtask test system`, or broad `cargo nextest run --lib`.

**Worker gate invariant:** Each worker report must state the invariant the narrow test proves, such as "controller publishes Draining with RestartRequired when shutdown is requested with active sessions" or "adapter retries immediate disconnect handoff without sleeping."

**Lead affected-change scope:** After a coherent batch, the lead runs `cargo xtask test changed`. If lifecycle refactor changes daemon startup, IPC accept, or adapter readiness behavior, the lead also runs the narrow existing integration tests before the changed gate when debugging signal is needed.

**Branch gate:** The lead runs `cargo xtask test dev` once before handoff.

**Replay/metric evidence:** No replay or metric evidence is required. Hard gates are the narrow worker tests, `changed`, and `dev`.

**Escalation triggers:** Run `cargo xtask test reliability` if session drain, watcher lifecycle, daemon restart, or shutdown-event behavior changes. Run `cargo xtask test system` if startup, workspace initialization, daemon registry, or adapter launch behavior changes. Do not assign these gates to workers.

**Assigned verification failure:** Workers stop and report when assigned verification fails, unless this plan explicitly says to update that gate.

**Verification ledger:** Record invariant, command, scope label, commit SHA, result, and timestamp. If the same HEAD already has a passing ledger entry for the required scope, reuse that evidence instead of rerunning the same expensive gate.

## Model Routing

**Project source of truth:** `RAZORBACK.md`. Do not copy the global model table into this plan. If a local sentence conflicts with `RAZORBACK.md`, `RAZORBACK.md` wins.

**Plan-specific overrides:** `run_daemon`, `accept_loop`, restart-pending semantics, session drain, shutdown, and adapter readiness are shared-invariant work. Use the coupled implementation route from `RAZORBACK.md`, with Codex `gpt-5.3-codex high` as the default worker route and `gpt-5.3-codex xhigh` for race-prone restart or shutdown debugging.

**Worker eligibility:** Use implementation-tier workers only for isolated tests or local edits with narrow ownership. Use coupled implementation or lead-owned work for `run_daemon`, `accept_loop`, `SessionTracker` integration, restart-pending semantics, and shutdown behavior.

**Escalation triggers:** Escalate on repeated worker failure, hidden lifecycle invariants, changed public daemon behavior, weak tests, or any plausible race in restart, drain, or shutdown.

**Mechanical exclusion:** Mechanical workers cannot own failing tests, replay evidence, metrics, or acceptance gates. Split docs-only edits from evidence interpretation.

**Unsupported harness behavior:** If the harness cannot choose models per agent, use `inherit`, note it in the worker report, and continue.

## Task Decomposition

- Lead-owned lane: define controller API and review every lifecycle state transition. This is shared daemon behavior and should not be left to an unattended low-cost worker.
- Worker lane A: add focused controller unit tests in `src/tests/daemon/lifecycle.rs` for one transition group at a time.
- Worker lane B: refactor `accept_loop` call sites after controller API exists, with write scope limited to `src/daemon/mod.rs` and exact lifecycle tests.
- Worker lane C: confirm adapter readiness behavior, with write scope limited to adapter tests unless a failing test proves production changes are needed.
- Lead integration lane: run affected-change gates, inspect race-sensitive diffs, and decide whether `system` or `reliability` gates are required.

## Risks

- The current `run_daemon` order is load-bearing. Publishing `ready` too late will reintroduce MCP startup timeouts during slow embedding initialization.
- `draining` currently behaves as a connectable state for adapter restart handoff. Changing that silently would make rebuild cycles flaky.
- A controller that owns `SessionTracker` would overreach. The tracker already owns session count and handles; the controller should consume counts and publish lifecycle state.
- Moving too much into `lifecycle.rs` can create a new god file. Keep database migration, workspace pooling, watcher pooling, embedding bootstrap, and dashboard setup outside the controller.
- Race bugs may pass unit tests. Any change to last-session disconnect or restart-pending behavior needs lead review plus reliability or system gates.
