# Julie Daemon Split + Search Reranker — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Split `julie-server` into `julie-adapter` + `julie-daemon` with a kernel-locked singleton + atomic discovery + token file, hard legacy-migration gate, and bounded shutdown drain; switch MCP request handling to connection-per-request with the existing `mutation_gate` as the sole writer-serialization point; make the daemon embeddable for in-process tests; add a Tantivy reranker with intent detection, kind boosts, and role-aware reweighting (preserving target-specific `exclude_tests` defaults).

**Architecture:** Two binaries from the same workspace via `[[bin]]` targets. Adapter has no SQLite/Tantivy/extractors/embeddings — only stdio↔HTTP forwarding and detached-child spawn. Daemon owns all heavy state, exposed via embeddable `DaemonApp::serve(listener)` so production and tests share one entry point. Concurrency uses per-workspace `WorkspaceConnectionPool` (each request gets its own rusqlite connection) plus the existing `mutation_gate` for write serialization across MCP handlers, the watcher, catch-up, and register flows. Reranker is a re-ranking stage over Tantivy results with no schema-breaking changes outside three new optional fields.

**Tech Stack:** Rust, tokio, axum, rusqlite (bundled), Tantivy 0.26, fcntl/LockFileEx for kernel locks, tracing, anyhow. Cross-platform code paths required (macOS / Linux / Windows). No new third-party dependencies are anticipated; if any are needed (e.g. `rusqlite_pool` shape), call it out in the task and confirm before adding.

**Architecture Quality:** See `docs/plans/2026-05-15-daemon-split-and-search-reranker-design.md` (v2). Approved module/interface shape and rejected shortcuts are recorded there; this plan does not relitigate them. **Architecture risk:** medium. Primary residual risk is legacy-daemon coexistence during plugin upgrade — explicit migration-gate test required (Phase A.1 task list).

**Design source of truth:** `docs/plans/2026-05-15-daemon-split-and-search-reranker-design.md` (committed at `7e141243`). If code reality contradicts the design, the worker reports a plan mismatch — do not redesign locally.

---

## File Structure

This plan changes the daemon and adapter substantially. Files written, modified, or removed by phase:

### Phase A.1 — lifecycle + binaries

**Create:**
- `src/bin/julie-adapter.rs` — thin adapter entry (~30 lines: parse args, delegate to `crate::adapter::run`)
- `src/bin/julie-daemon.rs` — daemon entry with `start|stop|status` subcommands (~80 lines)
- `src/daemon/app.rs` — `DaemonApp`, `DaemonConfig`, `DaemonHandle`, `DaemonRuntimeContext` — the embeddable daemon (~300 lines)
- `src/daemon/discovery.rs` — `DiscoveryFile`, `DiscoveryRecord`, atomic write, `DaemonLockGuard` kernel-lock RAII (~250 lines)
- `src/daemon/legacy_migration.rs` — legacy-file detection, hard-gate logic, attach-to-legacy support for adapter (~200 lines)
- `src/daemon/token_file.rs` — write `daemon.token` with mode 0600 / restricted ACL, read on adapter side (~80 lines)
- `src/daemon/shutdown.rs` — bounded drain + recovery markers (~150 lines)
- `src/tests/integration/legacy_migration.rs` — end-to-end test: upgraded adapter against running legacy daemon
- `src/tests/integration/discovery_atomic.rs` — atomic write durability + pid_creation_time identity under simulated PID reuse

**Modify:**
- `Cargo.toml` — add `[[bin]] name = "julie-adapter"` and `[[bin]] name = "julie-daemon"` alongside existing `julie-server`. Keep `julie-server` building for one release cycle as a compatibility shim.
- `src/main.rs` — becomes the `julie-server` compatibility shim: dispatches to adapter or daemon based on argv (no-args → adapter; `daemon`/`stop`/`status` → daemon).
- `src/paths.rs` — add `daemon_lock()`, `discovery_file()`, `token_file()`, `legacy_singleton_lock()`, `legacy_daemon_pid()`. Keep existing legacy getters until cleanup release.
- `src/daemon/mod.rs` — `run_daemon` becomes a thin wrapper that constructs `DaemonApp` + binds listener + calls `.serve()`. Target: ≤200 lines, the rest moves into `app.rs`.
- `src/daemon/lifecycle.rs` — delete `DaemonLifecycleController`, `LifecyclePhase`, `restart_pending` machinery, `binary_mtime` mtime tracking. Keep only what shutdown actually uses.
- `src/daemon/singleton.rs` — replaced by kernel-held lock in `discovery.rs`. Module deleted; legacy detection logic moves to `legacy_migration.rs`.
- `src/daemon/pid.rs` — kept for `pid_creation_time` query helpers (called from discovery + legacy_migration), but PID-file-as-lifecycle removed.
- `src/adapter/launcher.rs` — replace `daemon_readiness` poll-loop on legacy state files with `DiscoveryFile::read_and_validate(paths)` + legacy-detection branch. Spawn detached `julie-daemon start` instead of re-exec'ing self.
- `src/adapter/mod.rs` — minor: `run_adapter` no longer needs `WorkspaceStartupHint` plumbing for daemon-side concerns it didn't actually own.
- `~/.julie/plugin-manifest` equivalent (`julie-plugin` repo): plugin manifest update to spawn `julie-adapter` instead of `julie-server`. (Lands in a separate PR in `~/source/julie-plugin`.)

**Remove (in this phase or by the cleanup release):**
- `src/daemon/lifecycle.rs::DaemonLifecycleController` and related types
- `src/daemon/singleton.rs` (whole module)
- `src/daemon/shutdown_event.rs` is *kept* — it's the Windows shutdown event for `julie-daemon stop`, still needed.

### Phase A.2 — connection-per-request

**Create:**
- `src/daemon/connection_pool.rs` — per-workspace SQLite connection pool (~200 lines). API: `pool.acquire(workspace_id).await -> PooledConn`; uses an internal `Vec<rusqlite::Connection>` with `idle_timeout = 60s`; min/max sizes derived from CPU count.

**Modify:**
- `src/handler.rs` (96KB!) — each MCP tool handler acquires a pooled connection from `WorkspaceConnectionPool` instead of using the shared `WorkspacePool`-issued connection. **Do not refactor the file shape**; just change the call sites. (96KB file is over budget per CLAUDE.md but pre-existing; the connection-pool conversion does not require splitting it. Flag for later.)
- `src/daemon/workspace_pool.rs` — `WorkspacePool` no longer holds the per-workspace SQLite connection. It still owns Tantivy index handles, watcher handle, and the connection-pool reference. Connections come from `WorkspaceConnectionPool`.
- `src/handler/tools/*` — adjust each handler to call the connection pool. Read handlers: open `BEGIN DEFERRED TRANSACTION` for snapshot read. Mutating handlers: acquire `mutation_gate::acquire_gate(workspace_id)` then open the connection.
- `src/workspace/mutation_gate.rs` — already does what's needed; only mechanical change is to ensure all mutating MCP handlers call it. The watcher/catch-up/register paths already do (per design doc).
- `src/tests/integration/concurrent_mcp.rs` (new) — 8 concurrent MCP requests (mixed read+write) on one workspace + active filewatcher event stream; verify all complete within bounded time without wedging. (Path follows julie's `src/tests/**` convention per CLAUDE.md.)

### Phase B — in-process test harness

**Create:**
- `src/tests/harness/in_process.rs` — `InProcessDaemon` test fixture: builds `DaemonConfig` with `DaemonRuntimeContext::for_test()`, binds 127.0.0.1:0, returns `DaemonHandle` + bearer token + URL.
- `src/tests/harness/mod.rs` if not present.

**Modify:**
- `src/daemon/app.rs` — already has `DaemonRuntimeContext` from A.1; expose `for_test()` constructor returning a registry-isolated instance.
- `src/workspace/mutation_gate.rs` — add `Registry::new()` constructor for injectable use; production code keeps using `acquire_gate(workspace_id)` which routes to the global registry; tests construct an isolated `Registry` and route through `DaemonRuntimeContext`.
- `src/daemon/watcher_pool.rs` — accept a `max_watchers` config (default unbounded for production, 4 for tests).
- Tests in `src/tests/daemon/`, `src/tests/integration/daemon_lifecycle.rs`, and anywhere that spawns `julie-server` via `tokio::process::Command` — migrate to `InProcessDaemon`. Keep a small handful of explicit adapter-integration tests that still use a real subprocess.

### Phase C — search reranker

**Create:**
- `src/search/reranker.rs` — `rerank(query: &ParsedQuery, candidates: &[Candidate]) -> Vec<Ranked>`, including the scoring functions sketched in design C.3.
- `src/search/query_parse.rs` — `parse_query(raw: &str) -> ParsedQuery` with `QueryIntent::{Free, Symbol(SymbolKind), Test}`.
- `fixtures/search-quality/test-helper-discoverability.json` — regression queries proving definition search still finds test helpers.

**Modify:**
- `src/search/schema.rs` — add optional Tantivy text fields: `role`, `test_role`, `capability_flags`. Index existing symbols with default values until next index rebuild.
- `src/search/index.rs` (56KB, also pre-existing over budget) — populate the three new fields during indexing. Lookup helpers for reranker.
- `src/tools/search/text_search.rs` — `definition_search_with_index` and `content_search_with_index` call into `reranker::rerank()` after Tantivy returns candidates. `filter_test_symbols` (line 214) is **deleted** in this phase or its caller is updated — the reranker subsumes filtering+reweighting. Preserve today's target-specific defaults: definitions include tests, NL excludes tests unless test intent detected.
- `src/search/scoring.rs` — `apply_centrality_boost` stays, called inside reranker as the base layer; the new boosts compose on top.
- `xtask` / fixture loaders — `cargo xtask test dogfood` already exists; add the new fixture to the dogfood corpus.

---

## Verification Strategy

**Project source of truth:** `CLAUDE.md` ("RUNNING TESTS" section) defines julie's calibrated test tiers via `cargo xtask test`. Subagent test discipline section is non-negotiable: workers run narrow targeted tests only; the lead owns broader regression gates.

**Worker red/green scope:** A worker writes a failing test and verifies it fails, then writes implementation and verifies the *same exact test* passes, using `cargo nextest run --lib <exact_test_name> 2>&1 | tail -10`. No `cargo xtask test changed` from workers, no broad module filters, no test runs without an explicit name filter.

**Worker ceiling:** the single named test per change cycle. Two test runs total per fix (RED then GREEN). Workers do not own regression gates.

**Worker gate invariant:** Each worker proves the specific behavior its task adds — the test that fails before the change and passes after. Workers do not interpret broader gate signal; if they need to, that's an escalation, not a worker action.

**Lead affected-change scope:** After a coherent batch of tasks completes (typically the end of a phase or a meaningful sub-section), the lead runs `cargo xtask test changed`. This maps the git diff to the smallest matching bucket set. If shared infrastructure moved, `changed` falls back to `dev` automatically.

**Branch gate:** Before declaring a phase complete and merging to main, the lead runs `cargo xtask test dev` once. Per CLAUDE.md, this is the canonical batch-level regression tier.

**Replay/metric evidence:** Phase C has search-quality fixtures. `cargo xtask test dogfood` is the metric gate. Required: no regression on the existing search-quality fixture, AND no regression on the new test-helper-discoverability fixture (Phase C task). Both are hard gates; ledger row records `top5` and `mrr` per query category.

**Escalation triggers (apply for the whole plan):**
- Any concurrency-related test failure on Phase A.2 (suspected deadlock or torn read/write) → escalation tier reviews. Do not patch around.
- Legacy-migration-gate test failures (Phase A.1) → escalation tier reviews.
- Search-quality regression on the dogfood fixture (Phase C) → escalation tier reviews; revert reranker default-flag flip.
- Any test marked `[thorough]` failing — see CLAUDE.md.

**Assigned verification failure:** Workers stop and report when their named test fails after their fix; the orchestrating lead decides whether the test was wrong, the fix was wrong, or scope expanded. Workers do not silently re-scope.

**Verification ledger:** Use `docs/plans/verification-ledger-template.md`. Record invariant, command, scope label (worker/affected-change/branch/expensive), commit SHA, result, timestamp. Reuse evidence only when scope and commit SHA both match HEAD; rerun otherwise. For Phase C metric evidence, record `top5_hits`, `mrr`, and any per-category deltas; flag report-only deltas vs. hard-gate deltas.

---

## Model Routing

**Project source of truth:** `RAZORBACK.md`. Reproduced below as far as this plan's lanes need.

**Strategy tier:** planning, architecture, decomposition, lead review, finding triage
- Codex: `gpt-5.5` medium/high
- Claude: Opus (high-risk) or Sonnet (lower-risk)
- OpenCode: strongest available reasoning model

**Implementation tier:** bounded worker tasks from a clear plan
- Codex: `gpt-5.5` low (default), `gpt-5.5` medium when judgment needed
- Claude: Sonnet or Haiku for narrow boxed-in edits
- OpenCode: fast implementation model

**Mechanical tier:** docs, fixtures, rote edits, formatting, manifests with no test/replay/metric ownership
- Codex: `gpt-5.4-mini` low/medium
- Claude: Haiku
- OpenCode: fastest reliable model

**Gate-interpretation reviewer:** failing test or replay + diff triage
- Codex: `gpt-5.5` high (or `gpt-5.3-codex` high for terminal-heavy diagnosis)
- Claude: Opus or Sonnet high

**Escalation tier:** security, subtle correctness, high blast radius, weak tests, repeated failures, gate interpretation
- Codex: `gpt-5.5` high/xhigh
- Claude: Opus
- OpenCode: strongest available

**Worker eligibility:** implementation-tier workers may own any single-file or narrow multi-file task in this plan **except** the following, which require either escalation-tier ownership or coupled-implementation with strategy-tier-owned interface contracts (per RAZORBACK.md: lifecycle, concurrency, shared database/workspace behavior, and search ranking/scoring are non-implementation-tier unattended work):

**Escalation-tier owns (high blast radius, subtle correctness):**
- A1.2 — kernel advisory lock implementation (cross-platform OS API safety)
- A1.5 — legacy migration gate (regressing this corrupts user indexes)
- A1.7 — bounded shutdown drain (concurrency + recovery semantics; getting drain wrong loses writes silently)
- A2.3 — concurrent_mcp regression test design + lock-order proof
- C.4 — target-specific `exclude_tests` preservation (codex finding #7 surface; regression is silent)

**Coupled-implementation tier (strategy-tier owns the interface contract before edits; implementation-tier worker executes against the locked contract):**
- A1.6 — `DaemonApp` extraction (high blast radius; strategy tier owns the new caller-facing surface, including `DaemonRuntimeContext` field set, before any code moves)
- A2.2 — wire connection pool + mutation gate into MCP handlers (shared database invariants; strategy tier locks the handler-entry pattern + which handlers are mutating)
- B.1 — `Registry::new()` + `DaemonRuntimeContext` plumbing (production code must reach gates through the runtime context; strategy tier locks the routing contract so A2.2 has a stable API to wire against)
- C.1 — `parse_query` semantics (search-ranking invariants; strategy tier locks the intent-classification rules and edge cases before implementation)
- C.2 — reranker scoring function (search-ranking invariants; strategy tier locks the boost matrix and field-priority rules)
- C.3 — enriched Tantivy schema (search-schema invariants; strategy tier locks the field set, types, and role-classification rules across all 8+ project layouts)

**Mechanical-tier workers can own:** fixture file authoring, `Cargo.toml` bin-target adds (A1.1), plugin manifest update (A1.9), `julie-server` compatibility-shim main.rs delegation, ledger row entries, the `C.5` flag flip *after* escalation tier signs off on regression evidence.

**Escalation triggers:** see Verification Strategy escalation triggers — they apply here too.

**Mechanical exclusion:** Mechanical workers cannot own a failing test, the concurrent_mcp regression, search-quality dogfood gate, or any verification-tier decision.

**Unsupported harness behavior:** If a harness cannot choose models per agent, use `inherit` and note in the worker report.

---

## Phase A.1 — Lifecycle + Binaries (Daemon Split)

Sequenced tasks. Most independent within the phase; ordering noted where it matters. Each task ends with worker-scope verification and a commit.

### Task A1.1: Add `julie-adapter` and `julie-daemon` binary targets

**Files:**
- Modify: `Cargo.toml` — add two `[[bin]]` entries
- Create: `src/bin/julie-adapter.rs` (~20-line stub calling `julie::adapter::run`)
- Create: `src/bin/julie-daemon.rs` (~20-line stub calling `julie::daemon::cli::run`)
- Create: `src/daemon/cli.rs` — `pub async fn run() -> Result<()>` parsing `start|stop|status` argv, dispatching. Stubs for the subcommands that return `unimplemented!` for now (filled by later tasks).

**What to build:** Add the two new binary targets so cargo can build them. Adapter stub delegates to existing `crate::adapter::run_adapter` (works today). Daemon stub `start` subcommand delegates to existing `crate::daemon::run_daemon` (works today). This task does *not* change behavior; it just adds entry points.

**Approach:** Keep `julie-server` building unchanged. Production behavior unchanged. After this task, `cargo build --release --bins` produces three binaries; `julie-adapter` and `julie-daemon start` should both work end-to-end through the existing code paths.

**Acceptance criteria:**
- [ ] `cargo build --release --bins` succeeds, producing `julie-adapter`, `julie-daemon`, and `julie-server`.
- [ ] `julie-adapter` invoked with no args behaves identically to today's `julie-server` (no args).
- [ ] `julie-daemon start` behaves identically to today's `julie-server daemon`.
- [ ] `cargo nextest run --lib build_julie_fixture` (or whichever fast smoke test exercises spawn) passes.
- [ ] Mechanical-tier eligible.

### Task A1.2: Kernel-held advisory lock + `DaemonLockGuard`

**Files:**
- Create: `src/daemon/discovery.rs` (lock half only this task; discovery file added in A1.3)
- Test: `src/tests/daemon/lock_test.rs` (or extend existing `daemon/server.rs`)

**What to build:** `DaemonLockGuard` owning a held file descriptor on `~/.julie/daemon.lock` with the OS-native advisory lock. On POSIX: `fcntl(F_SETLK, F_WRLCK)` on a long-lived fd. On Windows: `LockFileEx(LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY)`. Drop releases the lock; OS releases on process exit (clean or crash).

**Approach:** Use `nix` or `rustix` for POSIX fcntl; `windows-sys` for `LockFileEx`. (julie may already depend on one of these — check before adding.) The lock file persists across daemon lifetimes; only the lock itself is per-process. Provide `try_acquire(path) -> Result<DaemonLockGuard, LockAlreadyHeld>`. Cross-platform integration test that verifies a second concurrent `try_acquire` fails while the first guard is held.

**Acceptance criteria:**
- [ ] Failing test asserts `try_acquire` returns `LockAlreadyHeld` when another process holds the lock; passes after implementation.
- [ ] Test for "lock released on guard drop" — second acquire succeeds after the first guard is dropped.
- [ ] Cross-platform: test runs on both POSIX (CI macOS/Linux) and a Windows path or is marked `#[cfg(unix)]`/`#[cfg(windows)]` with both branches covered.
- [ ] Worker test: `cargo nextest run --lib test_daemon_lock` passes.
- [ ] **Escalation tier owns** — cross-platform OS API surface; bugs here corrupt indexes.

### Task A1.3: Discovery file (atomic write + identity validation)

**Files:**
- Extend: `src/daemon/discovery.rs` with `DiscoveryRecord`, `DiscoveryFile::write_atomic`, `DiscoveryFile::read_and_validate`
- Modify: `src/paths.rs` — add `discovery_file()`, `token_file()` getters
- Test: `src/tests/daemon/discovery_test.rs`

**What to build:** `DiscoveryRecord` matching design A.2 schema (`pid`, `pid_creation_time_ns`, `host`, `port`, `token_path`, `log_path`, `daemon_version`, `protocol_version`, `schema_version`, `started_at`). Atomic write via temp file + `fsync(file)` + `rename` + `fsync(parent_dir)`. `read_and_validate` returns `DiscoveryState::{Live, Stale, Missing, Corrupt}` based on pid + creation_time check.

**Approach:** Use `pid::pid_creation_time(pid)` helper from existing `src/daemon/pid.rs` (already implements creation_time lookup per codex review). Parent-dir fsync requires opening the parent directory and calling fsync on its fd — POSIX-only; Windows skips this (Windows rename is already atomic on NTFS). Pure-function tests for serialization round-trip. Integration test simulates crash by writing a discovery record then killing the recorded pid and verifying the next reader returns `Stale`.

**Acceptance criteria:**
- [ ] Failing test for atomic write durability: write a record, simulate crash mid-write (write tmp file, fsync, do NOT rename), then `read_and_validate` returns `Missing` (no half-written file visible). Passes after implementation.
- [ ] Failing test for PID-reuse defense: write a record with a known dead pid (or fabricate creation_time mismatch), `read_and_validate` returns `Stale`. Passes after implementation.
- [ ] Schema round-trip test.
- [ ] Worker scope: `cargo nextest run --lib test_discovery_atomic_write` etc.
- [ ] Implementation tier.

### Task A1.4: Token file (mode 0600 / restricted ACL)

**Files:**
- Create: `src/daemon/token_file.rs`
- Test: `src/tests/daemon/token_file_test.rs`

**What to build:** `write_token(path: &Path, token: &str) -> Result<()>` that writes the token and explicitly enforces mode `0600` on POSIX (`std::os::unix::fs::PermissionsExt`) and a restricted ACL on Windows (current-user-only via `windows-acl` or a manual `SECURITY_DESCRIPTOR` build). `read_token(path) -> Result<String>` returns the contents.

**Approach:** POSIX path is straightforward (`OpenOptions::new().mode(0o600).write(true).create(true).open()` plus an explicit `set_permissions` post-write to handle umask-stripping platforms). Windows ACL is the harder half — if `windows-acl` is not already a dep, prefer a documented `cacls`-equivalent via `windows-sys` over adding a crate. Test asserts file mode is 0600 after write on POSIX; skip mode check on Windows but verify file exists.

**Acceptance criteria:**
- [ ] POSIX: failing test that `std::fs::metadata(path).permissions().mode() & 0o777 == 0o600` after write; passes after implementation.
- [ ] Round-trip: `write_token` then `read_token` returns the same string.
- [ ] Token file path is what `paths.token_file()` returns.
- [ ] Implementation tier.

### Task A1.5: Legacy migration gate

**Files:**
- Create: `src/daemon/legacy_migration.rs`
- Create: `src/tests/integration/legacy_migration.rs` — full end-to-end test (path per CLAUDE.md `src/tests/**` convention; runs under `cargo nextest run --lib`)
- Modify: `src/daemon/cli.rs` `start` subcommand — call `legacy_migration::check_or_refuse` before `DaemonApp::serve`
- Modify: `src/adapter/launcher.rs` `daemon_readiness` / `ensure_daemon_ready` — call `legacy_migration::detect_and_attach` before auto-spawn

**What to build:** Hard gate function `check_or_refuse(paths) -> Result<MigrationDecision>` that examines `daemon.singleton.lock`, `daemon.pid`, `daemon.state`, `daemon.lock` on disk. For each: if the file exists, attempt to determine whether a live legacy daemon owns it (fcntl probe for `daemon.singleton.lock`; `pid_creation_time` check for `daemon.pid`). If any legacy file is owned by a live process → return `MigrationDecision::LegacyDaemonAlive { pid }`. If all are dead/absent → return `MigrationDecision::ProceedAndUnlink` listing files to clean up.

For the adapter side: `detect_and_attach(paths) -> Option<TransportEndpoint>` returns `Some(endpoint)` if a live legacy daemon is detected (reads `daemon.port` file for the endpoint URL).

**Approach:** Existing `pid_creation_time` + `SingletonLock::try_acquire` logic informs the implementation. Reuse rather than re-implement. The end-to-end test is the hard part: spin up a real legacy `julie-server daemon` subprocess in a temp `JULIE_HOME`, then invoke `julie-daemon start` in the same `JULIE_HOME` and assert it refuses with the diagnostic. Then invoke `julie-adapter` and assert it attaches to the legacy daemon's HTTP endpoint rather than spawning a new daemon.

**Acceptance criteria:**
- [ ] End-to-end test in `src/tests/integration/legacy_migration.rs`: live legacy daemon + new `julie-daemon start` → daemon refuses to start, exit code non-zero, stderr contains "legacy daemon is running".
- [ ] End-to-end test: live legacy daemon + new `julie-adapter` → adapter connects to legacy endpoint, forwards a basic MCP request successfully, no new daemon process spawned.
- [ ] Unit test: dead legacy `daemon.pid` (file exists, recorded pid is dead) → `check_or_refuse` returns `ProceedAndUnlink`.
- [ ] Unit test: legacy `daemon.singleton.lock` with no process holding the fcntl lock → treated as dead, unlinked.
- [ ] `cargo nextest run --lib test_legacy_migration_gate` passes (test under `src/tests/integration/` is in the lib test target by julie's convention).
- [ ] **Escalation tier owns.** This is finding #2; getting it wrong corrupts user indexes.

### Task A1.6: `DaemonApp` and lifespan refactor

**Files:**
- Create: `src/daemon/app.rs` — `DaemonApp`, `DaemonConfig`, `DaemonHandle`, `DaemonRuntimeContext`
- Modify: `src/daemon/mod.rs` — `run_daemon` becomes ≤200-line wrapper
- Modify: `src/daemon/cli.rs` — `start` calls `DaemonApp::new(config)?.serve(listener).await`

**What to build:** The embeddable daemon surface from design B.1, plus moving the body of today's 510-line `run_daemon` into `DaemonApp::serve`. Component instantiation order moves into typed builders (e.g. `WatcherPool::builder().with_grace(...).build()`) where it clarifies dependencies; otherwise stays sequential. `DaemonRuntimeContext` exists as a config field; default (production) wires to global singletons, `for_test()` constructor isolates them.

**Approach:** This is mostly mechanical-with-judgment. Move code, don't rewrite it. Watch for these structural patches that need preserving:
- The lazy embedding-init background task (today: `tokio::spawn` after lifecycle marks ready) — keep the same structure.
- `tokio::select!` over shutdown signal / restart-pending / stop-event becomes shutdown signal / stop-event only (restart-pending removed).
- `perform_shutdown_sequence` becomes part of `DaemonHandle::shutdown` and `DaemonApp` Drop semantics.
- Dashboard HTTP server is still spun up here — separate router on a separate port. Same as today.

**Acceptance criteria:**
- [ ] `src/daemon/mod.rs::run_daemon` is ≤200 lines.
- [ ] Existing daemon integration tests (`src/tests/integration/daemon_lifecycle.rs`, `src/tests/daemon/server.rs`) still pass — they currently use `run_daemon`, which now delegates to `DaemonApp`.
- [ ] **New direct test of the embeddable surface** (added in this task): `src/tests/daemon/app_test.rs::test_daemon_app_serve_and_shutdown` — binds `127.0.0.1:0`, calls `DaemonApp::new(test_config)?.serve(listener).await`, hits the `/status` endpoint via reqwest, asserts a 200 response, calls `handle.shutdown().await`, verifies the listener is released (a second `TcpListener::bind` on the same `local_addr` would succeed but we don't rely on that — instead verify the join handle resolved cleanly within 5s). This test must exist and pass in this task; without it the new caller-facing surface is unverified until B.3 migration. Failing-first RED: write the test before code moves, watch it fail with "DaemonApp not defined." GREEN after code moves. Codex review #4 (medium) target.
- [ ] `DaemonApp::new(config).serve(listener).await` returns a `DaemonHandle` whose `shutdown()` cleanly stops the server.
- [ ] Worker scope: `cargo nextest run --lib test_daemon_app_serve_and_shutdown` passes, plus existing daemon lifecycle tests at the named-test level continue to pass.
- [ ] **Coupled-implementation tier.** Strategy tier owns the new caller-facing surface (`DaemonApp` / `DaemonConfig` / `DaemonHandle` / `DaemonRuntimeContext` field set) before any code moves; implementation tier executes against the locked contract. High blast radius, sequencing critical.

### Task A1.7: Bounded shutdown drain + recovery markers

**Files:**
- Create: `src/daemon/shutdown.rs` — bounded drain logic, recovery marker write/read
- Modify: `src/daemon/app.rs` — `DaemonHandle::shutdown` integrates the drain
- Modify: `src/daemon/cli.rs` `status` subcommand — surface recovery markers from `/status` endpoint
- Test: `src/tests/daemon/shutdown_drain_test.rs`

**What to build:** Per design A.5. On shutdown signal: publish `phase=stopping` in `discovery.json` (atomic rewrite), reject new HTTP with 503, drain up to `JULIE_DAEMON_DRAIN_TIMEOUT_SECS` (default 60, range 1-120). On timeout: write `.unclean_shutdown` markers under affected workspace index dirs listing in-flight mutations, force-abort with 502. Next `DaemonApp::new` reads markers and surfaces via `/status`.

**Approach:** Track in-flight mutating requests via existing session tracker (or extend it). The 503/502 distinction must be wired into the handler error path (today's handler can just panic on shutdown — needs explicit error code).

**Acceptance criteria:**
- [ ] Failing test: start daemon, issue mutating request that holds the mutation gate for 5s, send shutdown with `JULIE_DAEMON_DRAIN_TIMEOUT_SECS=10`, verify request completes cleanly. Passes after implementation.
- [ ] Failing test: same setup with timeout=2s, verify request is aborted with 502, recovery marker file present.
- [ ] `/status` endpoint returns `{ "recovery_markers": [...] }` if any are present at the next daemon startup.
- [ ] `cargo nextest run --lib test_shutdown_drain` passes.
- [ ] Implementation tier (coupled to A.6/A.7, may benefit from gate-interpretation reviewer on the recovery-marker semantics).

### Task A1.8: Wire it together + julie-server compatibility shim

**Files:**
- Modify: `src/main.rs` — becomes compatibility shim (dispatches argv to adapter or daemon)
- Modify: `src/adapter/launcher.rs` — spawn `julie-daemon start` as detached child instead of re-execing self
- Modify: `src/adapter/mod.rs::run_adapter` — drop `WorkspaceStartupHint` parameter if no longer meaningful (it's plumbed to a launcher concern)

**What to build:** Final wiring. After this task, `julie-adapter` is the binary the plugin should spawn, and it correctly spawns `julie-daemon start` if no live daemon is detected. `julie-server` (the old binary) still works as a compatibility shim by inspecting argv:
- no args → `julie-adapter` codepath
- `daemon` → `julie-daemon start`
- `daemon stop` → `julie-daemon stop`
- `daemon status` → `julie-daemon status`

**Approach:** Detached child spawn on POSIX uses `std::process::Command::new("julie-daemon").arg("start").stdout(Stdio::null()).stderr(Stdio::piped()).spawn()` + `setsid` (via `nix::unistd::setsid` in a `pre_exec` hook). On Windows: `CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS` flags. Inherit nothing.

**Acceptance criteria:**
- [ ] End-to-end test: invoke `julie-server` (no args) on a clean `JULIE_HOME` — spawns daemon successfully, MCP request round-trips.
- [ ] End-to-end test: invoke `julie-adapter` directly — same.
- [ ] Test: `julie-adapter` against an already-running `julie-daemon` connects without spawning a new one.
- [ ] Test: daemon survives adapter exit (detached). Adapter exits cleanly when daemon dies.
- [ ] Implementation tier.

### Task A1.9: Plugin manifest update

**Files:**
- Modify (in `~/source/julie-plugin`): `plugin.json` and related manifest to spawn `julie-adapter` instead of `julie-server`. Update `bin/archives/*.tar.gz` build job to include both binaries.

**What to build:** Update the plugin's release manifest. This lands as a separate PR in `julie-plugin`.

**Approach:** Mechanical. Coordinate the plugin update so the release pulls the new julie release binaries. After this PR + the next julie release ships, the plugin auto-installs the new binaries on update.

**Acceptance criteria:**
- [ ] Plugin manifest points at `julie-adapter` as the MCP launch binary.
- [ ] Built plugin archive contains both `julie-adapter` and `julie-daemon`.
- [ ] Smoke test in Claude Code: install updated plugin, plugin spawns `julie-adapter`, daemon starts, MCP tools work.
- [ ] Mechanical tier eligible (no test ownership) but coordinated by lead.

---

## Phase A.2 — Connection-per-Request

### Task A2.1: `WorkspaceConnectionPool`

**Files:**
- Create: `src/daemon/connection_pool.rs`
- Test: `src/tests/daemon/connection_pool_test.rs`

**What to build:** Per-workspace pool of `rusqlite::Connection`s. `pool.acquire(workspace_id).await -> PooledConn` returns a guard that returns the connection on Drop. Min size = 2 per workspace, max = `min(cpus * 2, 16)`. Idle eviction every 60s.

**Approach:** Simple `Mutex<Vec<Connection>>` + `Notify` for waiters works. If contention shows up in load tests, swap for `deadpool` or `bb8` (don't add a dep speculatively). Connection settings: same PRAGMAs as today's `WorkspacePool` connections (WAL mode, foreign keys on, busy_timeout). Pure-function tests for acquire/release semantics.

**Acceptance criteria:**
- [ ] Failing test: pool max=2, three concurrent acquires; third one waits for one of the first two to drop. Passes after implementation.
- [ ] Failing test: acquired connection survives across `await` points (verifying the guard is `Send`).
- [ ] Idle eviction test (driven by manual time advance, not `sleep`).
- [ ] `cargo nextest run --lib test_workspace_connection_pool` passes.
- [ ] Implementation tier.

### Task A2.2: Wire connection pool into MCP handlers

**Files:**
- Modify: `src/daemon/workspace_pool.rs` — `WorkspacePool` constructs and owns a `WorkspaceConnectionPool` per workspace. The connection it currently issues becomes a pool reference instead.
- Modify: `src/handler.rs` — each tool handler acquires from the pool at entry.
- Modify: `src/handler/tools/**` — same.
- Modify: mutating tools (`manage_workspace` index/refresh/register, editing tools) — acquire `mutation_gate::acquire_gate(workspace_id)` before opening the connection. Connection's transactions become per-request `BEGIN DEFERRED` for reads, normal for writes.

**What to build:** The conversion. Each handler entry point becomes:

```rust
let _gate = mutation_gate::acquire_gate(workspace_id).await;  // mutating handlers only
let conn = ctx.connection_pool(workspace_id).acquire().await?;
// ... use conn as today's shared connection was used ...
```

**Approach:** Mechanical-with-care. Run `fast_refs` on `WorkspacePool::shared_connection` (or whatever the current shared-connection API is) to find every handler. Each one becomes a `pool.acquire()` call. Read handlers do not take the mutation gate.

**Acceptance criteria:**
- [ ] All references to the old shared connection are gone; only the pool is used.
- [ ] Existing handler tests still pass.
- [ ] Worker scope: per-handler narrow test.
- [ ] Implementation tier per-file; **strategy tier reviews the diff** because mismatched gate acquisition is a subtle correctness bug.

### Task A2.3: Concurrent MCP regression test

**Files:**
- Create: `tests/integration/concurrent_mcp.rs`

**What to build:** Spawn an in-process daemon (using the harness from Phase B if it lands first — if not, use a real subprocess + parametrize). Fire 8 concurrent MCP requests on the same workspace: 4 reads (`fast_search`, `get_symbols`, `deep_dive`, `fast_refs`) and 4 writes (`edit_file` dry-run, `manage_workspace` refresh, `rename_symbol` dry-run, `edit_symbol` dry-run). Simultaneously, generate filewatcher events by writing to a file inside the workspace. Assert: all 8 requests complete within a bounded time (e.g., 30s budget) with no wedging.

**Approach:** The point is to catch deadlocks. Use `tokio::join!` for the 8 requests; if any single one takes more than 30s, the test fails. Filewatcher events are a separate `tokio::spawn` that writes files for the test duration. Connection-per-request + mutation_gate is the only thing serializing writers, so if there's a lock-order bug it'll surface here.

**Acceptance criteria:**
- [ ] Test passes consistently in 10 consecutive runs (no flakes — flakes here mean a deadlock surface still exists).
- [ ] If flakes happen, **escalation tier triages** before retry.
- [ ] **Escalation tier owns** this test's design and acceptance.

---

## Phase B — In-Process Test Harness

### Task B.1: `DaemonRuntimeContext` + `Registry::new()` for mutation_gate

**Files:**
- Modify: `src/workspace/mutation_gate.rs` — add `pub struct Registry`, `Registry::new() -> Self`, `Registry::acquire(&self, workspace_id: &str) -> MutationGuard<'_>`. Keep the existing module-level `acquire_gate(workspace_id)` as a thin wrapper around a global `Registry::global()`.
- Modify: `src/daemon/app.rs::DaemonRuntimeContext` — add `mutation_gate_registry: Arc<Registry>`, `tracing_handle: Option<TracingHandle>`, `env_overrides: HashMap<String,String>`, `workspace_id_namespace: Option<String>`, `watcher_quota: WatcherQuota`. Plus `for_test()` constructor.

**What to build:** Make the per-workspace gate cache injectable. Production uses `Registry::global()`; tests construct an isolated `Registry`. Production code must always reach gates via the runtime context, not via the module-level singleton — that means every existing call site of `mutation_gate::acquire_gate(workspace_id)` becomes `ctx.runtime.mutation_gate.acquire(workspace_id).await` or similar plumbing.

**Approach:** Add the Registry type without breaking existing callers. Phase 1 in this task: introduce `Registry::new()` and `Registry::global()`, change module-level `acquire_gate` to call `Registry::global().acquire(...)`. Phase 2: thread the registry through `DaemonRuntimeContext` for tests.

**Acceptance criteria:**
- [ ] `Registry::new()` produces an isolated cache; two registries do not see each other's locks.
- [ ] Production `acquire_gate` behavior unchanged (existing tests pass).
- [ ] `clear_cache_for_test()` becomes deprecated but stays for one release.
- [ ] Implementation tier.

### Task B.2: Tracing init becomes idempotent

**Files:**
- Modify: `src/daemon/app.rs::DaemonRuntimeContext` — `install_tracing()` method that is idempotent (no-op on repeated calls within a process).

**What to build:** Replace today's `tracing_subscriber::registry().init()` calls (which panic on second call) with a centralized idempotent installer using `OnceLock` or `tracing_subscriber::registry().try_init()`.

**Acceptance criteria:**
- [ ] Calling `install_tracing()` twice in the same process does not panic.
- [ ] Tracing still works in production (smoke check daemon log).
- [ ] Implementation tier.

### Task B.3: `InProcessDaemon` test fixture + migration of existing tests

**Files:**
- Create: `src/tests/harness/mod.rs`
- Create: `src/tests/harness/in_process.rs`
- Modify: every test in `src/tests/daemon/` and `src/tests/integration/daemon_lifecycle.rs` that spawns `julie-server` via `tokio::process::Command` — migrate to `InProcessDaemon`.
- Modify: keep a small set of adapter-integration tests (3-5) that exercise real stdio↔HTTP with a real subprocess.

**What to build:** The fixture per design B.1. After this task, `cargo xtask test dev` (which currently spawns many daemon subprocesses) runs them in-process and gets measurably faster.

**Approach:** The fixture builder takes `DaemonConfig` overrides as named parameters with sensible test defaults: `embedding_provider: Disabled`, `no_dashboard: true`, `watcher_quota: 4`, `runtime: DaemonRuntimeContext::for_test()`. Returns `Handle { url, token, daemon_handle }` plus a teardown closure.

Migration: for each existing test, replace subprocess setup with `InProcessDaemon::start(builder).await`, replace HTTP URL construction with `handle.url`, replace teardown with `handle.shutdown().await`. The actual test logic stays the same.

**Acceptance criteria:**
- [ ] Number of `tokio::process::Command::new("julie-server"|"julie-adapter"|"julie-daemon")` call sites in tests drops to ≤5 (adapter-integration suite).
- [ ] Measured: `cargo xtask test dev` runtime drops compared to a baseline ledger row captured before this task starts. Record both rows in the verification ledger.
- [ ] Parallelism check (does not use xtask's flag surface — uses nextest directly with the migrated tests): `cargo nextest run --lib --test-threads=8 daemon_lifecycle 2>&1 | tail -20` passes without `serial_test` annotations on the migrated tests. (xtask does not expose a `--test-threads` flag; verify directly via nextest. If julie's xtask later grows one, prefer it.)
- [ ] Implementation tier per file; **strategy tier reviews** because subtle global state leakage may surface only under parallelism.

---

## Phase C — Search Reranker

### Task C.1: `parse_query` + `ParsedQuery` type

**Files:**
- Create: `src/search/query_parse.rs`
- Test: pure-function unit tests in same file

**What to build:** `parse_query(raw: &str) -> ParsedQuery` per design C.2. `QueryIntent::{Free, Symbol(SymbolKind), Test}`. Symbol kinds parsed from the first token: `function|method|class|struct|trait|interface|type|enum`. Test intent: first token is `test` AND total terms ≥ 3.

**Approach:** Pure function, no dependencies. Heavy unit tests covering edge cases including: `"test"` alone (term count < 3, falls back to Free), `"test foo"` (count < 3, falls back), `"test foo bar"` (Test intent), `"function fooBar baz"` (Symbol intent), `"hello world"` (Free), case insensitivity, leading whitespace, tokens with hyphens.

**Acceptance criteria:**
- [ ] 15+ test cases covering Free / Symbol / Test intent classification.
- [ ] Worker test: `cargo nextest run --lib test_parse_query` passes.
- [ ] Implementation tier.

### Task C.2: Reranker scoring function

**Files:**
- Create: `src/search/reranker.rs`
- Test: pure-function unit tests in same file

**What to build:** `rerank(query: &ParsedQuery, candidates: &[Candidate]) -> Vec<Ranked>` per design C.3. Boost weights as specified. `Candidate` struct exposes fields the reranker needs: title, path, body excerpt, kind, role, test_role, is_test, is_file_doc, is_source_language, tantivy_score.

**Approach:** Pure function over candidates. Tests cover each boost path individually: exact title match adds 100, partial 50, path 40, body 10, phrase boost on long queries, intent-kind match adds 180 + 120 production bonus, etc. Use a `Candidate::builder()` for tests.

**Acceptance criteria:**
- [ ] Per-boost-path unit tests (one test per boost rule).
- [ ] Sort stability test: equal scores produce stable order by title then path.
- [ ] Worker test: `cargo nextest run --lib test_reranker_score` passes.
- [ ] Implementation tier.

### Task C.3: Enriched Tantivy schema

**Files:**
- Modify: `src/search/schema.rs` — add `role`, `test_role`, `capability_flags` as TEXT fields
- Modify: `src/search/index.rs` — populate these fields during symbol indexing
- Migration: existing indexes will have these fields absent; query path tolerates `Option<String>` lookup.

**What to build:** Three new optional text fields populated from existing `Symbol` metadata + path heuristics. Path heuristics already exist in `src/search/scoring.rs::is_test_path` — reuse, don't duplicate. `role` = `"test"` | `"source"` | `"docs"` | `"generated"` | `"vendor"` | `"unknown"`. `test_role` = `"unit"` | `"integration"` | `"smoke"` | empty.

**Approach:** Schema change adds optional fields — existing indexes keep working. On reindex (force or natural), new fields get populated. No forced migration.

**Acceptance criteria:**
- [ ] New fields appear in the Tantivy schema after this task.
- [ ] Indexing julie's own workspace populates the fields (manual check).
- [ ] Existing index opens cleanly without rebuild.
- [ ] Implementation tier.

### Task C.4: Wire reranker into search path, preserving target-specific defaults

**Files:**
- Modify: `src/tools/search/text_search.rs` — `definition_search_with_index` and `content_search_with_index` call `reranker::rerank()` after Tantivy candidate retrieval. Default `exclude_tests` semantics from CLAUDE.md preserved: definitions auto=false (include), content auto=NL-detection.
- Delete (or refactor): `filter_test_symbols` at line 214 — its job moves into the reranker's role-aware reweighting + the existing target-default policy.
- Create: `fixtures/search-quality/test-helper-discoverability.json` — regression queries that find test helper symbols via definition search.

**What to build:** The wiring. Reranker is gated behind a runtime flag (e.g. `JULIE_RERANKER_ENABLED=1` env or a config bool) during development. Once dogfood passes, flag flips to default-on, then flag removed in a follow-up release.

**Approach:** The hard part is preserving today's filtering defaults. Today's `text_search_impl` has different `exclude_tests` defaults based on `search_target`. Document the rule precisely in code comments at the call site. The reranker does not filter; it reweights. Tests must explicitly cover: `target="definitions"` with `exclude_tests=None` finds test helpers; `target="content"` with NL query auto-excludes; explicit `exclude_tests=false` includes; explicit `exclude_tests=true` excludes.

**Acceptance criteria:**
- [ ] Regression fixture `fixtures/search-quality/test-helper-discoverability.json` has ≥10 queries proving definition search still finds test helpers.
- [ ] `cargo xtask test dogfood` passes on both the existing search-quality fixture and the new test-helper fixture, with reranker enabled.
- [ ] No regression on `top5_hits` or `mrr` per category vs. baseline.
- [ ] **Escalation tier owns** — this is codex finding #7 surface and the regression is silent.

### Task C.5: Flip reranker default-on after dogfood passes

**Files:**
- Modify: `src/tools/search/text_search.rs` — default flag value changes to enabled.
- Modify: `xtask` if reranker flag plumbed there.

**What to build:** The flag flip. Only after a clean dogfood + test-helper-discoverability pass on a release tag.

**Acceptance criteria:**
- [ ] Reranker is on by default; dogfood and discoverability fixtures still pass.
- [ ] Mechanical tier eligible (flag flip) **after** escalation tier signs off on the regression evidence.

---

## Dispatch Order Summary

The lead orchestrates phases sequentially; tasks within a phase may parallelize where independent.

**Phase A.1:** A1.1 → A1.2 → A1.3 + A1.4 (parallel after A1.2) → A1.5 → A1.6 → A1.7 → A1.8 → A1.9.

> **Bridge (runtime-context prep):** B.1 (`Registry::new()` + `DaemonRuntimeContext` plumbing) **MUST run before A2.2**. A2.2 wires handlers to whatever gate API exists when it runs; if B.1 hasn't landed, handlers get wired to the global singleton and have to be rewritten in B.1. Land B.1 between A.1 and A.2.

**Phase A.2:** A2.1 → A2.2 → A2.3.
**Phase B (remaining):** B.2 → B.3.
**Phase C:** C.1 + C.2 + C.3 (parallel — touch different files) → C.4 → C.5.

Between phases, the lead runs `cargo xtask test dev` once and records a ledger row. **If `cargo xtask test dev` fails between phases, the lead does not advance to the next phase — fix the regression first or roll back the phase boundary commit.** After Phase C completes, run `cargo xtask test full` as the broad pre-merge confidence gate.

After all phases: razorback:finishing-a-development-branch decides integration; if a pre-merge external reviewer was requested at run-start, razorback:pre-merge-review runs that.

---

## Revision History

**2026-05-16 (v2)** — incorporated codex adversarial plan review (`gpt-5.5` high reasoning). Five findings addressed:

| # | Severity | Finding | Resolution |
|---|----------|---------|------------|
| 1 | high | A1.2 + A1.3 parallel but both own `src/daemon/discovery.rs` | Dispatch order sequences A1.2 before A1.3; A1.3 explicitly notes it extends the file A1.2 created |
| 2 | high | Integration tests planned at `tests/integration/*` but julie convention is `src/tests/**`; `--test-threads=8` not a supported xtask flag | All new integration tests relocated under `src/tests/integration/`; verification commands corrected to `cargo nextest run --lib <test_name>`; B.3 parallelism criterion changed to use nextest directly |
| 3 | high | A2.2 handler rewrite ordered before B.1 runtime-context gate it depends on | Bridge step added between A.1 and A.2: B.1 must run before A2.2 |
| 4 | medium | A1.6 creates `DaemonApp` with no test exercising the new interface | A1.6 gains a narrow RED/GREEN acceptance criterion for `DaemonApp::serve` + `DaemonHandle::shutdown` |
| 5 | high | Worker eligibility under-routes shared-invariant tasks (lifecycle, concurrency, search ranking) | Worker-eligibility section updated: A1.7, A2.2, C.1/C.2/C.3 promoted to coupled-implementation tier with strategy-tier-owned interface contracts before edits |

**2026-05-16 (v1)** — initial plan.
