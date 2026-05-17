# Daemon Fragility — Code Cartography (subagent investigation)

**Scope:** Read-only investigation. Trace the daemon lifecycle / session-counter / adapter-restart paths and identify why `julie-server` rejects every init for 20+ minutes after a binary rebuild.

**Verdict in one line:** The fault is not "active_sessions never decrements." It is that **`DaemonLifecycleController::notify_restart()` has zero subscribers**, so even when the lifecycle decides "restart now" the daemon process never exits — the `restart_pending=true` state is a sticky billboard and every subsequent init bounces off the stale-binary gate.

---

## State machine

```
                              ┌─────────────────────────────────────────────┐
                              │   process exit (only triggers)              │
                              │  - SIGTERM/SIGINT  (Unix)                   │
                              │  - stop_notify     (Windows named event)    │
                              └─────────────────────────────────────────────┘
                                              ▲
                                              │ NOT WIRED ↑
                                              │
LifecyclePhase    Starting ──StartupComplete──▶ Ready ──ShutdownRequested──▶ Draining{cause} ──SessionsDrained──▶ Stopping{cause}
                                                                                                                       │
                                                                                                                       └─ written to daemon.state
                                                                                                                          read by adapter.daemon_readiness()

restart_pending   false ──mark_restart_pending(…)──▶ true   (one-way — never cleared)

restart_notify    Arc<Notify>  ── notify_restart() ──▶ … (NO `.notified()` await anywhere in the binary)

active_sessions   sessions.add_session()   on  HttpJulieService::handler_for_request() AFTER admit_initialize succeeds
                  sessions.remove_session() on  Drop of HttpJulieService (async-spawned via finish())

Adapter loop      HTTP transport error / restart_required JSON-RPC error
                    → ImmediateDaemonDisconnect → restart_handoff_action(attempt, MAX_RETRIES=5)
                    → retry: sleep(1,2,4,8,16) → ensure_daemon_ready() → re-init
                    Never kills the daemon. Total budget ~31s.
```

Events that change lifecycle phase:
- `startup_complete()` → `Ready`
- `request_shutdown(cause, active)` → `Draining{cause}` if active>0, else `Stopping{cause}`
- `sessions_drained()` → from `Draining` to `Stopping`
- `mark_restart_pending(active, cause)` → swaps flag AND calls `request_shutdown(cause, active)`. Persists "draining" or "stopping" to `daemon.state`.

Events that change `active_sessions`:
- Successful init (after `admit_initialize` returns Ok): `SessionTracker::add_session()` via `HttpJulieService::register_session()` at `src/daemon/mcp_session.rs:336`.
- Session drop (HTTP DELETE, transport close, panic): `SessionTracker::remove_session()` via `remove_session_registration_for()` at `src/daemon/mcp_session.rs:363-372`, invoked from `Drop for HttpJulieService` at `src/daemon/mcp_session.rs:566-609`.

---

## Session lifecycle accounting verdict

- **Increment**: `src/daemon/mcp_session.rs:336` (`register_session` → `sessions.add_session()`). Called only **after** `admit_initialize(context)?` returns `Ok` at `src/daemon/mcp_session.rs:523`, then inside `handler_for_request` at `src/daemon/mcp_session.rs:525`.
- **Decrement**: `src/daemon/mcp_session.rs:364` (`sessions.remove_session(&id)`) via `remove_session_registration_for()`, called from:
  - `HttpJulieService::Drop` at `src/daemon/mcp_session.rs:566-609` (spawned onto the tokio runtime so cleanup is async-completed; the same task also calls `session.finish()`).
  - Two earlier sites: `Self::remove_session_registration(&registration)` is called at `src/daemon/mcp_session.rs:535` when `DaemonMcpSession::start` itself fails — but that error path runs only after `register_session()` has already incremented, so the decrement is matched.

**Does a rejected init leak a counted session?** **No.** `apply_admission_action` returning `Err` at lines 453/467 propagates through `?` at line 492 / 504 of `admit_initialize`, which propagates through `?` at line 523 of `handler_for_request`, all *before* `self.register_session()` is reached at line 525. The rejected init never increments the counter and therefore never needs to decrement. Verified by `test_http_julie_session_version_mismatch_does_not_emit_dashboard_session_change` (src/tests/daemon/http_transport.rs:617), which explicitly asserts a rejected init produces zero session-tracker churn.

**Conclusion:** H1 is **wrong**. The active_sessions counter is honestly maintained. The bug is downstream.

---

## Adapter restart logic

File: `src/adapter/http_stdio.rs` + `src/adapter/launcher.rs`.

- `run_http_adapter` at `src/adapter/http_stdio.rs:105-266` is the retry loop. Constants: `MAX_RETRIES = 5` (line 94), backoff `1, 2, 4, 8, 16 s` (line 100). Total budget ~31 s.
- `restart_required_error` JSON-RPC responses are detected by `is_restart_required_response_for_in_flight` at `src/adapter/http_stdio.rs:491-505` (matches code -32603 + message containing "restart") and converted to `ForwardOutcome::ImmediateDaemonDisconnect` at line 404.
- That outcome runs `restart_handoff_action(attempt, MAX_RETRIES, RestartReason::ImmediateDisconnect)` → `Retry` for the first 5 attempts, `Exhausted` on the 6th — which `bail!`s and exits the adapter process.
- Each retry goes through `launcher.ensure_daemon_ready()` at `src/adapter/launcher.rs:168-194`. The launcher **never kills the daemon**. It only spawns a fresh one if `daemon_readiness()` returns `Dead`:
  - `Ready` → return Ok (try init again, will fail again).
  - `Starting` → poll until ready.
  - `Stopping` → `wait_for_pid_exit` polls until PID is gone, then re-loops; if the daemon never actually exits, this hangs until the outer 60 s deadline trips at `src/adapter/launcher.rs:301-307` with `TimedOut`.
  - `Dead` → spawn detached `julie-daemon start`.
- `daemon_readiness()` priority at `src/adapter/launcher.rs:103-150`: discovery.json `Live` with `phase=stopping|draining` → `Stopping`; otherwise probes transport readiness → `Ready`. If discovery.json is stale/missing, falls back to PID + daemon.state file.
- `discovery.json.phase` is **only** updated by `publish_discovery_phase(&paths, "stopping")` inside `DaemonHandle::shutdown` (`src/daemon/app/handle.rs:81`). The `mark_restart_pending` / `notify_restart` paths do not touch discovery.json. So during the bug window the daemon is alive, transport answers ready, discovery.json still says "ready" → adapter sees `Ready` and dives back into the failing init loop.

**Net: the adapter has no mechanism to escalate from "the daemon keeps rejecting me" to "kill the daemon and respawn."** It just burns its 31 s retry budget and exits.

---

## Drain timeout coverage

The 60 s drain timeout (`src/daemon/mod.rs:84` `drain_timeout()`, env-overridable to [1,120] s) is **only** consumed in two places:
1. `DaemonHandle::shutdown()` at `src/daemon/app/handle.rs:88` — after `request_shutdown(Signal, …)` is explicitly invoked. That function is called by `run_daemon` *only* after `signal = shutdown_signal()` or `stop_notify.notified()` fires (`src/daemon/mod.rs:343-355`). Neither fires from a restart-pending event.
2. `stop_daemon()` at `src/daemon/lifecycle.rs:408` — used by the external `julie stop` CLI.

The disconnect path `apply_disconnect_action_for` (`src/daemon/mcp_session.rs:378-413`), the admission path `apply_admission_action` (lines 415-470), and `mark_restart_pending` (`src/daemon/lifecycle.rs:203-214`) **never invoke the drain or the shutdown sequence**. They only flip an atomic, publish a phase string to `daemon.state`, and (in two places) call `notify_restart()`.

The 60 s drain therefore never fires for `RejectForRestart` or `ShutdownForRestart`. It also doesn't fire on the disconnect-time `TriggerShutdown` arm.

---

## Existing plan doc decisions

- **2026-05-06-daemon-windows-lifecycle.md**: shipped. PR3 already (a) atomicized `daemon.state` writes, (b) bumped drain to 10 s + configurable env var, (c) added the `admit_initialize` short-circuit (one log line per rejection). None of these target the missing `notify_restart` listener; they target observability and stop-command behavior. The verification ledger shows tests for short-circuit + drain timeout but no test for "daemon eventually exits after RejectForRestart cycle."
- **2026-05-15-daemon-reliability-design.md / plan.md**: shipped. (1) added LRU eviction to WorkspacePool, (2) deferred embedding readiness, (3) bumped default drain from 10 s → 60 s and aligned `stop_daemon()` to the same value, (4) added adapter retry with `restart_handoff_action` exponential backoff (the same loop now causing 31 s per failed reconnect). Explicit out-of-scope: "Mid-session retry (after output written)". The plan assumes the daemon will actually restart inside the drain window — the missing-listener bug invalidates that assumption.
- **2026-05-15-daemon-split-and-search-reranker-design.md** lines 53, 70, 425 / **2026-05-16-…-plan.md** lines 39, 281: the architects already flagged this area as structurally broken. The plan is to **delete** `DaemonLifecycleController`, `LifecyclePhase`, `restart_pending`, and binary mtime tracking in the daemon split. Cited reasoning: "Stale-binary mtime tracking, `restart_pending` state, drain-with-timeout-or-lose-writes, three shutdown paths racing in a `tokio::select!`. Each is reasonable in isolation; together they're a brittle web." The new design replaces this with kernel-held singleton + atomic discovery + thin adapter that respawns daemons.

**What's deferred:** the entire stale-binary-restart subsystem is queued for deletion. No interim patch exists in plan docs for the missing-listener bug specifically; the assumption seems to be that the split rewrite handles it. But v7.10.0 ships before that rewrite, and TODO.md line 94 acknowledges only "skip stale-binary restart while sessions are busy" — not the missing-listener leak.

---

## Coverage gaps in tests

Read-only audit of `src/tests/daemon/*` (no test execution):
- `lifecycle.rs` — exhaustive unit tests for the pure-function decision helpers (`stale_binary_accept_action`, `stale_binary_disconnect_action`, `version_gate_action`, `restart_handoff_action`, `transition`). Also tests `DaemonLifecycleController::mark_restart_pending` idempotency, transitions, and `sessions_drained`. **But nothing exercises `notify_restart()` reaching an `.await`.** Because no such await exists.
- `admit_initialize_short_circuit.rs` — verifies `apply_admission_action` is called exactly once per init attempt under rejection. Asserts `lifecycle.restart_pending()` flips on rejection (lines 184, 237). **No assertion that the daemon process exits or that subsequent inits transition out of restart_pending.**
- `http_transport.rs` — has `test_http_julie_session_delete_triggers_restart_when_binary_became_stale` (line 761) and `test_http_julie_session_rejects_new_sessions_after_restart_pending_with_active` (line 900). The former asserts a DELETE on a stale-binary session sets `restart_pending=true` (it checks the flag, **not** that the process exits). The latter explicitly asserts the rejection cycle stays sticky.
- No test simulates "client A connects → binary rebuilt → client A disconnects → client B connects → daemon should be restarted by now." That's the bug.

---

## Root cause (verified, not guessed)

`restart_notify: Arc<Notify>` is constructed at `src/daemon/lifecycle.rs:161`, exposed via `restart_notify() -> Arc<Notify>` at `src/daemon/lifecycle.rs:176`, and woken via `notify_restart()` at `src/daemon/lifecycle.rs:216` from two call sites:
- `src/daemon/mcp_session.rs:411` (disconnect path, when `remaining == 0 && restart_pending`)
- `src/daemon/mcp_session.rs:452` (admission `ShutdownForRestart` arm)

Grep `fast_search(restart_notify)` returns **only** these definitions and the call sites — there is **no** `.notified()` consumer anywhere in `src/`. The `Arc<Notify>` is produced and signalled, but never awaited. Verified with three independent searches (`restart_notify`, `restart_notify()`, `restart_notify().notified`). Result: the Notify is dead infrastructure.

The `request_shutdown(cause, active_sessions)` call inside `mark_restart_pending` mutates the in-memory `LifecyclePhase` and writes a string to `daemon.state`, but no task reads that phase to drive process exit. The only exit triggers in `run_daemon` are SIGTERM/SIGINT and the Windows named-event waker.

The bug behaviour in the field:
1. User has Claude Code (session A) running. Binary mtime captured at daemon startup.
2. User rebuilds `julie-server` (release). Binary mtime advances.
3. Session A continues running. No effect yet.
4. Session A ends *or* a second client connects:
   - Second client: `admit_initialize` runs `stale_binary_accept_action(stale=true, active=1, restart_pending=false)` → `AcceptWithRestartPending`. Flag flips on, session B is admitted. `daemon.state` is rewritten to `draining`.
   - Session A ends: `apply_disconnect_action_for` runs, sees `remaining==0 && stale → TriggerShutdown`, calls `mark_restart_pending(0, RestartRequired)` (phase becomes `Stopping`), then `notify_restart()` — silent, nothing listens.
5. Any subsequent init: `stale_binary_accept_action(stale=true, active=?, restart_pending=true)`. If active>0 → `RejectForRestart` (no notify_restart). If active==0 → `ShutdownForRestart` → `notify_restart` (still no listener). Either way the init returns `restart_required_error` to the adapter.
6. Adapter does 5 retries × ~31 s, exits. MCP client respawns the adapter. Loop.
7. Daemon process never exits unless the user SIGTERMs it. `discovery.json.phase` is still "ready" or unset (the disconnect/admit paths don't touch discovery), so the adapter sees `Ready` and connects, then gets rejected again. 20+ minutes is "until the user notices."

---

## Smallest fix (design only — DO NOT IMPLEMENT)

Two options; both small.

**Option A — minimum viable: wire `restart_notify` to exit.**

In `DaemonApp::serve` (after `startup_complete()` at `src/daemon/app.rs:315`, before the handle is returned), spawn one task that does:

```rust
let restart_notify = self.lifecycle.restart_notify();
let stop_notify_for_restart = Arc::clone(&stop_notify);
tokio::spawn(async move {
    restart_notify.notified().await;
    stop_notify_for_restart.notify_one();
});
```

This makes the existing `stop_notify` path (already wired into `run_daemon` at `src/daemon/mod.rs:349`) the single source of truth for "exit now." `notify_restart()` then funnels into the same shutdown sequence as SIGTERM / `julie stop`, which already calls `DaemonHandle::shutdown` → `drain_with_markers` (60 s budget) → full LIFO teardown. Discovery.json gets rewritten to `phase=stopping` by that code path. Adapter sees `Stopping`, waits for PID exit, sees Dead, respawns the new binary. Self-healing within ~one drain timeout.

Trade-off: when active sessions exist at the moment of the restart trigger (the `RejectForRestart` arm fires `mark_restart_pending` without calling `notify_restart` — and that is the current contract for "let in-flight finish first"), this fix still leaves restart deferred until the last legit session disconnects. The fallback `if remaining == 0 && restart_pending` block in `apply_disconnect_action_for` (line 409) already calls `notify_restart` then, so once sessions drain naturally the chain completes. The only loop today where this stalls is "session A disconnects but `notify_restart` is a no-op" — fixed by this wiring.

**Option B — belt & suspenders: also push the phase into discovery.**

`mark_restart_pending(active>0, RestartRequired)` should also rewrite `discovery.json` to `phase=draining` so the adapter's faster path at `src/adapter/launcher.rs:104-115` sees `Stopping` and short-circuits the init attempt instead of running the full retry budget. Add a call to `publish_discovery_phase(&paths, "draining")` inside the `mark_restart_pending` lifecycle controller (requires threading paths into the controller) or, more cheaply, into `apply_admission_action` / `apply_disconnect_action_for` directly. This avoids the "5 retries × 31 s" wasted budget on every new MCP client connect during the restart-pending window.

**Recommendation**: ship A as the actual bug fix. Add B in the same PR because it removes the worst symptom (each disconnect still burns ~31 s of the user's life). Also add a test that simulates the exact scenario (`session connects → binary mtime advances → session disconnects → assert daemon exits within drain timeout`). The existing test fixture `AdmissionFixture` from `admit_initialize_short_circuit.rs` is reusable.

**Out of scope of "smallest fix" but worth flagging**: the `restart_pending` flag has no reset path. Once set, it stays true until the process exits. That is fine if the process is going to exit imminently; with option A it does. Without A, the flag is a permanent stuck bit. Worth a code comment ("`restart_pending` is one-way; the only legitimate clear is process exit").

---

## Confidence

- Verdict on H1 (counter leak): 95%. The counter is honestly maintained; rejected inits never increment.
- Verdict on root cause (missing listener): 90%. Verified by three fast_search passes for any `.notified()` await of `restart_notify`; none exists. The remaining 10% uncertainty is whether some background task reads `restart_pending.load()` and drives exit by another path — I checked `restart_pending_handle` callers (`src/dashboard/state.rs` for status reporting, `src/health/checker.rs` for /health, `src/daemon/app.rs` for forwarding into the handler) and none drive exit.
- Fix option A reasoning: 80%. The wiring is one tokio task and reuses an existing exit channel. The most likely failure mode is in tests that synthesise a `DaemonLifecycleController` standalone (not via `DaemonApp::serve`); they would no longer be coupled to the new spawn. Those tests are unit tests of the pure decision functions and unaffected.
