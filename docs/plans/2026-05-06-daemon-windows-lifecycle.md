# Daemon Windows Lifecycle Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `razorback:subagent-driven-development` when subagent delegation is available. Fall back to `razorback:executing-plans` for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Address all 11 findings from `docs/findings/2026-05-06-daemon-windows-lifecycle-review.md`, hardening daemon shutdown ordering, PID file safety, atomic state writes, and Windows-specific resource semantics across the daemon and adapter.

**Architecture:** Four sequential PRs grouped by theme: (1) shutdown teardown ordering and per-workspace resource release, (2) PID-reuse defense plus retry hardening, (3) atomic state writes plus drain-timeout polish plus admission short-circuit, (4) docs-only updates for the Windows in-place rebuild caveat and cosmetic findings. Implementation lands and validates on macOS only; the Windows CI gap is captured as a cross-cutting follow-up item rather than a blocker.

**Tech Stack:** Rust 1.x, Tokio (async runtime, `Notify`, `select!`), `fs2` (file locks), Tantivy (full-text index), `libc` and direct WinAPI extern declarations for cross-platform process introspection, `std::fs` for atomic file primitives.

**Spec source:** `docs/findings/2026-05-06-daemon-windows-lifecycle-review.md`

---

## Scope and Honest Caveats

Two findings (#10 fs2 lock semantics and lock-file persistence, #11 non-UTF-8 home path) are explicitly flagged as no-action / cosmetic by the reviewer. This plan captures both as **documented-and-closed** in PR 4: a doc note rather than fabricated code changes. The plan delivers all 11 findings, but #10 and #11 are documentation artifacts, not code edits. That is the honest read of the spec, not a scope reduction.

The reviewer's closing recommendation that "a Windows CI run that exercises the restart loop would be the only way to fully validate or refute these findings" is addressed in Section X (Cross-Cutting) as a follow-up issue. This plan does not block on Windows CI existing because doing so would block all of the high-severity fixes behind infrastructure work that is out of scope.

---

## Files Affected

| Path | Operation | Responsibility |
|---|---|---|
| `src/daemon/mod.rs` | Modify (~lines 663-680, plus shutdown body) | Reorder shutdown sequence (LIFO), call new pool shutdowns, await embedding service |
| `src/daemon/workspace_pool.rs` | Modify | Add `WorkspacePool::shutdown()` walking all entries, calling `SearchIndex::shutdown()` (mirrors `handler.rs:626-641`) |
| `src/daemon/watcher_pool.rs` | Modify | Add `WatcherPool::shutdown()` walking all entries, calling `detach()` |
| `src/daemon/embedding_service.rs` | Modify (lines 285-289 and surrounding) | Make `shutdown()` async, await child sidecar exit with bounded timeout |
| `src/daemon/pid.rs` | Modify (struct, parsers, retry loop) | Extend PID file format to `pid creation_time binary_mtime`, add backoff to `create_exclusive` retry loop, propagate non-NotFound `remove_file` errors |
| `src/daemon/lifecycle.rs` | Modify (lines 322-329) | Convert `write_daemon_state` to write-temp + atomic rename |
| `src/daemon/mcp_session.rs` | Modify (lines 452-498) | Short-circuit `admit_initialize` when first `apply_admission_action` returns `Err` |
| `src/embeddings/sidecar_provider.rs` (or wherever sidecar Command is built) | Modify | Confirm Command is built with stdin/stdout/stderr null and (Windows) `CREATE_NEW_PROCESS_GROUP`; add bounded `wait_with_timeout` for sidecar exit |
| `src/paths.rs` | Modify (doc comment only) | Add doc note about non-UTF-8 home path edge case in `julie_home_hash` |
| `src/tests/daemon/` (new test files) | Create | Tests for each task — see per-task acceptance criteria |
| `src/handler.rs:626-641` | Reference only | Existing pattern that PR 1 extends — do not modify |

**Existing precedent worth highlighting:** `src/handler.rs:626-641` already calls `SearchIndex::shutdown()` during per-session `teardown_loaded_workspace`. PR 1 extends this pattern to daemon-level teardown by walking the `WorkspacePool::workspaces` HashMap (already present at `src/daemon/workspace_pool.rs:19`).

---

## Verification Strategy

**Project source of truth:** `RAZORBACK.md` (model and gate ownership), `CLAUDE.md` (xtask tier definitions and subagent test rules), `docs/TESTING_GUIDE.md` (SOURCE/CONTROL methodology), `docs/plans/verification-ledger-template.md` (ledger format).

**Worker red/green scope:** Worker writes a failing test in `src/tests/daemon/<area>.rs` and runs ONLY that exact test by name:

```bash
cargo nextest run --lib <exact_test_name> 2>&1 | tail -10
```

**Worker ceiling:** A single named test. No xtask tiers. No broad module filters. Per `CLAUDE.md` "Subagent & Worker Agent Test Rules" and `RAZORBACK.md` "Workers run the assigned narrow scope."

**Worker gate invariant:** Each task lists the invariant the assigned test proves. Workers must restate that invariant in their report alongside command, scope label, commit SHA, result, and timestamp.

**Lead affected-change scope:** After each batch (a coherent group of tasks within a PR), lead runs:

```bash
cargo xtask test changed
```

Per `CLAUDE.md`: when `changed` falls back to `dev`, accept it — that signals shared-infrastructure movement, which is expected for daemon lifecycle edits.

**Branch gate:** Before each PR is opened or merged, lead runs:

```bash
cargo xtask test reliability
```

Reliability tier covers `daemon + workspace_init + integration` buckets, which is the natural home for this work per `CLAUDE.md`.

**Expensive tier (final pre-merge):** Before the final PR (PR 4) merges:

```bash
cargo xtask test full
```

**Replay/metric evidence:** None. This work is structural correctness, not performance or scoring.

**Escalation triggers (per RAZORBACK.md):**
- Two worker attempts on the same task fail review → escalate to strategy tier (Opus / gpt-5.5)
- Test passes but lead sees a plausible second-order race → escalate
- Concurrency or restart behavior not covered by the task's narrow test → escalate before merge
- Plan no longer matches codebase reality → revise plan

**Assigned verification failure:** Workers stop and report when assigned verification fails. They do not retry, do not commit, do not present the failure as evidence. The lead diagnoses or reassigns.

---

## Verification Ledger

Per `docs/plans/verification-ledger-template.md`. Reuse rule: same scope label AND same HEAD SHA AND prior result `pass`.

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| Worktree baseline: `daemon + workspace_init + integration` buckets all pass on the branch starting point, so any new failure during PR 1-4 work is a real regression | `cargo xtask test reliability` | branch-baseline | cd903f1375122061690806d0b5789b9db9d79fa7 | pass (61.6s; daemon 11.6s, workspace-init 0.8s, integration 49.2s) | 2026-05-06T23:06:42Z | no |
| PR 1 Task 1.1 worker invariant: After `WorkspacePool::shutdown()`, no workspace retains an active Tantivy `IndexWriter`; locks are released. Test asserts `is_shutdown()==true` on N=3 workspaces, plus poisoned-mutex recovery | `cargo nextest run --lib test_workspace_pool_shutdown_calls_search_index_shutdown` and `cargo nextest run --lib test_workspace_pool_shutdown_recovers_from_poisoned_mutex` | worker-red-green | 201a42ea397162ebef04369ae4f39b788b0b269e | pass (0.150s + 0.106s) | 2026-05-06T23:14:40Z | no |
| PR 1 Task 1.2 worker invariant: After `WatcherPool::shutdown()`, no `IncrementalIndexer` task remains live for any workspace; OS file-watcher handles are released. Test asserts ref_count==0 and no grace deadlines for N=3 workspaces | `cargo nextest run --lib test_watcher_pool_shutdown_drops_all_watchers` (+ empty-pool no-op + mixed-states variants) | worker-red-green | e0029079ecd78c14830e87cbc2f25f1bf8b5a5a5 | pass (0.012s) | 2026-05-06T23:14:18Z | no |
| PR 1 Task 1.3 worker invariant: On clean shutdown, the new daemon does not race the old sidecar's handle release: shutdown blocks until the sidecar child has exited or a 3-second bound elapsed. Tests assert `wait_for_exit` is called and elapsed bounds are honored | `cargo nextest run --lib test_embedding_service_shutdown_waits_for_child_exit` and `cargo nextest run --lib test_embedding_service_shutdown_returns_on_timeout` | worker-red-green | 3bdc24099fb9ff6612932cd92b5f0ad79b2cad0e | pass (0.068s + 0.013s) | 2026-05-06T23:17:21Z | no |
| PR 1 Task 1.4 worker invariant: Shutdown sequence is LIFO of the dependency graph; in-flight HTTP requests cannot observe a torn-down embedding service, and Tantivy locks are released before file watchers are dropped. Tests assert call-log positions of http_transport/workspace_pool/watcher_pool steps | `cargo nextest run --lib test_shutdown_calls_pools_after_transport` and `cargo nextest run --lib test_shutdown_calls_workspace_pool_before_watcher_pool` | worker-red-green | d451206e7f35eff2c9e543a28db23e5e78ea47fb | pass | 2026-05-06T23:27:30Z | no |
| PR 2 Tasks 2.1+2.2 worker invariant: A different process assigned a recycled PID cannot impersonate a daemon AND PID file creation does not silently swallow Windows sharing violations or burn ten retries in microseconds. Tests assert 3-field PID format, creation_time-based PID-reuse rejection, exponential backoff math, and remove_file error propagation | `cargo nextest run --lib test_pid_file_writes_three_fields` and `test_check_running_rejects_pid_reuse` and `test_check_running_accepts_matching_creation_time` and `test_create_exclusive_propagates_remove_file_errors` and `test_exponential_backoff_formula` | worker-red-green | 92523334a2fc4a598f70d875ebd392491a721b82 | pass (5 tests + 23 daemon pid tests overall) | 2026-05-06T23:51:35Z | no |
| PR 3 Task 3.1 worker invariant: Concurrent readers of `daemon.state` observe only complete state strings; partial reads are impossible after temp+rename. Test stresses 1000 writes against a tight reader loop and asserts allowed-content set | `cargo nextest run --lib test_write_daemon_state_no_partial_reads` | worker-red-green | 0568d77d46b0e6a930a17ede3e338dc985ec102f | pass (0.213s) | 2026-05-06T23:42:53Z | no |
| PR 3 Task 3.2 worker invariant: Operators can extend the drain window for slow filesystems via `JULIE_DAEMON_DRAIN_TIMEOUT_SECS` without recompiling; drain timeout surfaces as ERROR not WARN | `cargo nextest run --lib test_drain_timeout_reads_env_var` and `test_drain_timeout_default_when_unset` and `test_drain_timeout_clamps_out_of_range` | worker-red-green | 0568d77d46b0e6a930a17ede3e338dc985ec102f | pass (3 tests, 0.046s total) | 2026-05-06T23:42:53Z | no |
| PR 3 Task 3.3 worker invariant: A failed first admission action prevents the second from running; only one log line per admit attempt describes the rejecting gate. Tests assert apply_admission_action call count is 1 when stale-binary gate rejects and 2 when stale-binary passes | `cargo nextest run --lib test_admit_initialize` (matches both tests) | worker-red-green | 09260b75c38a53094731e8d401eb2e0c3a1ab4a5 | pass (2 tests) | 2026-05-06T23:49:01Z | no |
| Lead PR 2+3 batch gate: daemon, workspace-init, and integration buckets all pass after the PID-format extension and atomic-state-file commits land. First run failed two legacy tests parsing PID file as a single int; lead inline-fixed both with PidFile::read_pid (commits 28e38275 + 7d0b9e3f), then rerun went green | `cargo xtask test reliability` | branch-gate | 7d0b9e3f23a8fdcb4afdc1d3edb6ae0f9c1cd3d9 | pass (3 buckets, 65.1s; daemon 11.0s, workspace-init 1.0s, integration 53.1s) | 2026-05-07T00:01:00Z | no |
| PR 4 Tasks 4.1+4.2+4.3 worker invariant: Stale-binary detection's Windows in-place rebuild limitation is documented at function and architecture level; fs2 lock semantics, lock-file persistence, PID format, and state file format are documented in OPERATIONS.md; julie_home_hash's non-UTF-8 collision impact is documented at the function level | `cargo build` (mechanical-tier docs only — no test verification) | worker-red-green | 1cc1a6d9325538ba89f33dc32c7dee808878ad69 | pass (build clean in 0.19s) | 2026-05-07T00:00:24Z | no |
| Lead PR 4 pre-merge gate: full tier (dev + system + dogfood) passes against the final HEAD of the daemon-windows-lifecycle branch before handoff. All 30 buckets green | `cargo xtask test full` | branch-gate | 1cc1a6d9325538ba89f33dc32c7dee808878ad69 | pass (30/30 buckets, 630.1s) | 2026-05-07T00:11:30Z | no |

---

## Model Routing

**Project source of truth:** `RAZORBACK.md` (lines 7-65 model routing table; lines 49-56 explicitly classify daemon lifecycle as shared-invariant work).

**Strategy tier (lead, planning, escalation, gate review, finding triage):**
- Harness mapping: Opus 4.7 (Claude Code lead). Codex review counterpart: `gpt-5.5 high/xhigh` only if escalation trigger fires.

**Implementation tier (bounded worker tasks):**
- **Not eligible unattended for this plan.** Per `RAZORBACK.md` lines 96-101, "daemon, watcher, restart, or concurrency behavior" is on the do-not-use-implementation-tier-unattended list.

**Coupled implementation tier (bounded cross-file work after lead has fixed the contract):**
- Harness mapping: Sonnet 4.6 with explicit lead-defined scopes. Codex counterpart: `gpt-5.3-codex xhigh` for the concurrency, restart, or shutdown behavior tasks (PR 1, parts of PR 2, parts of PR 3); `gpt-5.3-codex high` for shared-invariant tasks without concurrency content.

**Mechanical tier (docs, fixtures, rote edits, no gate ownership):**
- Harness mapping: Haiku 4.5 (Claude) or `gpt-5.4-mini low/medium` (Codex). Used ONLY for PR 4 prose updates. Mechanical workers cannot own failing tests, replay evidence, metrics, or acceptance gates per `RAZORBACK.md` lines 68-82.

**Gate-interpretation reviewer:**
- Harness mapping: Opus (Claude Code lead self-review) or `gpt-5.3-codex high` (Codex external review).

**Worker eligibility (per RAZORBACK.md lines 84-105):**
- Allowed for: PR 4 docs-only tasks (Mechanical tier).
- Allowed under coupled-implementation tier with explicit scopes for: PR 1, PR 2, PR 3 individual tasks where file ownership is narrow and non-overlapping.
- Forbidden unattended for: anything that interprets shutdown ordering correctness, race conditions, or admission gate semantics. Lead reviews every PR 1-3 diff before commit.

**Escalation triggers:** As per Verification Strategy section above.

**Mechanical exclusion:** PR 4's docs worker cannot decide whether the existing comment at `daemon/mod.rs:302-310` is correct; the lead pre-decides what the new prose should say (see Task 4.1 Approach). Worker only edits.

**Unsupported harness behavior:** Claude Code's `Agent` tool accepts only `opus | sonnet | haiku` short names. The plan uses these directly. Codex routing is recorded as RAZORBACK-mapped but only invoked if Codex review is requested at handoff.

---

# PR Groupings (Sequential)

PRs are sequential because each builds on the prior PR's invariants. Do not parallelize PR work; parallelize tasks WITHIN a PR.

---

## PR 1: Daemon shutdown — explicit per-workspace and sidecar teardown

**Theme:** Make resource release ordered and synchronous instead of relying on Drop-during-stack-unwind.

**Findings closed:** #1 🔴 (no explicit per-workspace `SearchIndex` shutdown), #2 🔴 (adapter races OS handle cleanup after `wait_for_pid_exit`), #8 🟡 (embedding shutdown before HTTP shutdown).

**Branch:** off `main`, named `daemon-windows-lifecycle-pr1-shutdown-teardown`.

### Task 1.1: Add `WorkspacePool::shutdown()`

**Files:**
- Modify: `src/daemon/workspace_pool.rs:18-22` (struct), `src/daemon/workspace_pool.rs:170-194` (after evict_workspace)
- Test: `src/tests/daemon/workspace_pool_shutdown.rs` (new)

**What to build:** A new `pub async fn shutdown(&self)` method on `WorkspacePool` that walks every entry in `self.workspaces`, calls `SearchIndex::shutdown()` on each workspace's search index, and clears the map. Mirrors the precedent at `src/handler.rs:626-641` but operates pool-wide instead of per-session.

**Approach:**
- Take a write guard on `self.workspaces`, drain the HashMap into a Vec, drop the guard before invoking shutdown to avoid blocking other reads.
- For each `WorkspaceEntry`, lock the workspace's `search_index` mutex; on poisoned, recover via `into_inner()` (matches `handler.rs:635-639` pattern).
- Call `SearchIndex::shutdown()` and log per-workspace success/failure with `info!` / `warn!`.
- Method must be infallible at the API level (returns `()`); individual workspace failures are logged but don't propagate.
- Document that this is a one-shot teardown and the pool should be dropped after.

**Acceptance criteria:**
- [ ] `WorkspacePool::shutdown()` exists with the documented signature.
- [ ] Test `test_workspace_pool_shutdown_calls_search_index_shutdown` proves: starting with N>=2 workspaces in the pool, after `shutdown()` returns, each workspace's `SearchIndex::is_shutdown()` reports true.
- [ ] Test `test_workspace_pool_shutdown_recovers_from_poisoned_mutex` proves: a poisoned search-index mutex does not crash shutdown; the writer is still dropped.
- [ ] Worker red/green: `cargo nextest run --lib test_workspace_pool_shutdown_calls_search_index_shutdown 2>&1 | tail -10` and the poisoned-mutex variant.
- [ ] Worker invariant in report: "After `WorkspacePool::shutdown()`, no workspace retains an active Tantivy `IndexWriter`; locks are released."

### Task 1.2: Add `WatcherPool::shutdown()`

**Files:**
- Modify: `src/daemon/watcher_pool.rs:37-40` (struct), `src/daemon/watcher_pool.rs:266-278` (after spawn_reaper)
- Test: `src/tests/daemon/watcher_pool_shutdown.rs` (new)

**What to build:** A new `pub async fn shutdown(&self)` method that walks every entry in `self.entries`, calls the existing `detach()` flow per workspace_id, and clears the map.

**Approach:**
- Take a write guard, collect all keys into a Vec, drop the guard.
- For each key, call `self.detach(&key).await` so the existing detach path (which handles ref-counting, grace-period bookkeeping, and `IncrementalIndexer` shutdown) runs once per workspace.
- Note: detach decrements ref_count and cleans up if zero — for shutdown we want unconditional cleanup. If `detach` cannot force this, add a `detach_force` private helper and call that. Verify by reading `detach`'s body before writing the test.
- Log per-workspace cleanup with `info!`.
- Infallible at the API level; per-watcher failures logged.

**Acceptance criteria:**
- [ ] `WatcherPool::shutdown()` exists with the documented signature.
- [ ] Test `test_watcher_pool_shutdown_drops_all_watchers` proves: starting with N>=2 watcher entries, after `shutdown()` returns, `self.entries` is empty (or all entries have `watcher: None`).
- [ ] No `notify` watcher threads survive past shutdown completion (verify by waiting briefly and observing thread count or by waiting on watcher's join handle if available).
- [ ] Worker red/green: `cargo nextest run --lib test_watcher_pool_shutdown_drops_all_watchers 2>&1 | tail -10`.
- [ ] Worker invariant: "After `WatcherPool::shutdown()`, no `IncrementalIndexer` task remains live for any workspace."

### Task 1.3: Make `EmbeddingService::shutdown()` async with bounded child wait

**Files:**
- Modify: `src/daemon/embedding_service.rs:285-289`
- Modify: `src/embeddings/sidecar_provider.rs` (or wherever the Python sidecar `Command` is built and the `Child` handle stored)
- Test: `src/tests/daemon/embedding_service_shutdown.rs` (new)

**What to build:** Convert `EmbeddingService::shutdown(&self)` to `async fn shutdown(&self)` that calls `provider.shutdown()` AND then awaits the underlying sidecar child's exit with a bounded timeout (default 3s, range 2-5s per finding). On Windows, also confirm the sidecar `Command` was built with stdin/stdout/stderr null and `CREATE_NEW_PROCESS_GROUP`.

**Approach:**
- Read the existing `EmbeddingProvider` trait (`src/embeddings/mod.rs:94`-ish) and check whether `shutdown()` already returns or whether sidecar provider stores its `Child` handle.
- If the trait's `shutdown` is sync and fire-and-forget, extend the trait OR add a separate `wait_for_exit(&self, timeout: Duration) -> bool` method on the sidecar provider that callers can opt into.
- Service-level `shutdown` calls `provider.shutdown()` then `provider.wait_for_exit(Duration::from_secs(3))`. If timeout fires, log `warn!` with `embedding sidecar exit timed out` and return; do not crash.
- Windows: in the sidecar provider's `Command` builder, verify or add: `.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())` and `#[cfg(windows)] cmd.creation_flags(CREATE_NEW_PROCESS_GROUP)`. Per the finding, this prevents handle inheritance into the Python sidecar.
- Update the single caller `run_daemon` (mod.rs:668) to `embedding_service.shutdown().await`.

**Acceptance criteria:**
- [ ] `EmbeddingService::shutdown` is `async` and awaits child exit with bounded timeout.
- [ ] Test `test_embedding_service_shutdown_waits_for_child_exit` proves: a fake provider whose child is alive blocks `shutdown` until the child exits or the timeout elapses; result is observable.
- [ ] Test `test_embedding_service_shutdown_returns_on_timeout` proves: a hung child does not block forever; shutdown returns within timeout + tolerance and logs the warning.
- [ ] Sidecar `Command` builder includes null stdio AND (Windows) `CREATE_NEW_PROCESS_GROUP`. Verify by reading the Command-construction site.
- [ ] Worker red/green: `cargo nextest run --lib test_embedding_service_shutdown_waits_for_child_exit 2>&1 | tail -10` and the timeout variant.
- [ ] Worker invariant: "On clean shutdown, the new daemon does not race the old sidecar's handle release."

### Task 1.4: Reorder `run_daemon` shutdown sequence (LIFO)

**Files:**
- Modify: `src/daemon/mod.rs:663-680`
- Test: `src/tests/daemon/shutdown_ordering.rs` (new)

**What to build:** Reorder the shutdown sequence in `run_daemon` from current `embedding → http → pid → state` to LIFO of dependency graph: `http_transport.shutdown().await → embedding_service.shutdown().await → workspace_pool.shutdown().await → watcher_pool.shutdown().await → port file cleanup → pid_file cleanup → state file cleanup`.

**Approach:**
- Reaper handle and cleanup_sweep_handle aborts stay first (lines 663-666).
- HTTP transport shutdown moves before embedding service shutdown so new HTTP requests stop arriving before the embedding provider goes down (closes Finding #8's data-race window).
- Workspace pool and watcher pool shutdowns are new (depend on Tasks 1.1 and 1.2).
- The ordering is: HTTP server stops accepting → embedding service stops (in-flight requests can no longer reach it) → workspace pool releases Tantivy locks → watcher pool drops file watchers → housekeeping files removed.
- Add an integration-style test that records call order via channel-or-flag instrumentation (or by observing log events).

**Acceptance criteria:**
- [ ] `run_daemon` shutdown sequence reflects the new LIFO order.
- [ ] Test `test_shutdown_calls_pools_after_transport` proves: `WorkspacePool::shutdown` is observed AFTER `http_transport.shutdown` completes.
- [ ] Test `test_shutdown_calls_workspace_pool_before_watcher_pool` proves: workspace pool shuts down before watcher pool.
- [ ] No regression in existing shutdown tests under `tests::daemon` (lead-affected-change scope).
- [ ] Worker red/green: `cargo nextest run --lib test_shutdown_calls_pools_after_transport 2>&1 | tail -10` and the ordering variant.
- [ ] Worker invariant: "Shutdown sequence is LIFO of the dependency graph; in-flight HTTP requests cannot observe a torn-down embedding service."

### PR 1 Lead duties before merge

- Run `cargo xtask test changed` after each task batch.
- Run `cargo xtask test reliability` before opening the PR.
- Inline review of all four diffs against acceptance criteria.
- Verify: no new clippy warnings on touched files (`cargo clippy --lib -- -D warnings` scoped to changed files).
- Update PR 1 row in the verification ledger.

---

## PR 2: PID-reuse defense and retry hardening

**Theme:** Strengthen PID file invariants against Windows PID recycling and surface real errors during creation retries.

**Findings closed:** #3 🟠 (no PID-reuse defense), #4 🟠 (`create_exclusive` retry loop has no backoff).

**Branch:** off `main` (rebased on PR 1 once merged), named `daemon-windows-lifecycle-pr2-pid-defense`.

### Task 2.1: Extend PID file format to include creation_time and binary_mtime

**Files:**
- Modify: `src/daemon/pid.rs:14-16` (struct), `src/daemon/pid.rs:27-45` (create), `src/daemon/pid.rs:50-53` (read_pid), `src/daemon/pid.rs:104-114` (check_running), `src/daemon/pid.rs:126-163` (create_exclusive)
- Test: `src/tests/daemon/pid_file_format.rs` (new)

**What to build:** Change PID file format from a single integer to a whitespace-separated triple: `"<pid> <creation_time_unix_micros> <binary_mtime_unix_micros>"`. On read, parse all three fields. On `is_process_alive` and `check_running`, require BOTH PID-alive AND `creation_time` matches the running process's creation time (Windows: `GetProcessTimes::CreationTime`; Unix: `/proc/<pid>/stat` start time, falling back to `clock_gettime(CLOCK_BOOTTIME)` if the proc fs path is not available). The third field (`binary_mtime`) is recorded but only consumed by stale-binary detection (existing code path); writing it here means the daemon's own state is self-describing.

**Approach:**
- Add a new struct `PidFileContents { pid: u32, creation_time_micros: u64, binary_mtime_micros: u64 }` with `parse(&str) -> Option<Self>` and `Display`.
- The on-disk format is a single line: three integers separated by single spaces, terminated by `\n`. This stays human-readable (`cat ~/.julie/daemon.pid` still works for triage).
- Backwards compatibility: if a single-integer file is read, treat as legacy and return `None` from `read_pid` (forces a stale-cleanup path). Document this behavior; do not preserve compatibility.
- Add a `current_process_creation_time() -> Option<u64>` cross-platform helper.
- Add a `process_creation_time(pid: u32) -> Option<u64>` cross-platform helper. On Windows, use `OpenProcess + GetProcessTimes`. On Unix (Linux), parse `/proc/<pid>/stat` field 22; on macOS, use `proc_pidinfo` from `libproc` if available, else accept that creation_time is best-effort and document it.
- Update `check_running` and `is_process_alive` to reject when stored `creation_time` differs from current `process_creation_time(pid)`.

**Acceptance criteria:**
- [ ] PID file write produces a three-field format; read accepts the new format and rejects single-integer legacy files (forced stale).
- [ ] Test `test_pid_file_writes_three_fields` proves: written file matches expected format on a known process.
- [ ] Test `test_check_running_rejects_pid_reuse` proves: synthetic scenario where stored `creation_time` differs from running process's creation_time → `check_running` returns `None` and removes the stale file.
- [ ] Test `test_check_running_accepts_matching_creation_time` proves: live daemon process matches its own stored creation_time.
- [ ] Worker red/green: `cargo nextest run --lib test_check_running_rejects_pid_reuse 2>&1 | tail -10` (and the matching variant).
- [ ] Worker invariant: "A different process that happens to be assigned a recycled PID cannot impersonate a daemon."

### Task 2.2: Add exponential backoff to `create_exclusive` retry loop

**Files:**
- Modify: `src/daemon/pid.rs:126-163`
- Test: `src/tests/daemon/pid_create_exclusive.rs` (new or extend Task 2.1's file)

**What to build:** Replace the current zero-delay retry loop with exponential backoff `Duration::from_millis(50 * (1 << retries))` (50, 100, 200, 400, 800, ... up to 5000ms cap). Propagate non-`NotFound` errors from `fs::remove_file` instead of swallowing them.

**Approach:**
- Add `std::thread::sleep(backoff)` before each retry iteration after the first.
- Cap backoff at `Duration::from_millis(5000)`; with MAX_RETRIES=10, total wait is bounded ~10s.
- Replace `let _ = fs::remove_file(path);` with explicit error handling: if the error kind is `NotFound`, treat as success (file already gone). For `PermissionDenied` or anything else, propagate via `bail!` with full context (prevents Windows `ERROR_SHARING_VIOLATION` 32 from being silently swallowed).
- Test cannot easily simulate Windows sharing violations on macOS, so the test focuses on backoff timing AND the propagation path (mock the remove_file failure via a helper, or accept that the propagation path is covered by the no-mock straight-line test).

**Acceptance criteria:**
- [ ] `create_exclusive` exponential backoff is implemented with the 5000ms cap.
- [ ] Non-`NotFound` `remove_file` errors propagate with context.
- [ ] Test `test_create_exclusive_propagates_remove_file_errors` proves: when `remove_file` returns a non-`NotFound` error (synthesized), `create_exclusive` returns an `Err` containing the error message and stops retrying.
- [ ] Test `test_create_exclusive_backoff_pacing` proves (via wall-clock timing on a contended path): with 3 retries, total elapsed time exceeds the sum of the first three backoffs (50+100+200 = 350ms minimum).
- [ ] Worker red/green: `cargo nextest run --lib test_create_exclusive_propagates_remove_file_errors 2>&1 | tail -10`.
- [ ] Worker invariant: "PID file creation does not silently swallow Windows sharing violations and does not burn ten retries in microseconds."

### PR 2 Lead duties before merge

- Run `cargo xtask test changed`.
- Run `cargo xtask test reliability` (covers `daemon` bucket).
- Inline review of pid.rs delta — flag any path where `binary_mtime` is read but not validated (it should only be diagnostic in this PR; stale-binary detection logic stays as-is).
- Update verification ledger.

---

## PR 3: Atomic state writes, drain timeout, admission short-circuit

**Theme:** Polish remaining medium and low-severity findings into a single coherent PR.

**Findings closed:** #5 🟠 (`daemon.state` non-atomic write), #7 🟡 (5-s drain timeout calibrated for Linux), #9 🟡 (two admission actions both fire `mark_restart_pending`).

**Branch:** off `main` (rebased on PR 1 + PR 2 once merged), named `daemon-windows-lifecycle-pr3-state-and-admission`.

### Task 3.1: Atomic `write_daemon_state` via temp + rename

**Files:**
- Modify: `src/daemon/lifecycle.rs:322-329`
- Test: `src/tests/daemon/daemon_state_atomic.rs` (new)

**What to build:** Replace `std::fs::write(path, state)` with a write-temp-then-rename pattern that mirrors `PidFile::create` (`pid.rs:27-45`). On Windows, `MoveFileExW(MOVEFILE_REPLACE_EXISTING)` is the atomic primitive; `std::fs::rename` invokes it with the right semantics on the same filesystem.

**Approach:**
- Build a sibling `.tmp` path: `path.with_extension("state.tmp")`.
- Write contents to `.tmp` via `fs::write`.
- Call `fs::rename(.tmp, path)` for atomic replacement.
- On any failure, attempt to clean up the `.tmp` file and log a `warn!` (matches existing best-effort semantics — the function returns `()`; failures are advisory).
- The test must demonstrate that no concurrent reader can observe a partial write. Easiest way: spin up a tight loop reading the file in a separate thread, do many writes from the main thread, and assert that every read either returns the previous full contents or the new full contents — never a truncated string.

**Acceptance criteria:**
- [ ] `write_daemon_state` uses temp + rename.
- [ ] Test `test_write_daemon_state_no_partial_reads` proves: 1000 concurrent reads while 1000 writes execute observe only complete state strings (one of the documented values: `"starting"`, `"ready"`, `"draining"`, `"stopping"`).
- [ ] Worker red/green: `cargo nextest run --lib test_write_daemon_state_no_partial_reads 2>&1 | tail -10`.
- [ ] Worker invariant: "Concurrent readers observe only complete state strings; partial reads are impossible after this change."

### Task 3.2: Configurable drain timeout with louder warning

**Files:**
- Modify: `src/daemon/mod.rs:644` (drain_sessions call)
- Modify: `src/daemon/mod.rs` (introduce a tunable constant or env var)
- Test: `src/tests/daemon/drain_timeout.rs` (new)

**What to build:** Increase default drain timeout to 10 seconds, make it configurable via `JULIE_DAEMON_DRAIN_TIMEOUT_SECS` environment variable (range 1-120s, default 10), and elevate the drain-timeout log line from `warn!` to `error!` so it surfaces in default log filters when in-flight work is dropped.

**Approach:**
- Add a small helper `fn drain_timeout() -> Duration` that reads the env var with a default and validates the range.
- Replace the literal `Duration::from_secs(5)` at line 644 with `drain_timeout()`.
- At line 648-652, change `warn!` to `error!` (or add a structured field `severity = "data_loss_risk"` so users grepping for ERROR see it).
- Document the env var in the daemon-mode section of `CLAUDE.md` and `docs/ARCHITECTURE.md` (small one-liner).

**Acceptance criteria:**
- [ ] Default drain timeout is 10s; env var `JULIE_DAEMON_DRAIN_TIMEOUT_SECS` overrides it within range.
- [ ] Drain-timeout log line is `error!` level (or carries `severity = "data_loss_risk"` field, lead's choice).
- [ ] Test `test_drain_timeout_reads_env_var` proves: setting the env var to 7 produces `Duration::from_secs(7)`; out-of-range or unparseable values fall back to the 10s default with a `warn!`.
- [ ] Worker red/green: `cargo nextest run --lib test_drain_timeout_reads_env_var 2>&1 | tail -10`.
- [ ] Worker invariant: "Operators can extend the drain window for slow filesystems without recompiling."

### Task 3.3: Short-circuit `admit_initialize` on first failed admission action

**Files:**
- Modify: `src/daemon/mcp_session.rs:452-498` (admit_initialize body)
- Test: `src/tests/daemon/admit_initialize_short_circuit.rs` (new)

**What to build:** Restructure `admit_initialize` so that if the first `apply_admission_action` (stale-binary gate) returns `Err`, the function returns immediately without invoking the second `apply_admission_action` (version gate).

**Approach:**
- The current code calls both admission actions back-to-back; both can call `mark_restart_pending` which produces confusing log output ("accepted while restart pending" then "rejecting while waiting to restart" for the same admit attempt).
- Use the `?` operator on the first call so an `Err` returns early. Or wrap the second call in an `Ok(_) =>` arm of a match. Either way the invariant is: at most one admission action observes `first_request = true`, and the second call only runs when the first succeeded.
- Verify by adding a test that constructs a `JulieServerHandler` (or mocks `apply_admission_action` if test infrastructure permits) where the stale-binary gate rejects, and asserting that the version gate's call counter is 0 after admit_initialize returns.

**Acceptance criteria:**
- [ ] `admit_initialize` short-circuits on first `Err`.
- [ ] Test `test_admit_initialize_short_circuits_on_stale_binary_reject` proves: when the stale-binary gate would reject, the version gate never fires (call counter = 0).
- [ ] Test `test_admit_initialize_runs_version_gate_when_stale_passes` proves: when the stale-binary gate accepts, the version gate is invoked exactly once.
- [ ] No regression in existing `admit_initialize` tests under `tests::daemon` (lead-affected-change scope).
- [ ] Worker red/green: `cargo nextest run --lib test_admit_initialize_short_circuits_on_stale_binary_reject 2>&1 | tail -10`.
- [ ] Worker invariant: "A failed first admission action prevents the second from observing inconsistent restart-pending state."

### PR 3 Lead duties before merge

- Run `cargo xtask test changed`.
- Run `cargo xtask test reliability`.
- Inline review of all three diffs.
- Update verification ledger.

---

## PR 4: Documentation updates (mechanical-tier eligible)

**Theme:** Tell the truth about Windows in-place rebuilds and capture cosmetic findings as documentation.

**Findings closed:** #6 🟠 (stale-binary detection inert under Windows in-place rebuild — docs only), #10 🟢 (fs2 lock semantics + lock-file persistence), #11 🟢 (non-UTF-8 home path edge case).

**Branch:** off `main` (rebased on PR 1 + 2 + 3), named `daemon-windows-lifecycle-pr4-docs`.

### Task 4.1: Honest comment near `binary_mtime()` capture

**Files:**
- Modify: `src/daemon/mod.rs:302-310` (or wherever `binary_mtime()` is captured at daemon launch — verify exact line range during execution)
- Modify: `docs/ARCHITECTURE.md` (small note in the daemon-mode subsection)

**What to build:** Replace the existing comment near binary_mtime capture with text that admits Windows in-place `cargo build --release` is blocked by the OS file lock; stale-binary detection only fires when the binary is replaced via `MoveFileEx`, an installer, or by deleting + writing a new-named binary out of band.

**Approach:**
- Lead pre-decides the prose. Worker only edits.
- Suggested prose (lead-owned, paste verbatim or lightly adapted):
  > Capture the binary's mtime so we can detect a replacement at runtime.
  >
  > Note (Windows): the running `julie-server.exe` holds an exclusive image-section lock on its own binary, so a developer running `cargo build --release` against a live daemon FAILS with "Access is denied" rather than producing a new binary the daemon could see. This stale-binary detection therefore fires for: (a) installers that use `MoveFileEx(MOVEFILE_REPLACE_EXISTING)`, (b) `touch`-style mtime bumps without byte changes, (c) a delete + new-name + rename sequence done out of band. It does NOT fire for in-place developer rebuilds on Windows; the developer must stop the daemon first.

**Acceptance criteria:**
- [ ] Comment at the binary_mtime() capture site reflects the prose above (lead-decided wording).
- [ ] `docs/ARCHITECTURE.md` daemon-mode subsection notes the Windows in-place rebuild caveat in one or two sentences.
- [ ] No code changes in this task. Lead reviews diff for accuracy of the prose.
- [ ] Worker scope: docs only. No verification command beyond `cargo build` to confirm the file still compiles. Lead owns affected-change verification.

### Task 4.2: Document fs2 lock semantics and lock-file persistence

**Files:**
- Modify: `docs/ARCHITECTURE.md` (add an "Operations and Triage" subsection at the end of the daemon-mode section)
- OR Create: `docs/OPERATIONS.md` if `ARCHITECTURE.md` is already too dense (lead's call during execution)

**What to build:** A short documentation block explaining: (a) on Unix, `daemon.lock` uses advisory `flock`; on Windows it uses mandatory `LockFileEx`; (b) on Windows, force-termination of an adapter mid-syscall can briefly leak a held lock; (c) the lock file is never deleted by the daemon process and accumulating a single stale `daemon.lock` in `~/.julie/` is normal — not a leak, just visible.

**Approach:**
- Lead pre-decides the prose. Worker only edits.
- Place under an "Operations and Triage" subsection so it shows up in `ls ~/.julie/` triage flows.
- Tone: matter-of-fact, no overpromising of fixes that aren't there.

**Acceptance criteria:**
- [ ] Documentation lands in the chosen file.
- [ ] Prose accurately reflects the finding's severity (cosmetic, no fix).
- [ ] Worker scope: docs only.

### Task 4.3: Note non-UTF-8 home path edge case in `julie_home_hash` doc

**Files:**
- Modify: `src/paths.rs` (doc comment near `julie_home_hash` at ~line 78)

**What to build:** Add a doc comment explaining that `to_string_lossy()` mangles non-UTF-8 path segments (rare, mostly legacy Windows filesystems), and that this affects only the `daemon_shutdown_event` name, which is per-user-per-home anyway. No code change.

**Approach:**
- Lead pre-decides the prose. Worker only edits.
- Place as a `///` doc comment just above `pub fn julie_home_hash`.

**Acceptance criteria:**
- [ ] `julie_home_hash` carries the new doc comment.
- [ ] No code change.
- [ ] Worker scope: docs only.

### PR 4 Lead duties before merge

- Run `cargo xtask test changed` (will likely fall back to `dev` due to lifecycle-area touches in PR 1-3 already in this branch).
- Run `cargo xtask test full` as the final pre-merge expensive gate before this stack lands.
- Inline review of all three diffs for prose accuracy.
- Update verification ledger.

---

# Section X: Cross-Cutting

## X.1: Windows CI gap (follow-up issue, no implementation in this branch)

**Why this is in the plan:** The findings doc closes with "a Windows CI run that exercises the restart loop would be the only way to fully validate or refute these findings." Honest reporting requires capturing this so it doesn't fall on the floor.

**What to do:**
- File a GitHub issue titled "Windows CI: integration test for daemon restart loop" referencing this plan and the findings doc.
- Issue body lists: (a) the eleven findings touched, (b) what a Windows CI matrix entry would need to do (start daemon → spawn adapter → simulate stale binary via `MoveFileEx` → assert restart completes within budget → verify Tantivy lock release on the new daemon's startup), (c) the cost-vs-confidence tradeoff (Windows runners on GHA are slower and more expensive but the lifecycle correctness is platform-specific so existing macOS/Linux runs cannot substitute).
- Lead opens the issue at PR 4 merge time, not before — the issue depends on this branch landing so it has somewhere to point.

**Acceptance criteria:**
- [ ] Issue is filed and linked from the merge commit of PR 4.
- [ ] Issue does NOT block this branch.

---

# Execution Sequencing Summary

```
PR 1 (shutdown teardown)          ← Tasks 1.1 → 1.2 → 1.3 → 1.4, lead-reviewed
                ↓
PR 2 (PID-reuse + retry backoff)  ← Tasks 2.1 → 2.2, lead-reviewed
                ↓
PR 3 (atomic + drain + admit)     ← Tasks 3.1 → 3.2 → 3.3, lead-reviewed
                ↓
PR 4 (docs + cosmetic findings)   ← Tasks 4.1 → 4.2 → 4.3, mechanical-tier
                ↓
Open Windows CI follow-up issue   ← X.1
```

Within each PR, tasks may be parallelized across coupled-implementation workers if file ownership stays disjoint. Task 1.1 and 1.2 (different files) can parallelize; Task 1.3 and 1.4 cannot, because 1.4 depends on 1.3's new `async` signature.

---

# Worker Briefing Template

When dispatching a worker for any Task N.M, the prompt must include:

1. **Task ID and PR:** "PR <X> Task <N.M> from `docs/plans/2026-05-06-daemon-windows-lifecycle.md`"
2. **Files in scope:** Exact paths from the task's Files section.
3. **Acceptance criteria:** Verbatim from the task.
4. **Worker red/green test:** The exact `cargo nextest run --lib <name>` command.
5. **Worker invariant:** Verbatim from the task. Worker must restate this in their report.
6. **Verification ceiling:** "Run only the named test. Do not run xtask tiers. Do not run cargo nextest with broad filters. Maximum two test runs per fix cycle."
7. **Julie tool requirement:** "Before modifying any symbol, run `deep_dive(symbol=<name>)` and `fast_refs(symbol=<name>)` to confirm impact. Use `get_symbols(file_path=<path>)` instead of Read."
8. **Failure protocol:** "If the test fails after your fix attempt, stop and report. Do not retry beyond the second attempt. Do not present a failing test as evidence."
9. **Subagent test rules:** Reference `CLAUDE.md` "Subagent & Worker Agent Test Rules" section verbatim.

---

# Glossary

- **Coupled implementation tier:** Per `RAZORBACK.md`, the worker tier appropriate for shared-invariant work (daemon lifecycle, restart, concurrency) when the lead has already fixed the contract. Sonnet 4.6 high or `gpt-5.3-codex xhigh`.
- **Worker red/green test:** A single named test that proves the task's invariant. Run with `cargo nextest run --lib <name>` and a `tail -10` filter.
- **LIFO of dependency graph:** Last-In-First-Out shutdown ordering — components that started last are torn down first, so dependencies remain available until their consumers are gone.
- **Tantivy lock:** The exclusive file lock Tantivy's `IndexWriter` holds on `.tantivy-writer.lock` inside the index directory. Released on `IndexWriter::drop`, but uncommitted writes are lost. `SearchIndex::shutdown()` commits AND drops, releasing the lock cleanly.

---

**Plan version:** 1.0 (2026-05-06)
**Plan author:** Claude Opus 4.7 (lead)
**Spec source:** `docs/findings/2026-05-06-daemon-windows-lifecycle-review.md`
**Branch:** `daemon-windows-lifecycle` (this worktree)
