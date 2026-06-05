# Julie Rescue Phase 3c — In-Process Server + Leader Election (Cutover) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Flip the no-args `julie-server` path from "fork+exec a daemon and bridge stdio↔HTTP" to an **in-process MCP server** that serves the same `JulieServerHandler` directly over rmcp stdio, with a **per-workspace OS leader-election lock** (winner = sole watcher + Tantivy writer + the 8 canonical writers; losers = SQLite-WAL + Tantivy-mmap readers that embed via the 3b resident host).

**Architecture:** Additive-first / flip-last. Land the leader-lock primitive, the handler-level hang deadline, the leadership-gated watcher/writers, the host-backed embedding wiring, the in-process serve entry, and handoff recovery — all behind a still-green build with the daemon path still live — THEN flip `main.rs`'s `None` arm in one small, revertible commit. The daemon + adapter still compile and exist; they are **bypassed, not deleted** (deletion is Phase 3d).

**Tech Stack:** Rust; rmcp `ServiceExt`/`serve(stdio())`; fs2 advisory file lock (harvested `DaemonLockGuard`); existing Tantivy single-writer OS lock; SQLite WAL; the merged 3a (cross-process reload proof + projection reconciliation) and 3b (resident embedding-host RPC client) work, reused as-is.

**Architecture Quality:** Approved shape (design doc `docs/plans/2026-06-04-julie-phase3-daemon-teardown-design.md` §1–3, owner-approved 2026-06-04): in-process `serve(stdio())` + per-workspace leader lock; losers are pure readers; dashboard → standalone `registry.db` reader (Option B, built in 3d). Main architecture risk: the cutover (T10) has zero in-process-serve prod precedent and the per-process mutation gate gives **no** cross-process write serialization — correctness rests entirely on the leader lock + Tantivy's single-writer OS lock. If code reality contradicts this shape, the worker reports a plan mismatch rather than redesigning locally.

---

## Decisions (defaults — owner may override any at approval)

- **D1 — Loser write-tools: REFUSE (owner-confirmed 2026-06-05).** A non-leader session returns a clear "another session owns writes" error for `edit_file` / `rename_symbol` / `rewrite_symbol` / `manage_workspace(index|register|refresh)`. Read tools (search/refs/symbols/context/deep_dive/call_path) work fully on losers. Native Edit/Write still work; the leader's watcher re-indexes those changes within ~500ms. Forward-to-leader IPC is explicitly **out of 3c scope**.
- **D2 — Embedding default: DEFAULT-ON via the 3b host.** The in-process path sources embeddings from `connect_or_spawn_host` → `RpcEmbeddingProvider` (one shared CodeRankEmbed in VRAM for N sessions), gated by the F2 `ensure_ready()` hard-gate. Host-down degrades gracefully to keyword-only **without hanging** startup (cold host spawn up to 180s must not block the session). `JULIE_EMBEDDING_PROVIDER=none` still force-disables.
- **D3 — Loser tool-call metrics: DROP.** `record_tool_call_outcome`'s per-call SQLite write does not run on losers (in-memory `session_metrics` only). Persistent metrics belong to the leader; `registry.db` consolidation is 3d.
- **D4 — Multi-workspace targeting: reads preserved, writes follow the leader rule.** A session may open another workspace's index **read-only** (no leadership needed to read). Cross-workspace *writes* only succeed if this process leads that workspace. No "one process holds N leader locks for writing" machinery in 3c.
- **D5 — Handoff recovery: FULL `ensure_current_from_database` rebuild on promotion** (design §6 default). Durable `tantivy_dirty` incremental table is deferred.
- **D6 — Dashboard: DESIGN-ONLY in 3c.** T13 produces the persist-vs-drop signal classification; the standalone `registry.db` reader server and mutation-route decisions are 3d.
- **D7 — Index/lock location: ONE canonical per-workspace path, deterministic via `generate_workspace_id`.** T1's D7 finding (shipped): the in-process server replaces the **adapter** (not stdio), so it uses the **shared `~/.julie/indexes`** layout — derive index dir + lock from ONE `DaemonPaths` so they share an inode. **⚠ Elevated by codex 3c.1 F2 (HIGH):** the *current* `new_in_process` wiring (daemon_db/workspace_pool=None) actually falls through to **project-local** `.julie/indexes` storage while the lock is `DaemonPaths`-based — so as-built they do NOT agree. T8 owns the reconciliation (thread a shared `index_root` override so storage + lock share `{indexes}/{workspace_id}`; create the dir before lock acquisition; test the shared parent). This is no longer "documented intent" — it is a hard T8 acceptance gate.

---

## PR decomposition (3 human-merge-gated sub-PRs)

The §9 design sketch listed 3c as one PR; the blast radius (process model + leader election + serve seam + dashboard classification) is too large for one safe review. Split, mirroring Phase 2's 2a/2b/2c cadence:

- **PR 3c.1 — Leader-lock + hang-guard infra (additive, zero runtime-path change).** Tasks **T1, T2, T3, T4, T13**. Pure additions + a refactor-move + a design table. The no-args runtime path is untouched → merges with zero behavioral risk and de-risks the rest. Branch `julie-rescue-phase3c` (this branch) or `julie-rescue-phase3c-1`.
- **PR 3c.2 — In-process wiring (still additive — entry exists, `main.rs` NOT flipped).** Tasks **T5, T6, T7, T8, T9**. The `serve_in_process` module, leader-gated watcher/writers, host embeddings, and handoff recovery land and are tested in isolation while the daemon path is still the live one. A regression here cannot break shipping users.
- **PR 3c.3 — The cutover (one risky, minimal merge).** Tasks **T10** (flip `main.rs` None arm), **T11** (acceptance HARD GATE), **T12** (boundary tripwire). Small diff, high consequence, revertible in one commit if the live-session smoke fails.

> Execution proceeds one sub-PR at a time, each human-merge-gated, each with its own branch-gate-green + verification-ledger evidence, exactly like 2a→2b→2c. This plan covers all three; we execute 3c.1 first.

---

## Verification Strategy

**Project source of truth:** `CLAUDE.md` (xtask test tiers) + `docs/TESTING_GUIDE.md` + `docs/plans/verification-ledger-template.md`.

**Worker red/green scope:** `cargo nextest run -p <crate> <exact_test_name>` — narrow, named, ≤2 runs (RED→GREEN). Crate routing: T1 → `julie-core`; T2 → `julie-core`/`julie` (re-export); T11 kill-writer half → `julie-index`; T3/T4/T5/T6/T7/T8/T9/T10/T11-recovery/T12 → `julie` (top crate — handler/main/startup/tools); T5/T6 watcher-gate also touches `julie-runtime`.

**Worker ceiling:** the exact named test(s) above. Workers do NOT run `cargo xtask` tiers or unfiltered `cargo nextest`.

**Worker gate invariant:** each task's acceptance line states the behavior its test proves (e.g. T5: "exactly one of two in-process handlers starts a watcher; the loser never acquires the Tantivy writer lock").

**Lead affected-change scope:** `cargo xtask test changed` after a coherent batch. Because T5/T8/T10 move shared infrastructure, `changed` will likely fall back to `dev` — accept it.

**Branch gate (lead, once per sub-PR before handoff):** `cargo xtask test dev` GREEN **with the daemon/adapter tests still present** — the in-process path must not regress `src/tests/daemon/**` or `src/tests/adapter/**`. For the 3c.2 and 3c.3 batches add `cargo xtask test system` (startup/workspace/serve) and `cargo xtask test reliability` (leader handoff = lifecycle).

**Acceptance HARD GATE (3c.3):** **T11** — a two-process kill-the-writer / new-leader-recovers test (reuse the `current_exe()` subprocess pattern from `tantivy_cross_process_reload_test.rs`): kill the leader → freshness degrades only (~500ms eventual consistency, never an error) → a fresh process wins the lock and reconciles Tantivy to canonical. Analogous to 3b's 3-host race. PLUS a manual dogfood smoke: run `julie-server` no-args against Julie itself, confirm via `ps`/absence of `discovery.json` that no daemon forked, and that a live `fast_search` + `edit_file` + re-search round-trip works.

**Replay/metric evidence:** N/A (no replay/metric gates).

**Escalation triggers:** any cutover task (T5, T6, T8, T9, T10, T11) failing in a way that implicates the process model, leader election, or the serve seam → lead investigates, do not hand to a mechanical worker.

**Assigned verification failure:** workers stop and report; a failing assigned gate is not acceptance evidence.

**Verification ledger:** record invariant, command, scope label, commit SHA, result, timestamp per `docs/plans/verification-ledger-template.md`. Reuse a passing entry only when scope label + HEAD SHA match exactly.

---

## Model Routing

**Project source of truth:** repo convention (no `RAZORBACK.md`) — Opus lead + 3 Sonnet teammates (per project memory).

- **Strategy tier (Lead, Opus):** this plan, decomposition, inline review, finding triage, the cutover seam design.
- **Implementation tier (Worker, Sonnet):** bounded tasks with clear acceptance + narrow ownership — T1, T2, T3, T4, T7, T12, T13.
- **Escalation tier (Lead, Opus — implement directly or Sonnet + MANDATORY intensive lead review):** high-blast-radius tasks — **T5, T6, T8, T9, T10, T11**. These touch the process model / leader election / serve seam / handoff. The lead owns correctness here; if delegated to Sonnet, the lead reviews every line and re-runs the acceptance gate personally.
- **Mechanical tier:** none (T13 is design/classification but requires judgment → implementation tier).
- **Pre-merge external reviewer:** **codex** (as on 3b — high value caught 4 real findings). Run `razorback:pre-merge-review` before each sub-PR's `finishing-a-development-branch`.

**Worker eligibility:** implementation-tier workers only for T1/T2/T3/T4/T7/T12/T13 (clear acceptance, narrow ownership, local change). **Escalation tasks are not handed to unattended workers.** `src/handler.rs` is touched by T3/T4/T5/T7/T9 → **serialize** those, never parallelize across workers (file-collision + shared-invariant risk).

---

## Tasks

> File:line refs below come from the code-grounded Phase 3c map (workflow `wf_4ffab9be`, 7 agents, 2026-06-05). Implementers MUST re-verify each ref with Julie's code-intel tools (`get_symbols`/`deep_dive`/`fast_refs`) before editing — refs may have shifted.

### PR 3c.1 — Leader-lock + hang-guard infra

#### T1 — Per-workspace leader-lock path helper  · `julie-core` · mechanical
**Files:** Create helper in `crates/julie-core/src/paths.rs` (sibling of `workspace_index_dir`/db/tantivy at ~`:350-364`, mirroring `daemon_lock` at ~`:393`). Test: `crates/julie-core/src/tests/paths.rs`.
**What:** Add `workspace_leader_lock(workspace_id) -> .../indexes/{workspace_id}/leader.lock`. Ensure the parent index dir is created before acquisition (mirror `recreate_index_with_lock`'s create-if-needed). It MUST be a DISTINCT file from any `{dirname}.julie-rebuild.lock` so the non-blocking leader lock never aliases the blocking rebuild lock. **Resolve D7 here:** read how the current standalone/stdio path picks `indexes_dir` (project-local `.julie/indexes` vs shared `~/.julie/indexes`) and document the single canonical choice in the task report, so T8's `generate_workspace_id`-derived lock and index dir agree on the same inode.
**Acceptance:** `cargo nextest run -p julie-core paths` green; helper returns `indexes/{ws}/leader.lock`, sibling of `db/`+`tantivy/` (survives a tantivy-dir rebuild). Report states the canonical index/lock location decision.

#### T2 — Generalize `DaemonLockGuard` into a workspace-neutral leader-lock primitive  · `julie-core`/`julie` · mechanical
**Files:** `src/daemon/discovery.rs` (lift from ~`:53-160`), `crates/julie-core/src/workspace/mod.rs` (new home), `src/lib.rs` (re-export).
**What:** Move `DaemonLockGuard` + `try_acquire` + `Drop` + `AcquireError`/`LockAlreadyHeld` + `HELD_DAEMON_LOCKS` + `normalize_lock_path` into `julie-core` (depends only on fs2+std, sits below `julie-runtime`). **Re-export from the old path** so the 3 existing `daemon_lock()` callers (`app.rs:143`, `app/helpers.rs:96`, `legacy_migration.rs`) compile unchanged. Leave `DiscoveryRecord`/`DiscoveryFile` behind (dies in 3d). **NO behavior change.**
**Acceptance:** Guard usable from a non-daemon module; existing acquire sites unchanged; the existing discovery-lock tests stay green; full build compiles. `cargo nextest run -p julie <discovery lock test>`.

#### T3 — Handler-level per-request deadline (§5b in-process hang guard)  · `julie` · implementation
**Files:** `src/handler.rs` (tool dispatch at ~`:2407` / `tool_router.call`).
**What:** Wrap in-process tool dispatch in a `tokio::time::timeout` (configurable env/const) that synthesizes a JSON-RPC error for the in-flight id on expiry — the in-process replacement for the absent adapter `forwarder.rs:166` receive-deadline. Do NOT touch the forwarder (3d deletes it). **CRITICAL:** scope the timeout so it never aborts an in-flight canonical SQLite/Tantivy write mid-transaction — apply to read/query tools, or make writers cancellation-safe. (§5a embed-before-lock already shipped in 3a.)
**Acceptance:** New test installs a deliberately stalling tool and asserts the handler returns a JSON-RPC timeout error within the deadline instead of hanging (companion to `crates/julie-tools/.../lock_free_embed.rs`). `cargo nextest run -p julie <deadline test>`.

#### T4 — Leadership state on the handler + startup-hint-carrying in-process constructor  · `julie` · implementation · depends T2
**Files:** `src/handler.rs` (new ctor; refs at `:685` `new()`, `:700-726` deps-None, `:2440-2453` on_initialized cwd-deferral, `:1288` embedding_provider()).
**What:** Add a NEW `JulieServerHandler::new_in_process(startup_hint, embedding_provider: Option<Arc<dyn EmbeddingProvider>>, leader: LeadershipState)` (do NOT extend `new()` — dozens of tests use it). It must: preserve `WorkspaceStartupHint.source` (so `on_initialized` cwd-deferral still fires — `new()` drops source to None); keep `MutationGateRegistry::global()` + all daemon deps None; carry `Option<DaemonLockGuard>`/`is_leader()`; surface the injected provider via `embedding_provider()`. Do NOT call `mark_standalone_embedding_skipped`.
**Acceptance:** Handler from a Cwd-source hint defers auto-index in `on_initialized`; Cli-source indexes eagerly; `is_leader()` reflects the injected lock; injected provider returned by `embedding_provider()`. `cargo nextest run -p julie <ctor test>`.

#### T13 — Dashboard live-signal persist-vs-drop classification (design-only)  · `julie` · implementation · depends —
**Files:** `docs/plans/2026-06-04-julie-phase3-daemon-teardown-design.md` (append table), read-only refs `src/dashboard/state.rs:128`, `src/daemon/app.rs`.
**What:** Enumerate every live in-process signal `DashboardState` consumes (SessionTracker active-count/phase, lifecycle `LifecyclePhase`/`ShutdownCause`/`restart_pending`, `RecoveryMarker`, WatcherPool/WorkspacePool `IndexingRuntimeSnapshot`, `ErrorBuffer`, SSE broadcast) and classify each **PERSIST-to-`registry.db`** (with a write-site) or **DROP**, with rationale. Producer-side classification ONLY — no standalone server (that's 3d). Pre-classified by the maps: PERSIST = per-workspace session_count, workspace rows, tool_calls, RecoveryMarker (durable file), projection freshness (durable in symbols.db); DROP = live per-session phase, live daemon LifecyclePhase/restart_pending, live indexing-activity health, SSE stream, error buffer, uptime; mutation routes (register/open/refresh/delete, search_compare) = DROP from a pure reader.
**Acceptance:** A documented persist-vs-drop table (signal → persist|drop + rationale + write-site for persisted) sufficient for 3d to build the standalone reader without re-deriving it. No behavior change.

### PR 3c.2 — In-process wiring (additive; main.rs NOT flipped)

#### T5 — Gate watcher start + the 8 canonical writers behind leadership  · `julie` + `julie-runtime` · ESCALATION · depends T4
**Files:** `src/handler.rs` (restore-path start ~`:112-113`, primary-swap start ~`:1536`), `crates/julie-runtime/src/workspace/mod.rs` (`start_file_watching` ~`:732`).
**What:** Make watcher start conditional on `is_leader()`: the inline restore-path start (`initialize_file_watcher` + `start_file_watching`) and the primary-swap start only fire for the leader; losers never construct an `IncrementalIndexer`. Thread the leader flag into `JulieWorkspace::start_file_watching`. Audit for double-start (`initialize_file_watcher` is idempotent but `start_watching` is not). Structural gating covers the 4 watcher writers; the other 4 (catch-up, force-index, refresh, register) are gated at their entry in T7.
**Acceptance:** Two in-process handlers on one workspace: exactly one starts a watcher (leader `is_running` true, loser watcher None); loser searches succeed with no watcher and the loser never acquires the Tantivy writer lock. `cargo nextest run -p julie <leader watcher test>`.

#### T6 — In-process embedding routes through the 3b host (ensure_ready gate)  · `julie` · ESCALATION · depends T4
**Files:** Create `src/server_in_process.rs`; `src/embedding_host_launch.rs` (`connect_or_spawn_host` ~`:41`); `src/handler.rs`.
**What:** Add an in-process startup path that sources embeddings from `connect_or_spawn_host(&paths)` → `RpcEmbeddingProvider` and injects it into the T4 ctor (NOT per-workspace `create_embedding_provider()`, which would spawn one CodeRankEmbed per session → OOM). Replicate the daemon's `helpers.rs:429` hard-gate: gate Ready on `RpcEmbeddingProvider::ensure_ready()` (never the silent-default getters); host-down degrades to keyword-only/lazy WITHOUT hanging. Keep the blocking host connect off the runtime thread (`spawn_blocking`). **D2: default-on** (env can force-disable).
**Acceptance:** Three concurrent in-process sessions on one workspace produce exactly ONE resident sidecar (one model in VRAM); each `embed_query` round-trips to the shared host; host-connects-but-unhealthy is surfaced unavailable (not silently degraded); host-down does not hang startup. `cargo nextest run -p julie <shared host test>`.

#### T7 — Refuse write-tools on loser (reader) processes  · `julie` · implementation · depends T5
**Files:** `src/tools/workspace/commands/index.rs` (force=true ~`:173`), `.../registry/refresh_stats.rs` (~`:188`), `.../registry/register_remove.rs` (~`:15`), `src/handler/tool_metrics.rs` (`record_tool_call_outcome` ~`:222`), `src/handler.rs`.
**What (D1 + D3):** The 4 non-watcher writer entry points check `is_leader()` and return a clear non-panicking "another session owns writes" error on a loser. The editing tools (`edit_file`/`rename_symbol`/`rewrite_symbol`) are writers → same refusal on losers. Read tools work unchanged. Resolve the hidden write: `record_tool_call_outcome` pushes a per-call SQLite write on EVERY tool call (incl. read-only) — on a loser, **drop** it (in-memory `session_metrics` only); persistent metrics are the leader's.
**Acceptance:** On a loser, `manage_workspace(operation=index, force=true)` and `edit_file` return a graceful error; a read-only `fast_search` performs NO write to the leader-owned metrics DB (no `SQLITE_BUSY`); search still returns results. `cargo nextest run -p julie <loser refusal test>`.

#### T8 — In-process serve entry: `run_in_process_server(startup_hint)`  · `julie` · ESCALATION · depends T3, T6, T7
**Files:** `src/server_in_process.rs`, `src/lib.rs`, `src/cli_tools/mod.rs` (reuse `bootstrap_standalone_handler` build sequence ~`:273`).
**What:** New module exposing `run_in_process_server(startup_hint)`: resolve `workspace_id` via `generate_workspace_id(startup_hint.path)` (matching `mcp_session.rs:206` so lock + index dir agree), `try_acquire` the T1 leader lock (win→leader, AlreadyHeld→loser — distinguish cross-process from in-process AlreadyHeld), build the T4 handler with the T6 embedding provider, then `handler.serve(stdio()).await?.waiting().await?` via `rmcp::{ServiceExt, transport::stdio}`. Driven by `on_initialized` (NOT a forced index) so cwd-deferral + client-roots resolution work. No fork, no HTTP, no `discovery.json`, no header round-trip.
**⚠ Codex 3c.1 F2 (HIGH) — storage+lock layout MUST be coupled, not just `workspace_id`:** `new_in_process` (daemon_db/workspace_pool=None) currently falls through to **project-local** `.julie/indexes` storage (`JulieWorkspace::detect_and_load`), while the T1 leader lock is `DaemonPaths`(`~/.julie/indexes`)-based — so the lock and the index would live in **different directories**, the exact inode mismatch D7/Risk #6 warns of (two processes with different `JULIE_HOME` could hold different locks while racing the same project-local index). T1's D7 chose shared `~/.julie/indexes` for the in-process path (it replaces the *adapter*, not stdio). T8 MUST make the opened index dir and the leader lock share **ONE base**: thread a shared `index_root` override into `new_in_process`/`initialize_workspace_with_force` so storage lands under the same `{indexes}/{workspace_id}` as the lock. Create that index dir BEFORE lock acquisition. ONE resolver returns lock+db+tantivy from the same base — no second path.
**Acceptance:** Integration test (build on `src/tests/harness/in_process.rs`) serves a full MCP session over stdio against a real workspace: initialize + on_initialized auto-index + a `fast_search` succeed end-to-end with NO daemon process; cwd-source deferral triggers. **PLUS (codex F2):** a test asserts `leader.lock`, `db/`, and `tantivy/` resolve to the SAME `{indexes}/{workspace_id}` parent, and two in-process handlers on the same workspace path contend on the SAME lock inode (Risk #6). `cargo nextest run -p julie <serve in_process test>`.

#### T9 — Handoff recovery on leader acquisition (reuse existing machinery)  · `julie` · ESCALATION · depends T8
**Files:** `src/server_in_process.rs`, `src/handler.rs`, `src/startup.rs` (`run_primary_workspace_repair` ~`:77`, `reconcile_projection_lag_if_needed` ~`:289` — the 3a addition).
**What:** On winning the leader lock (initial or after a prior leader's death), invoke existing catch-up `run_primary_workspace_repair` + `retry_persisted_repairs`/`retry_dirty_tantivy` + `reconcile_projection_lag_if_needed` → `ensure_current_from_database` (D5 full rebuild). Losers skip the writing variant entirely. **NO change to the recovery functions** — only call ordering/gating at the T8 startup site. Order matters: catch-up THEN projection-lag reconcile, or a SQLite-committed-but-Tantivy-failed file survives handoff.
**⚠ Codex 3c.1 F1 (HIGH) — the T3 per-request deadline only wraps `tool_router.call`, NOT the pre-dispatch `ensure_primary_workspace_for_request`** (verified: it calls `list_roots_from_peer` — an unbounded client round-trip — and `complete_deferred_auto_index_if_needed` — indexing/writes). So a Cwd-path FIRST read can still hang in resolution/repair, entirely outside the hang guard, which is the exact failure mode T3 exists to prevent and which the old adapter receive-deadline DID cover. T8/T9 MUST close this: classify read-vs-write BEFORE workspace resolution, and put a bounded, cancellation-safe envelope around the WHOLE read request — but run the deferred auto-index / handoff repair as a **non-cancellable background task** (or return a bounded `indexing-pending` error to the read), NEVER inside a cancellable read timeout (cancelling mid-index corrupts canonical/Tantivy state). Reuse this task's background-repair structure as that non-cancellable home.
**Acceptance:** Integration test (build on `src/tests/integration/projection_repair.rs`): writer commits SQLite but skips Tantivy (projected<canonical), process dies; a fresh leader acquires the lock and reconcile rebuilds Tantivy so a search returns the doc (projected==canonical). **PLUS (codex F1):** a test where deferred resolution/repair stalls before `tool_router.call` → the read returns a bounded error/indexing-pending within the deadline (NOT a hang), and the stalled repair is NOT cancelled mid-write. `cargo nextest run -p julie <handoff recovery test>`.

### PR 3c.3 — The cutover

#### T10 — Flip the `main.rs` None arm to in-process serve (THE CUTOVER)  · `julie` · ESCALATION · depends T9
**Files:** `src/main.rs` (None arm ~`:111-134`).
**What:** Replace ONLY the None arm body: swap `adapter.log` tracing for per-project `.julie/logs/julie.log` (confirm JULIE_HOME/indexes_dir per T1's D7 decision), and call `run_in_process_server(startup_hint)` instead of `julie::adapter::run_adapter`. Leave EVERY other match arm (Daemon/Stop/Status/Restart/Dashboard/tool subcommands) and the adapter/daemon modules untouched. Single, last, smallest seam-flip — everything it calls already landed green in T1–T9.
**Acceptance:** `julie-server` (no args) serves MCP over stdio; `ps`/no `discovery.json` confirms no `julie-daemon` fork; all other subcommands behave exactly as before; `cargo build` compiles all daemon/adapter modules; stdio smoke test passes.
**⚠ Codex 3c.2 F-A (HIGH) — leader election must key on the canonical bound root, not the raw startup hint (carried from PR 3c.2 review).** `run_in_process_server` derives `workspace_id` → `index_root` → `leader.lock` from `startup_hint.path` BEFORE the MCP client roots resolve. For a `Cwd`-source hint (the default once T10 flips the None arm), `ensure_primary_workspace_for_request` later rebinds the primary to the client's `list_roots` root (`handler.rs:596`), which can differ from the hint. The lock+storage stay hint-keyed while the binding moves → two sessions launched from different cwds (e.g. `/repo/sub` vs `/repo`) but reporting the same client root each win a *different* lock and maintain *divergent* index trees for the same logical workspace (split indexes + binding↔storage key mismatch). NOTE: this is NOT shared-file corruption — different hints yield different storage *and* lock, so no two writers touch one inode; codex's "SQLite/Tantivy corruption" wording is overstated. **T10 MUST:** do leader election AFTER the final canonical primary root is known (or re-elect / re-key when request-root reconciliation changes the primary), and assert lock path + `index_root` + initialized workspace ID all derive from the SAME canonical root before any watcher/writer starts. Add a regression test for the cwd-hint-vs-client-root mismatch. T11's kill-the-writer harness is the natural place to prove single-writer-per-canonical-workspace holds.

**RESOLVED design (impl in 3c.3) — PIN to canonical hint (not deferred re-election).** The design doc says each process *wins the OS lock at startup*, before client roots are known → workspace identity MUST be hint/cwd-derived and fixed; the `list_roots` rebind is daemon-era behavior that does not belong in-process. So the fix is to SUPPRESS the rebind for in-process, not to defer election (which would need dynamic leadership + ripple into T5/T7/T9). Three parts: (1) `run_in_process_server` canonicalizes `startup_hint.path` (via `JulieServerHandler::canonicalize_workspace_path`) BEFORE deriving `workspace_id`, and passes the canonicalized hint to `new_in_process` so lock id == index_root id == binding id (binding canonicalizes too). (2) `ensure_primary_workspace_for_request` gates on a new `request_prefers_client_roots()` = `!is_in_process() && startup_source_prefers_request_roots(source)` → in-process always binds to the canonical startup hint, never `list_roots`. Keep `Cwd` source so `on_initialized`'s deferral + the `complete_deferred_auto_index_if_needed` follower guard stay intact. (3) Defense-in-depth: `run_primary_workspace_repair` early-returns `Ok(None)` for `is_in_process_follower()` so a follower never writes through ANY entry path (the non-deferred `run_auto_indexing` path was previously unguarded). Tests: `request_prefers_client_roots` truth table; follower repair is a no-op; canonicalization couples lock id to binding id.

#### T11 — Kill-the-writer / new-leader-recovers acceptance test (3c HARD GATE)  · `julie-index` + `julie` · ESCALATION · depends T10
**Files:** `crates/julie-index/src/tests/search/tantivy_cross_process_reload_test.rs`, `src/tests/integration/projection_repair.rs`.
**What:** Two-process test (reuse the `current_exe()`-subprocess pattern): process A wins the leader lock, indexes a file, is SIGKILLed without clean release. Assert (a) the lock is kernel-released, (b) a second process wins it on next acquire, (c) a file changed during the gap is recovered by catch-up + reconcile, (d) surviving readers degrade to freshness-only (~500ms) but never error.
**Acceptance:** Test proves kill leader → freshness degrades only (no error); new leader wins the lock and reconciles to canonical. `cargo nextest run -p julie-index <kill-writer test>` + `-p julie <recovery test>`.

#### T12 — Boundary tripwire: no-args path bypasses (does not delete) the daemon  · `julie` · mechanical · depends T10
**Files:** `src/main.rs`, `src/lib.rs`, create `src/tests/integration/in_process_boundary.rs`.
**What:** Add an assertion (precedent: `crates/julie-context` `no_upward_deps`) that the no-args `main.rs` path does NOT call `run_adapter`/`DaemonLauncher`, while `src/daemon/**` and `src/adapter/**` still compile. A grep-scan confirms ZERO §7-DAG files were removed on the 3c branch (adapter/**, bin/julie-adapter.rs, daemon HTTP transport, singleton/legacy/pid, search_compare, migration.rs). Pins the 3c/3d boundary so a reviewer can prove "bypassed but present."
**Acceptance:** Test asserts the no-args path serves in-process and never enters `run_adapter`; `cargo build` compiles all daemon/adapter modules; grep-scan confirms no §7 files removed.

---

## Risks (ranked)

1. **[critical] Cutover (T10) regresses live MCP sessions** — in-process `serve(stdio())` has zero prod precedent; if rmcp 1.6 stdio doesn't deliver `initialized` to `on_initialized`, auto-index never fires; if the startup-hint-as-arg replacement of the `x-julie-workspace` header contract mis-binds the workspace, sessions open the wrong/empty index. *Mitigation:* land T1–T9 additively behind a green build; T8's serve test must exercise the FULL initialize → on_initialized → auto-index → fast_search round-trip BEFORE T10; verify `ServerInfo` carries `JULIE_AGENT_INSTRUCTIONS` against a live client; keep the daemon path reachable via the `julie-adapter` binary as an escape hatch until 3d.
2. **[critical] Losers accidentally write** — the mutation gate is per-process only (`Registry::global`) → ZERO cross-process serialization. A single missed `is_leader()` check (or the hidden `record_tool_call_outcome` write) races SQLite/Tantivy or collides on the Tantivy single-writer lock. *Mitigation:* T5 gates the 4 watcher writers structurally; T7 gates the other 4 + the metrics write; add a loser-side assertion that a read-only `fast_search` performs NO write; enumerate every writer call site once and assert each is unreachable on a loser. Cross-process exclusion rests on leader-lock + Tantivy's OS lock, not the gate — document it.
3. **[high] Leader-handoff correctness gap** — a SQLite-committed-but-Tantivy-failed file is hash-stable (catch-up skips it); only `reconcile_projection_lag` catches it via projected<canonical; the 3a per-save `projected_revision` stamp is the precondition. *Mitigation:* T9 runs BOTH catch-up AND reconcile, in that order; verify the 3a stamp ordering holds the §6 invariant; T11 is the hard-gate.
4. **[high] The known fast_search/deep_dive hang resurfaces** — §5b per-request deadline is genuinely new; in-process there's no transport loop to host a deadline. *Mitigation:* T3's handler-level timeout (shipped 3c.1: `dispatch_with_deadline`, default 120s, writers exempt) + stalling-tool test; the T6 host cold-spawn (≤180s) must not block startup (degrade lazily). **⚠ Codex 3c.1 F1 (HIGH) found the shipped T3 guard is too narrow** — it wraps only `tool_router.call`, leaving the pre-dispatch `ensure_primary_workspace_for_request` (peer roots round-trip + deferred auto-index) unbounded, so a Cwd-path first read can still hang. T8/T9 MUST extend the bounded envelope to the whole read request with repair pushed to a non-cancellable background task (see T9's F1 note). Until then the guard is partial.
5. **[medium] Thundering-herd handoff** — when the leader dies, N losers may all `try_acquire`. *Mitigation:* `try_acquire` is non-blocking + kernel-arbitrated (exactly one wins); make promotion idempotent; distinguish cross-process vs in-process AlreadyHeld; bounded loser re-try cadence (no busy spin).
6. **[high — raised from medium by codex 3c.1 F2] Per-workspace lock-path / storage-layout mismatch** — the lock helper is `DaemonPaths`(`~/.julie`)-based but `new_in_process` as-built stores project-local (`.julie/indexes`), so the lock and the index it guards live in different dirs (and two processes with different `JULIE_HOME` could lock different files while racing one index). *Mitigation:* T1 standardizes ONE canonical location (D7) + T8 derives `workspace_id` via the same `generate_workspace_id` **AND** threads a shared `index_root` override so storage + lock share `{indexes}/{workspace_id}` from one resolver (create dir before acquire); add tests that `leader.lock`/`db/`/`tantivy/` share a parent and that two processes with the same workspace path contend on the same inode.

---

## Open questions deferred to later phases (not blocking 3c)

- Forward-to-leader IPC for loser writes (3d or later — D1 chose refuse).
- Standalone `registry.db` dashboard reader + mutation control channel (3d — D6).
- Durable `tantivy_dirty` incremental-replay table vs full rebuild (revisit if workspaces are large — D5).
- `daemon.db` → `registry.db` consolidation + the §7 deletion DAG (3d).
