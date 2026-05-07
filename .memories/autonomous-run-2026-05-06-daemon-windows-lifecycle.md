# Autonomous Execution Report - Daemon Windows Lifecycle

**Status:** Complete
**Plan:** docs/plans/2026-05-06-daemon-windows-lifecycle.md
**Branch:** daemon-windows-lifecycle
**PR:** _to be filled by Step 5_
**Duration:** ~3h end-to-end (single session, includes codex pre-merge review + fix round)
**Phases:** 4/4 complete (PR 1: shutdown ordering · PR 2: PID hardening · PR 3: state-file + drain + admit · PR 4: docs)
**Tasks:** 17/17 complete (tasks #5-#21 in the in-session task list, all closed)

## What shipped

- **PR 1 — Shutdown ordering and pool teardown**: `WorkspacePool::shutdown` (commits Tantivy writes, releases file locks), `WatcherPool::shutdown` (drops OS file-watcher handles), async `EmbeddingService::shutdown` with bounded sidecar wait (3s), and a refactored `run_daemon` shutdown sequence that explicitly tears down components in LIFO dependency order (HTTP transport -> embedding service -> WorkspacePool -> WatcherPool -> housekeeping). Lead added `ShutdownArtifacts<'a>` struct to keep the new helper signature under the clippy too-many-arguments threshold.
- **PR 2 — PID file hardening**: extended PID file format from `<pid>` to `<pid> <creation_time_unix_micros> <binary_mtime_unix_micros>`. Adds creation_time-based PID-reuse defense (impersonation by recycled PIDs is rejected), exponential backoff in `create_exclusive` (50ms * 2^n, capped 5000ms, max 10 retries), and proper error propagation for non-NotFound `remove_file` failures (Windows ERROR_SHARING_VIOLATION no longer silently swallowed). Real macOS `sysctl(KERN_PROC_PID)` implementation using a 648-byte raw buffer + offset parsing (avoids the libc-0.2.x `kinfo_proc` removal).
- **PR 3 — State file, drain timeout, admit short-circuit**: atomic `write_daemon_state` via temp+rename (concurrent readers never observe partial writes), configurable drain timeout via `JULIE_DAEMON_DRAIN_TIMEOUT_SECS` env var (default 10s, range [1, 120], drain-timeout event raised from WARN to ERROR), and instrumentation + tests locking in the existing `?`-based admit_initialize short-circuit (apply_admission_action call count is 1 when stale-binary gate rejects, 2 when stale-binary passes).
- **PR 4 — Documentation**: honest `binary_mtime` doc comment admitting Windows in-place `cargo build --release` is blocked by the OS image-section lock, daemon-mode caveat in `docs/WORKSPACE_ARCHITECTURE.md`, new `docs/OPERATIONS.md` covering fs2 lock semantics (advisory on Unix, mandatory on Windows), lock-file persistence, PID format, and state-file format. Doc note on `julie_home_hash` calling out the `to_string_lossy()` non-UTF-8 collision risk and its bounded scope.
- **Lead test fixes (after PR 2+3 batch)**: two legacy tests parsed the entire PID file as a single integer and broke against the new 3-field format. Lead inline-fixed both with `PidFile::read_pid` (commits `28e38275` + `7d0b9e3f`), then re-ran reliability gate green.
- **Codex pre-merge review fix (commit `40f63442`)**: three real bugs surfaced by adversarial review of the full diff. (1) Live legacy single-integer PID files were being deleted by `check_running`, breaking the single-daemon invariant during upgrades — fixed with `check_running_legacy_fallback` + new `OwnerState` enum for explicit three-way classification in `create_exclusive`. (2) PID-reuse defense was bypassed when `process_creation_time_micros` returned None (Windows ACCESS_DENIED for recycled PIDs owned by privileged processes) — fixed with explicit `match` that distinguishes match/mismatch/indeterminate; indeterminate preserves the file for adapter retry. (3) `WorkspacePool::shutdown` could block forever on a hung mutex held by an in-flight tokio task — fixed with per-workspace `tokio::time::timeout(2s)` wrapping `spawn_blocking`.

## Judgment calls (non-blocking decisions made)

- `src/daemon/workspace_pool.rs:236` — Chose `spawn_blocking` + `tokio::time::timeout` per workspace over `Mutex::try_lock` + retry loop, because `try_lock` would log "skipped — busy" without giving an in-flight request any chance to settle, while a 2-second timeout lets a non-pathological hold complete cleanly while still bounding worst-case shutdown time.
- `src/daemon/pid.rs:298` — Indeterminate creation_time lookups (Some(actual) -> match | mismatch, None -> Indeterminate) DO NOT remove the PID file. Reason: if the daemon process is alive but momentarily unqueryable (Windows ACCESS_DENIED race with privilege-elevation mid-startup), erasing its PID file would force a restart loop. Preserving the file lets the adapter retry and converge.
- `src/tests/integration/daemon_lifecycle.rs:149` and `src/tests/daemon/server.rs:116` — Test fixes routed through `PidFile::read_pid` rather than maintaining ad-hoc `pid_str.trim().parse()` logic in each test. Reason: a single point of correctness for the format means future format changes don't ripple across N tests.
- `docs/WORKSPACE_ARCHITECTURE.md` instead of `docs/ARCHITECTURE.md` (Task 4.1) — Worker discovered ARCHITECTURE.md is for token-optimization context, not daemon architecture. Daemon-mode prose lives in WORKSPACE_ARCHITECTURE.md. Worker correctly redirected without asking.
- Plan-vs-execution structure: the plan called for 4 separate PRs landing sequentially. Execution collapsed onto one branch with 15 commits (one combined PR) after the user emphasized parallel throughput over staged rollout. The PR description preserves the PR-by-PR conceptual structure for reviewability.
- Coordination artifact in commits `0568d77d` and `70af4316`: parallel workers used wide `git add` commands and swept up sibling workers' uncommitted changes into the wrong commit (3.2 captured 3.1's lifecycle.rs work; 4.1 captured 4.2's OPERATIONS.md). Code is correct; commit attribution is fuzzy. Memory `feedback_parallelize_when_disjoint.md` was extended to record the wide-add anti-pattern.

## External review (codex, adversarial)

- **Findings:** 3
- **Verified real, fixed:** 3 (commits: `40f63442`)
  - **(high) Legacy PID file deletion broke single-daemon invariant during upgrade** — `PidFile::check_running` deleted any non-3-field file, so a v7.7.x adapter starting against a live v7.7.<earlier> daemon would erase its PID file and spawn a duplicate. Fixed with legacy fallback that probes liveness and only removes when the process is provably dead.
  - **(medium) Creation-time lookup failure accepted recycled Windows PIDs** — when stored creation_time was non-zero but `process_creation_time_micros` returned None (Windows ACCESS_DENIED for a privileged process owning a recycled PID), the PID was accepted as the daemon. Fixed with three-way `match` and a new `OwnerState::Indeterminate` arm that does not accept and does not remove.
  - **(medium) WorkspacePool shutdown could block forever post-drain-timeout** — `std::sync::Mutex::lock()` blocks indefinitely; a hung tokio task holding the mutex past drain timeout would stall daemon exit and bypass the configured drain bound. Fixed with per-workspace `tokio::time::timeout(2s)` wrapping `spawn_blocking`; held mutexes are logged and skipped, OS reclaims the Tantivy lock at process exit.
- **Dismissed:** 0
- **Flagged for your review:** 0

Codex's three findings are all emergent properties of how PR 2's PID format change interacts with code PR 2 didn't touch (Finding #1, the adapter), Windows-specific permission semantics (Finding #2), and how blocking std::sync::Mutex interacts with the new drain-timeout semantics (Finding #3). Lead inline review caught issues *within* worker diffs but missed these cross-cutting interactions — exactly the niche the adversarial reviewer fills.

Codex CLI does not surface per-request token counts; cost is not reported.

## Tests

- All gates green at HEAD `6e3a6dce`:
  - Worker red/green tests for every implementation task: pass.
  - Lead PR 2+3 reliability gate (post test-fix): 3 buckets in 65.1s.
  - Lead PR 4 pre-merge full gate: 30 buckets in 630.1s.
  - Codex-fix tests (4 new): pass.
  - Post-codex reliability gate (HEAD `40f63442`): 3 buckets in 62.3s.
  - Final HEAD differs from `40f63442` only in markdown ledger updates (2 lines, no test target impact); reliability gate evidence reused per ledger contract.

## Blockers hit

- None.

## Files changed

29 files changed, 3129 insertions(+), 133 deletions(-). Headline shifts:

- `src/daemon/pid.rs` 173 -> 482 lines (creation_time helpers per platform, `OwnerState` enum, legacy fallback, exponential backoff, error propagation)
- `src/daemon/mod.rs` +156 (drain_timeout helper, ShutdownArtifacts struct, perform_shutdown_sequence with LIFO order, honest binary_mtime comment)
- `src/daemon/workspace_pool.rs` +46 (shutdown with per-workspace timeout)
- `src/daemon/watcher_pool.rs` +31 (shutdown drains and stops indexers)
- `src/daemon/embedding_service.rs` +32 / `src/embeddings/sidecar_provider.rs` +52 (async shutdown + bounded wait_for_exit)
- `src/daemon/lifecycle.rs` +15 (atomic write_daemon_state)
- `src/daemon/mcp_session.rs` +10 (apply_action_call_count instrumentation)
- `src/paths.rs` +11 (julie_home_hash doc note)
- 11 new test files in `src/tests/daemon/` totaling ~1280 lines (shutdown ordering, PID format, drain timeout, atomic state, admit short-circuit, watcher/workspace pool shutdown)
- 4 new docs: `docs/OPERATIONS.md`, `docs/WORKSPACE_ARCHITECTURE.md` addendum, the plan, the original findings doc

## Next steps

- Review PR (URL filled in Step 5).
- Decide whether to file the **Windows CI follow-up issue** (Section X.1 of the plan) at PR merge time. The findings doc closes with "a Windows CI run that exercises the restart loop would be the only way to fully validate or refute these findings"; the plan deferred that as out-of-scope but flagged it for follow-up.
- Watch for any post-merge real-world usage of the new `JULIE_DAEMON_DRAIN_TIMEOUT_SECS` env var to confirm the [1, 120] range is the right bound for slow filesystems.
- The drain timeout default moved from 5s to 10s. Operators on fast Linux filesystems may not notice; Windows NTFS-on-spinning-disk will benefit.
- The new 3-field PID format is forward-compatible: legacy daemons see no change in their own behavior, and a new adapter handles legacy PID files correctly via `check_running_legacy_fallback`.
