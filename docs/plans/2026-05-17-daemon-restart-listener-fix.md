# Daemon Restart Listener Fix Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use `razorback:subagent-driven-development` when subagent delegation is available. Fall back to `razorback:executing-plans` otherwise.

**Goal:** Stop the daemon from sitting in `restart_pending` forever after a binary rebuild. Recovery must be bounded by the existing 60s drain timeout, regardless of whether active sessions disconnect on their own.

**Architecture:** Two small changes plus tests. (1) Make `mark_restart_pending` always call `notify_restart()` so every transition signals shutdown. (2) Spawn a listener task in `DaemonApp::serve` that bridges `restart_notify.notified().await` into the existing `stop_notify` shutdown path. The bridge funnels restart triggers into the same drain-and-teardown pipeline that SIGTERM already exercises.

**Tech Stack:** Rust, tokio (`Notify`, `select!`, `spawn`), rmcp HTTP session admission.

**Blocking version:** v7.10.0 ships only after this fix is merged and live-validated.

**Design context:** `docs/plans/2026-05-17-daemon-restart-listener-design.md` (this plan is the implementation companion). Codex adversarial review verdict `needs-attention` (1 high finding on test scope, addressed below). Field evidence in `.memories/2026-05-17/daemon-fragility-log-archaeology.md` and `.memories/2026-05-17/daemon-fragility-code-cartography.md`.

---

## Bug Summary

**Symptom (production-confirmed across 8 days of logs):** when the Julie release binary is rebuilt while sessions are active, the daemon detects stale mtime, marks `restart_pending=true`, then begins rejecting every new MCP session init with `restart_required_error`. The daemon never exits on its own. Observed: 637 rejections on 2026-05-10, 33 on 2026-05-17, 22-minute outages. Recovery requires external SIGKILL.

**Root cause:** `src/daemon/lifecycle.rs:152` defines `restart_notify: Arc<Notify>` and `notify_restart()` at line 217 calls `notify_one()` on it. There is **no `.notified()` consumer of `restart_notify` anywhere in src/**. Verified independently by the code cartographer and by adversarial grep. The notification channel is dead infrastructure — every `notify_restart()` call fires into a void. The daemon's only real exit triggers are SIGTERM/SIGINT and the Windows named-event waker (`stop_notify`).

Additionally, only the disconnect-time path (`mcp_session.rs:411`) and `ShutdownForRestart` (line 452) call `notify_restart()` at all. `AcceptWithRestartPending` (line 428) and `RejectForRestart` (line 455) mark the flag but never signal. Even with a listener wired, the dominant failure mode (active sessions that never disconnect on their own) would still hang. Both arms must signal.

Codex adversarial review framed the design choice: **bounded forced restart vs preserving existing sessions**. This plan picks bounded forced restart. The 60s drain timeout still preserves most user work; truly long tool calls (>60s) get cut off, which is acceptable compared to indefinite silent hang.

---

## Files Affected

| Path | Operation | Responsibility |
|---|---|---|
| `src/daemon/lifecycle.rs` | Modify (`mark_restart_pending` at lines 203-214) | Call `self.notify_restart()` after `swap(true, …)` ONLY when `first_request==true` so the first transition signals shutdown and subsequent calls are no-ops on the flag and on the channel. Document the new invariant: "flipping restart_pending commits to shutdown via the listener in DaemonApp::serve." |
| `src/daemon/mcp_session.rs` | Modify (lines 411 and 452) | Delete the now-redundant explicit `notify_restart()` calls. The earlier `mark_restart_pending` call at each site now handles the notify internally. Code stays correct because `Notify::notify_one` coalesces, but the explicit calls become misleading double-fires. |
| `src/daemon/app.rs` | Modify (after `startup_complete()` near line 315) | Spawn a tokio task: `restart_notify.notified().await; stop_notify.notify_one();`. The listener runs once — `notify_one` semantics: if notify_restart fires before the listener arms, the next `.notified()` returns immediately, so startup races are handled. |
| `src/tests/daemon/restart_listener.rs` | Create | RED tests for Task 2 and Task 3 (see below). Task 2 tests use the full `DaemonApp::serve` path, not a bare `DaemonLifecycleController`, since the listener task only exists there. |
| `src/tests/daemon/lifecycle.rs` | Modify | Add unit tests for Task 1: `mark_restart_pending` wakes a registered `.notified()` listener within ε; second call does NOT re-notify (proves the `first_request` gate). |
| `src/tests/daemon/mod.rs` | Modify | Register new `restart_listener` module. |

No changes to `src/adapter/*`. The existing 5-retry × exponential-backoff (~31s) loop is correct as-is once the daemon actually exits.

No changes to `discovery.json` publish path. Codex finding 2 confirms: publishing `draining` from `mark_restart_pending` before shutdown is actually armed creates a new stuck mode (adapters wait for death that may not come). The existing `DaemonHandle::shutdown` path already publishes `stopping` to discovery.json once shutdown is armed; that's the right place.

---

## Task 1: Lifecycle Invariant — first mark_restart_pending notifies the restart channel

**Files:**
- Modify: `src/daemon/lifecycle.rs:203-214` (`DaemonLifecycleController::mark_restart_pending`)
- Modify: `src/daemon/mcp_session.rs` (delete redundant `notify_restart()` calls at lines 411 and 452)
- Modify: `src/tests/daemon/lifecycle.rs` (add unit tests)

**What to build:** Make `mark_restart_pending` call `self.notify_restart()` on the first transition (gated by the existing `first_request` flag). The first transition commits to shutdown; subsequent calls are no-ops on both flag and channel. Then delete the two redundant explicit `notify_restart()` calls in `mcp_session.rs` whose preceding `mark_restart_pending` now handles the notify.

**Approach:**

```rust
pub fn mark_restart_pending(
    &self,
    active_sessions: usize,
    cause: ShutdownCause,
) -> RestartPendingTransition {
    let first_request = !self.restart_pending.swap(true, Ordering::Relaxed);
    let next_phase = self.request_shutdown(cause, active_sessions);
    if first_request {
        // First transition commits to shutdown. The listener wired in
        // DaemonApp::serve bridges this signal into the SIGTERM exit path,
        // which runs the 60s drain and full LIFO teardown. Gating on
        // first_request matches the existing flag semantics and avoids
        // spurious permits if the listener task is restarted by a future
        // refactor. Notify::notify_one would coalesce anyway.
        self.notify_restart();
    }
    RestartPendingTransition {
        first_request,
        next_phase,
    }
}
```

Also update the doc comment on `restart_pending` (line 151) to: "One-way bit; the only legitimate clear is process exit. The first call to `mark_restart_pending` signals the restart channel, which the listener in `DaemonApp::serve` bridges into the SIGTERM exit path."

In `src/daemon/mcp_session.rs`:
- Line 411 (`apply_disconnect_action_for`, `remaining == 0 && restart_pending` arm): the `mark_restart_pending(remaining, ShutdownCause::RestartRequired)` call earlier in the function now handles the notify. Delete the explicit `admission.lifecycle.notify_restart()` line.
- Line 452 (`apply_admission_action`, `ShutdownForRestart` arm): the `mark_restart_pending(active_sessions, ShutdownCause::RestartRequired)` call at line 444 now handles the notify. Delete the explicit `admission.lifecycle.notify_restart()` line.

**Acceptance criteria:**
- [ ] `mark_restart_pending` calls `self.notify_restart()` inside `if first_request { … }`
- [ ] Doc comment on `restart_pending` field updated to call out the invariant
- [ ] mcp_session.rs:411 explicit `notify_restart()` deleted
- [ ] mcp_session.rs:452 explicit `notify_restart()` deleted
- [ ] RED test `test_mark_restart_pending_signals_listener_on_first_transition`: register a `.notified()` waiter on `restart_notify()`; call `mark_restart_pending`; assert waiter wakes within 100ms
- [ ] RED test `test_mark_restart_pending_does_not_double_signal`: call `mark_restart_pending` once (first call consumed by a waiter); register a fresh waiter; call `mark_restart_pending` again; assert the fresh waiter does NOT wake within a short window (e.g., 50ms); proves the `first_request` gate
- [ ] Existing `lifecycle.rs` tests pass unchanged (verify `test_controller_restart_pending_is_idempotent_with_active_sessions`, etc.)

**Worker invariant:** First `mark_restart_pending` call wakes the restart channel; subsequent calls do not.

**Worker red/green scope:** `cargo nextest run --lib test_mark_restart_pending_signals_listener_on_first_transition 2>&1 | tail -10` and `cargo nextest run --lib test_mark_restart_pending_does_not_double_signal 2>&1 | tail -10`

---

## Task 2: Listener Bridge in DaemonApp::serve

**Files:**
- Modify: `src/daemon/app.rs` (insert tokio::spawn after `startup_complete()`)
- Create: `src/tests/daemon/restart_listener.rs`
- Modify: `src/tests/daemon/mod.rs` (register module)

**What to build:** A one-shot listener task that bridges `restart_notify.notified()` into `stop_notify.notify_one()`. The bridge funnels every restart signal into the existing SIGTERM exit path that calls `DaemonHandle::shutdown` → `drain_with_markers` (60s) → full LIFO teardown → `publish_discovery_phase("stopping")`.

**Approach:**

In `DaemonApp::serve` after the call to `self.lifecycle.startup_complete()` (around line 315 — verify exact line at implementation time):

```rust
let restart_notify = self.lifecycle.restart_notify();
let stop_notify_for_restart = Arc::clone(&stop_notify);
tokio::spawn(async move {
    restart_notify.notified().await;
    tracing::info!(
        "Restart channel signaled; triggering daemon shutdown via stop_notify"
    );
    stop_notify_for_restart.notify_one();
});
```

Notes:
- `Notify::notify_one` semantics: if a permit exists at await time (i.e., notify_restart fired before the listener was spawned), `.notified()` returns immediately. Startup-race safe.
- One-shot is sufficient. `stop_notify` triggers the daemon's only exit path; once fired, the daemon goes through shutdown and `restart_pending` is moot.
- No panic handling needed. If the spawned task panics, the daemon stays up (worst-case fallback to the pre-fix behavior). But the task has no failure modes — both `.notified()` and `notify_one()` are infallible.
- The `tracing::info!` log is REQUIRED for live validation — operators need to distinguish "restart via the new fix" from "restart via SIGTERM" in daemon.log when verifying recovery.

**Acceptance criteria:**
- [ ] Listener task spawned in `DaemonApp::serve` after startup
- [ ] `tracing::info!` log fires before `stop_notify.notify_one()` (required by live-validation step 6)
- [ ] RED test `restart_listener_bridge_routes_notify` (bare-Notify isolation, ~10 lines): build two bare `Arc<Notify>`, spawn the bridge task, fire the "restart" Notify, assert the "stop" Notify wakes within 100ms. Tests the bridge in isolation without DaemonApp's TcpListener/dashboard/WatcherPool overhead. The full-DaemonApp wiring is exercised by Task 3.
- [ ] RED test `restart_listener_handles_pre_spawn_notify` (bare-Notify isolation): fire the "restart" Notify BEFORE the bridge task is spawned; spawn the bridge; assert the "stop" Notify wakes (proves `Notify::notify_one`'s "permit before waiter" semantics so startup races are safe)

**Worker invariant:** The bridge task awakens `stop_notify` once `restart_notify` is signaled, including when the signal arrived before the bridge was spawned.

**Worker red/green scope:** `cargo nextest run --lib restart_listener_bridge_routes_notify 2>&1 | tail -10`

---

## Task 3: Integration Test — Active-Session Bounded Recovery

**Files:**
- Modify: `src/tests/daemon/restart_listener.rs` (add active-session integration test)

**What to build:** The critical regression test Codex called out as the missing test scope. Asserts that recovery happens within drain timeout EVEN IF an active session never disconnects.

**Approach (option a — reuse the existing HTTP-based pattern):**

Copy the session-keep-alive pattern from `test_http_julie_session_rejects_new_sessions_after_restart_pending_with_active` in `src/tests/daemon/http_transport.rs:900`. That existing test already keeps one HTTP session alive across a second `post_initialize` call and exercises the same admission gate; we extend it to assert daemon exit rather than just admission rejection. No `AdmissionFixture` extension needed.

The test sequence:

1. Build a full `DaemonApp` via the existing HTTP-test harness, with admission wired and a controlled `current_binary_mtime` closure (the binary check function is injected per `mcp_session.rs` admission setup — confirm exact field/type at implementation by reading the call site in `DaemonApp::serve`)
2. Configure short drain timeout via `JULIE_DAEMON_DRAIN_TIMEOUT_SECS=2` so the test runs in ~3s wall-clock. **MUST mark the test `#[serial_test::serial]`** — `serial_test 3.2` is already a workspace dependency and the existing `src/tests/daemon/drain_timeout.rs` already uses this pattern for the exact same env var. Match it.
3. Spawn `DaemonApp::serve` in a tokio task; capture its `JoinHandle`. Wait until startup completes (poll for daemon-state="ready" via the existing harness helper, or use the bind-address handshake the HTTP-test harness already exposes).
4. Open an active session: send a real HTTP `initialize` request. Daemon's `stale_binary_accept_action(false, 0, false)` → `Accept`. Session registered, `active_sessions=1`. KEEP THE SESSION OPEN — do not send the corresponding disconnect.
5. Advance the fake binary mtime past the captured startup mtime (mutate the closure-backing `Arc<AtomicI64>` or equivalent).
6. Send a second HTTP `initialize` from a different transport. Daemon's `stale_binary_accept_action(true, 1, false)` → `AcceptWithRestartPending`. This is the admission arm that fires `mark_restart_pending` while the first session is still alive — with Task 1's fix, it also fires `notify_restart`, which the listener bridges to `stop_notify`.
7. Assert `DaemonApp::serve`'s JoinHandle completes within `drain_timeout + 1s` (i.e., ~3s), proving the daemon actually exited despite session 1 still being held open.

**Note on the admission path tested:** Cartographer correctly pointed out the second `initialize` hits `AcceptWithRestartPending`, NOT `RejectForRestart`. To hit `RejectForRestart` the test would need a THIRD `initialize` after the second flips the flag. We don't need to hit the Reject path here — the invariant under test is "any `mark_restart_pending` in the presence of an active session triggers daemon exit within drain timeout", and the AcceptWithRestartPending path is the FIRST admission arm to fire and the one most users hit. The Reject path is incidentally covered because it also calls `mark_restart_pending`, and the bridge fires once regardless of which arm flipped the flag.

**Acceptance criteria:**
- [ ] Test `daemon_exits_within_drain_when_active_session_never_disconnects` passes
- [ ] Test marked `#[serial]` (env-var concurrency safety; matches `src/tests/daemon/drain_timeout.rs` pattern)
- [ ] Test sets `JULIE_DAEMON_DRAIN_TIMEOUT_SECS=2` and asserts `DaemonApp::serve` JoinHandle completes within 3s
- [ ] Test holds session 1 open across the second initialize (does NOT close/disconnect it) — this is what makes the test fail-RED before Task 1+2 land
- [ ] Failing-RED-first: confirmed by running the test against `main` (pre-fix) — it must hang/timeout
- [ ] Test does NOT depend on actual binary-rebuild; uses injected mtime closure

**Worker invariant:** A stale-binary `AcceptWithRestartPending` event with an active session that never disconnects triggers daemon exit within drain timeout.

**Worker red/green scope:** `cargo nextest run --lib daemon_exits_within_drain_when_active_session_never_disconnects 2>&1 | tail -10`

---

## Task Dependencies

```
Task 1 (lifecycle invariant)    ┐
Task 2 (listener bridge)        ┘── independent files, run in parallel
Task 3 (integration test)       ── depends on Task 1 + Task 2 both merged (test asserts end-to-end behavior)
```

Tasks 1 and 2 touch disjoint files (lifecycle.rs vs app.rs). Both are <50 lines of code. Task 3 is the integration test that proves both work together; it would fail-RED against either change in isolation.

**Recommended worker assignment:** 2 implementer subagents in parallel for Tasks 1 and 2, 1 implementer for Task 3 once both land. Lead does inline review on all three.

---

## Out of Scope

- **discovery.json `draining` publish from `mark_restart_pending`** — explicitly rejected per Codex finding 2. Publishing `draining` before shutdown is armed creates a new stuck mode. The existing `DaemonHandle::shutdown` already publishes `stopping`; that's the right place.
- **Adapter changes** — the existing 5-retry × exponential-backoff loop in `src/adapter/http_stdio.rs` is correct. Once the daemon actually exits via this fix, the adapter's existing `daemon_readiness() → Dead` detection will spawn the new binary.
- **Removing `restart_notify` plumbing** — the daemon-split rewrite (docs/plans/2026-05-15-daemon-split-and-search-reranker-design.md lines 53, 70, 425) plans to delete `DaemonLifecycleController` entirely. This fix is a minimal interim that preserves backward compatibility until the rewrite lands.
- **The `restart_pending → cleared` transition** — the field stays one-way (no clear). After this fix, daemon exits on the first transition anyway, so a clear is unnecessary.

---

## Verification Strategy

**Worker red/green scope:** Per task above. Workers run exactly one named test, capture output, report.

**Lead affected-change scope:** After Tasks 1 and 2 land together, lead runs:

```bash
cargo xtask test changed
```

If `changed` falls back to `dev` (likely — daemon lifecycle is shared infrastructure), accept it.

**Branch gate:** Before merging:

```bash
cargo xtask test reliability
```

The `reliability` tier (`daemon + workspace_init + integration` buckets per CLAUDE.md) is the natural home for this work.

**Live validation:** Required before declaring done. The plan does not merge to main without this.

Procedure:
1. Build new release binary: `cargo build --release`
2. Run `cargo xtask dev-link` to ensure plugin cache points at dev binary
3. Open Claude Code (or any MCP client) against julie-server, send a tool call to confirm baseline
4. From a second terminal, `cargo build --release` again to advance binary mtime
5. From the MCP client, send another tool call
6. Tail `~/.julie/daemon.log.$(date +%Y-%m-%d)`:
   - Expect ONE `Last HTTP session disconnected and restart is pending. Triggering restart.` event OR a `Triggering restart` event from the new listener path
   - Within ~60s of step 4, expect `Binary mtime captured for stale-binary detection` from a new daemon process
   - Subsequent tool calls should succeed within ~65s of step 4 (drain + adapter respawn)
7. Capture relevant log lines and attach to verification ledger as evidence

**Codex review:** Already complete (verdict `needs-attention` on the original design; this plan addresses all 3 findings). Re-review the plan doc itself before implementation? Optional — only if the design has changed materially since the codex review. As of this writing, the design Codex reviewed and this plan agree.

---

## Verification Ledger

Per `docs/plans/verification-ledger-template.md`. Reuse rule: same scope label AND same HEAD SHA AND prior result `pass`.

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| T1 worker: `mark_restart_pending` signals on first transition; subsequent calls do not | `cargo nextest run --lib test_mark_restart_pending_signals_listener_on_first_transition test_mark_restart_pending_does_not_double_signal` | worker-red-green | 6a0ead78 | pass (2 tests, <0.1s) | 2026-05-17T22:30:15Z | no |
| T2 worker: bridge task funnels `restart_notify` into `stop_notify`, including the pre-spawn-permit case | `cargo nextest run --lib restart_listener_bridge_routes_notify restart_listener_handles_pre_spawn_notify` | worker-red-green | 219ce80e | pass (2 tests, 0.023s) | 2026-05-17T22:29:13Z | no |
| T3 worker: active session that never disconnects still triggers daemon exit within drain timeout (bounded recovery) | `cargo nextest run --lib daemon_exits_within_drain_when_active_session_never_disconnects` | worker-red-green | 781884b7 | pass (8.931s); RED proven on `219ce80e^` with seam+test cherry-picked: FAIL (8.749s timeout) | 2026-05-17T22:48:00Z | no |
| Lead branch gate: daemon + workspace_init + integration buckets all pass | `cargo xtask test reliability` | lead-branch-gate | 781884b7 | pass (127.7s; daemon 61.5s, workspace-init 2.8s, integration 63.4s) | 2026-05-17T22:54:00Z | no |

---

## Risks and Honest Caveats

1. **The `mark_restart_pending` first-call-notify change has subtle implications**: every existing call site (admission accept-with-pending, reject, disconnect) now drives drain on the first transition. The first post-rebuild session has up to 60s to finish. Long embedding refresh / large file index could be cut off. This is intentional; the alternative (indefinite hang) is worse. If users hit this, the response is "the user reconnects against the new daemon, retries the tool call" — not "we make restart unbounded again".

2. **The listener task lives for the daemon's lifetime but only fires once.** After it fires `stop_notify.notify_one()` and returns, subsequent `notify_restart()` calls hit nothing. This is fine because the daemon is shutting down immediately after the first signal; restart_pending is moot. If a future refactor makes the daemon resume after notify_restart without exiting, the listener would need to loop. The first_request gate in `mark_restart_pending` makes this asymmetry explicit (only first call wakes). Comment in code calls this out.

3. **The 60s drain bounds the recovery window but not the user's tool-call wait.** During drain, the adapter's 5-retry × exponential-backoff loop (~31s) plus drain (up to 60s) plus new daemon spawn (~2-5s) means worst-case user-visible latency is ~95-100s. The previous unbounded hang was infinitely worse, but this is not zero-cost.

4. **There is one remaining "stays alive on old binary indefinitely" case that is not a bug:** if the user has exactly one client with exactly one open session and never reconnects nor opens a second tool, the daemon never detects the new mtime (detection only runs at admission gates). The daemon keeps serving the existing session with the old in-memory binary. This is fine — no rejection storm, no broken UX. The first new admission event triggers the bounded recovery path normally.

5. **Codex's adversarial review and the cartographer's independent audit reached the same root cause** but Codex pushed the bounded-vs-preserve choice harder. The chosen design (bounded) is the conservative call. If field evidence later shows the 60s drain truncates important work consistently, we revisit — but the user's "no deferring work" rule says fix it now and revisit only on real evidence.

---

## Glossary

- `restart_pending` — atomic bool in `DaemonLifecycleController` indicating the daemon has detected a need to restart and is waiting to drain.
- `mark_restart_pending` — flips the atomic and transitions lifecycle phase. With this fix, also signals the restart channel.
- `restart_notify` — `Arc<Notify>` channel; before this fix, had no listener. This fix wires one.
- `stop_notify` — `Arc<Notify>` channel that triggers daemon process exit. Wired by SIGTERM, SIGINT, and the Windows named-event handler. After this fix, also wired by the new listener.
- `drain_timeout` — the 60s configurable timeout for `DaemonHandle::shutdown` to wait for active sessions to complete (`src/daemon/mod.rs:84`, env: `JULIE_DAEMON_DRAIN_TIMEOUT_SECS`).
