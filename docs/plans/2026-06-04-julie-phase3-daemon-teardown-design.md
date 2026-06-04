# Julie Rescue — Phase 3: Daemon Teardown (Design)

**Status:** APPROVED (owner, 2026-06-04) — architecture confirmed (in-process server + leader lock); dashboard → standalone `registry.db` reader (option B); 3a-first sequencing confirmed. Implementation underway.
**Branch:** `julie-rescue-phase3` (based on merged `main` @ `0f0d588b`, the #26 merge)
**Inputs:** the 8-subsystem daemon architecture map + completeness critique (workflow `wf_e99f97f7`, 9 agents, ~1.1M tokens, 2026-06-04). Maps archived under `/tmp/phase3map/`.
**Supersedes:** the Phase 3 sketch in `docs/plans/2026-06-03-julie-rescue-design.md` §Phase 3 (still accurate at the spec level; this doc is the code-grounded build plan).

---

## 1. The decision in one paragraph

Replace the always-on HTTP daemon + stdio adapter (~11.1k LOC: `src/daemon` 9.8k + `src/adapter` 1.3k) with an **in-process MCP server per session + an OS leader-election lock**. `julie-server` (no args) stops fork+exec'ing a daemon and bridging stdio↔HTTP; instead it serves the **same `JulieServerHandler`** directly over `rmcp` stdio. On startup each process tries to win one OS advisory file lock (we **harvest the existing `DaemonLockGuard`** — `src/daemon/discovery.rs:76`). The **winner** runs the sole file watcher, holds the single Tantivy `IndexWriter`, and performs all 8 canonical writes. **Losers are pure readers** over SQLite-WAL + Tantivy mmap. Exactly **one resident "embedding-host" process** owns the single PyTorch sidecar (because N sessions each loading CodeRankEmbed into VRAM would OOM); every other process embeds by RPC to it. Leader death → lock releases → a reader wins it and reconciles via Julie's existing catch-up + repair. The maps confirm this is the lowest-risk model because Tantivy's single-writer lock is a hard OS constraint that already forces "one writer process."

---

## 2. What the daemon actually does today (so we delete the right things)

| Subsystem | Files | Fate |
|---|---|---|
| stdio↔HTTP adapter | `adapter/{mod,launcher,forwarder,http_stdio}.rs`, `bin/julie-adapter.rs` | **DELETE** — only exists to bridge two processes |
| daemon HTTP server | `daemon/{http_transport,transport,http_client}.rs` | **DELETE** — in-process server has no HTTP |
| singleton / discovery / legacy | `daemon/{singleton,legacy_migration,pid,shutdown_event,token_file}.rs`, `src/migration.rs`, `paths.rs:daemon_singleton_lock` | **DELETE** (ordered — see §7) |
| **leader lock primitive** | `daemon/discovery.rs:DaemonLockGuard` (fs2 exclusive flock, kernel-released on death, Windows-mapped) | **HARVEST → keep** as the leader-election lock |
| cross-session pooling | `daemon/{workspace_pool,watcher_pool,session,workspace_session_attachment}.rs` | **COLLAPSE** — replaced by leader-owns-writes + per-process open |
| embedding sharing | `daemon/embedding_service.rs` + `julie-pipeline` sidecar supervisor/provider/protocol | **MOVE** → resident embedding-host; sidecar mechanism reused **as-is** |
| daemon DB | `daemon/database/**` (workspaces, tool_calls, codehealth, search_compare), `daemon/{workspace_registry_store,connection_pool}.rs` | **MOVE** registry/tool_calls/codehealth → shared `registry.db` (WAL); **DELETE** search_compare + session-count surface + connection_pool shim |
| runtime context | `daemon/app/runtime.rs` (`DaemonRuntimeContext` owns the `mutation_gate_registry`) | **REWIRE** — this is the splice point for the leader lock |
| dashboard host | `daemon/app.rs` binds+serves the dashboard; `DashboardState` consumes nearly every daemon internal | **DECISION REQUIRED — G1, see §8** |

**Keep untouched (the substrate the new model builds ON, not part of the teardown):** the in-process `mutation_gate` (still serializes the leader's own concurrent writers), `SearchIndex` Tantivy mechanics, SQLite WAL config, the projection/revision tables, and the catch-up + repair machinery.

---

## 3. Target model (converged across maps 0/1/2/3/5)

1. **Client path:** `main.rs` `None` arm builds deps (workspace open, mutation gate, embedding client) and `serve(stdio())` the handler in-process. No HTTP, no discovery.json, no token, no fork. The `x-julie-workspace*` header contract becomes plain startup args (`WorkspaceStartupHint` is already resolved in `main`).
2. **Write leadership:** generalized `DaemonLockGuard` on a per-workspace lock under `.julie/indexes/{workspace_id}/`. Winner = sole watcher + writer. Losers never call any of the 8 writers.
3. **Reads:** losers open `symbols.db` over WAL (already configured, `database/mod.rs:127`, busy_timeout 5s) and the Tantivy index read-only via mmap. **`registry.db`** (ex-`daemon.db`) is opened WAL by any process.
4. **Embedding-host:** the one always-on process. Reuses `SidecarEmbeddingProvider` + envelope protocol + circuit breaker **as-is**; adds a **front door** (UDS on unix / named pipe on Windows) exposing `embed_query`/`embed_batch`/`health` over the existing `RequestEnvelope`. Session processes get a thin client `EmbeddingProvider` that RPCs the host instead of spawning their own sidecar.
5. **Handoff/recovery:** new leader runs existing `run_primary_workspace_repair` (catch-up) + `retry_persisted_repairs`, **plus the new durable projection-revision reconciliation in §6.**

---

## 4. R1 — cross-process Tantivy (the flagged "biggest unknown"): substantially DE-RISKED, one experiment remains

**Known-good from vendored `tantivy-0.26.1` source:**
- Single-writer is a **hard OS lock** (`.tantivy-writer.lock`); a 2nd `IndexWriter` (even cross-process) fails `LockBusy`. Julie already **releases the writer eagerly** after each projection (`index.rs:667`) — so leader-owns-writes fits the grain of the code.
- Default reader is `ReloadPolicy::OnCommitWithDelay` → a **poll-based** `FileWatcher` checksums `meta.json` every **500ms** (`file_watcher.rs:12`) and broadcasts reload. This is filesystem-driven, so a reader **in a separate process** picks up another process's commits within ~500ms — no in-process channel needed.
- Segment GC holds `META_LOCK` while computing living files (`managed_directory.rs:109-145`) — explicitly cross-process-safe; on Unix an mmap'd-then-unlinked segment stays valid.

**The one remaining unknown (G3 — must be closed by a TEST, not by reading):** Julie's search read path calls `reader.searcher()` **without** an explicit `reader.reload()` (`index.rs:780,1145`); only `num_docs()` reloads (`index.rs:601`). Under `OnCommitWithDelay` the background poll *should* drive reload, but this was only ever exercised inside the single daemon process. **Required pre-teardown experiment:** a 2-process integration test — process A opens writer, indexes + commits; process B (separate OS process) opens a reader on the same dir, searches, asserts the new docs appear within ~1s. Plus a **Windows variant** for the GC-of-mmap'd-file path (`managed_directory.rs:171-173` logs-and-skips; segments leak but must not corrupt). If the poll does not propagate to `searcher()`, the fix is a cheap `reader.reload()` (or a canonical-revision-changed check) on the read path before `searcher()`.

**Staleness contract:** cross-process readers are eventually-consistent (~500ms). Acceptable for code intel; must be documented. Same-process read-your-writes is preserved (the commit path force-reloads).

**RESULT (3a, 2026-06-04 — CONFIRMED ✅, lead-verified):** the unknown is closed positively. `crates/julie-index/src/tests/search/tantivy_cross_process_reload_test.rs` proves a reader picks up a writer's commits via `search_symbols()` (the real `reader.searcher()` path, no explicit `reload()`): two independent in-process `SearchIndex` instances → 377ms; a genuinely separate OS process (writer via `current_exe()` subprocess) → 220ms, with mmap'd segments confirmed readable after the writer process exits. **No `reader.reload()` fix is required.** Windows GC-of-mmap'd-segment is documented as a leak-not-corruption caveat (Unix proof is the must-have). The shared-on-disk-Tantivy read model is validated.

---

## 5. The hang bug (Phase 3 prerequisite — now localized)

Map 7 localized the never-root-caused `fast_search` hang / `deep_dive` disconnect to two compounding causes:
1. **Index lock held across a 30s embed round-trip.** `get_context`/`hybrid_search` acquire the per-process `SearchIndex` `Mutex` and *then* call the sidecar `embed_query` (`get_context/pipeline.rs:235-237` → `hybrid.rs:321`, up to 30s). A slow/restarting sidecar stalls every other index user.
2. **No per-request deadline on the transport.** The adapter's `transport.receive()` has no timeout (`forwarder.rs:166`), so a stalled handler presents to the MCP client as an indefinite hang.

**Fix (additive, no teardown):** (a) compute the query embedding **before** acquiring the `SearchIndex` lock; (b) add a per-request deadline (server-side timeout layer or client-side receive deadline that synthesizes a JSON-RPC error for the in-flight id). Capture a reproducing test first (the spec's "verify the teardown against a captured repro"). In the in-process model the deadline becomes a handler-level guard.

---

## 6. The recovery hole (G6 — recovery is NOT free)

Maps 3/5 claimed handoff "rides existing machinery." Map 6 **proved** a concrete uncovered case:

> A file whose **SQLite commit succeeded** (`handlers.rs:332`) — which also advances the durable blake3 hash (`handlers.rs:341`) and bumps `canonical_revision` — but whose **Tantivy projection did not** (`handlers.rs:409`) is **permanently stale** and recovered by **nothing**: catch-up skips it (hash matches, `incremental.rs:116-118`), repair-replay ignores it (only `ExtractorFailure` is durable, `runtime.rs:226`), Tantivy-dirty retry lost it (in-memory `HashSet`, `runtime.rs:42`), and startup open-rebuild only fires on structural incompatibility. The durable `canonical > projected` lag signal exists but is read **only** by dashboard/health (`health/projection.rs:8`) — nothing repairs it.

**Fix (new, but reuses existing functions):**
- Make the watcher per-file Tantivy success **advance `projected_revision`** (one `upsert_projection_state` per save) so `canonical > projected` becomes a *true* crash signal instead of firing on every healthy save.
- On leader acquisition, add a **startup reconciliation**: if `projected_revision < canonical_revision`, call the existing `SearchProjection::ensure_current_from_database` (`projection.rs:40`) which rebuilds Tantivy from canonical SQLite and stamps `projected = canonical`.
- This is materially cheaper than per-write SQLite+Tantivy two-phase atomicity. (Alternative: persist the `tantivy_dirty` set to a durable table for exact replay — more code, incremental instead of full rebuild. Decide by workspace size.)

This work is **valuable today** (the hole exists in the current daemon), so it lands early and de-risks handoff for free.

---

## 7. Deletion DAG (G5 — single owner, ordered edges)

Three maps independently scheduled overlapping deletions with inconsistent prerequisites. The ordered DAG:

1. **Only after the in-process server + leader model is live and proven:** delete `adapter/**`, `bin/julie-adapter.rs`, `daemon/{http_transport,http_client,transport}.rs`.
2. **Only after the HTTP path is gone:** delete `daemon/token_file.rs` (auth for HTTP), `daemon/shutdown_event.rs` (Windows stop-IPC), most of `daemon/cli.rs` Start/Stop/Status, `fd_limit.rs`.
3. **`legacy_migration.rs` first, then `singleton.rs` + `daemon_singleton_lock()`** — singleton is only referenced by legacy_migration + tests; legacy_migration only guards against pre-split daemons that no longer exist.
4. **`src/migration.rs`** (pre-daemon per-project→shared index mover) — **deletes only after** confirming no not-yet-migrated users depend on it (`migration.rs:278` still upserts into the registry). Gate behind a one-release deprecation if telemetry can't confirm.
5. **`daemon/database/search_compare.rs` + migration_004** — dev/dogfood bakeoff telemetry; delete with the dual-write cleanup (G7).
6. **`pid.rs` write-path** (create/create_exclusive — all 11 callers are tests today) — delete with singleton.

**G7 (dual-write):** tool-calls are written to BOTH `daemon.db.tool_calls` and per-workspace `SymbolDatabase.tool_calls`; dashboard reads only the central one. Before picking a source of truth, `fast_refs` the per-workspace read methods — if dead read-side, the central copy wins and the dual-write is pure deletion.

---

## 8. DECISION REQUIRED — G1: the dashboard's host

The dashboard is **bound and served by the daemon itself** (`app.rs:261/326/416`). `DashboardState` consumes `lifecycle::{LifecyclePhase,ShutdownCause}`, `SessionTracker`, `shutdown::RecoveryMarker`, `WatcherPool`, `WorkspacePool`, `daemon_db`, plus dashboard-only `search_analysis.rs`/`search_compare.rs`/`error_buffer.rs`. It is **the single largest consumer of the structures we're deleting.** Three options:

- **(A) Re-home onto the leader process** — the leader (always present when any session is open) binds the dashboard. Forces `SessionTracker`/`RecoveryMarker`/lifecycle-phase to survive in some thin form. Most feature-preserving, most coupling retained.
- **(B) Standalone reader of `registry.db`** — dashboard becomes a small CLI-launched server reading the shared WAL DB + Tantivy read-only. Cleanest separation; loses live in-process signals (lifecycle phase, live session list) unless persisted.
- **(C) Drop the live dashboard** — keep only what the CLI/`get_context` already surface. Smallest teardown; removes a shipped feature.

This is a product call, not a code call.

**DECISION (owner, 2026-06-04): Option B — standalone `registry.db` reader.** The dashboard becomes a small CLI-launched server that opens the shared WAL `registry.db` + Tantivy read-only. Live in-process signals it currently pulls from `SessionTracker`/`RecoveryMarker`/lifecycle-phase must either be **persisted to `registry.db`** (so the standalone reader can surface them) or dropped from the dashboard; 3c enumerates each consumed signal and classifies persist-vs-drop. The dashboard-only `search_analysis.rs`/`search_compare.rs`/`error_buffer.rs` move with it or are dropped per the §7 dual-write/search_compare cleanup.

---

## 9. Proposed decomposition (4 human-merge-gated sub-PRs, like Phase 2)

- **3a — De-risk + prerequisites (additive, zero teardown).** (i) Hang fix §5 + captured repro test. (ii) R1 2-process Tantivy reload experiment §4 (+Windows). (iii) Recovery hardening §6 (watcher advances `projected_revision` + startup reconciliation). All three are correctness wins *today* and gate the cutover. **Lowest risk, highest immediate value — ships first regardless of the G1 decision.**
- **3b — Resident embedding-host.** Move `EmbeddingService` out of `daemon/`; add the UDS/named-pipe front door + thin RPC client provider. Runs alongside the existing daemon (additive) — proves one sidecar serves N processes before anything is deleted. Acceptance: 3 concurrent sessions, one model in VRAM.
- **3c — In-process server + leader election (the cutover).** Rewire the no-args path to in-process `serve(stdio())`; generalize `DaemonLockGuard` to the per-workspace leader lock; winner runs watcher+writes, losers are WAL/mmap readers; resolve G1 dashboard host. The daemon still exists but is now bypassed by the in-process path.
- **3d — Delete the daemon + adapter.** Execute the §7 deletion DAG; move `daemon.db` → `registry.db`; collapse pooling; reduce `daemon/` to leader-coord + embedding-host glue. Acceptance: `adapter/` gone, repro no longer reproduces, killing the writer degrades freshness only and a new leader recovers (verified).

Each sub-PR: own branch, dep-direction tripwires preserved, `cargo xtask test dev` branch-gate green, relink/behavior evidence in the verification ledger.

---

## 10. Open decisions for the owner

1. **G1 dashboard host:** (A) re-home on leader / (B) standalone `registry.db` reader / (C) drop. *(blocks 3c)*
2. **G6 recovery:** full `ensure_current_from_database` rebuild on handoff (simpler) vs durable `tantivy_dirty` table for incremental replay (more code). *(default: full rebuild; revisit if workspaces are large)*
3. **Architecture confirm:** in-process stdio server + leader lock (maps converge here) vs keep a thin always-on coordinator daemon. *(default: in-process per the maps)*
4. **3a-first sequencing:** ship the de-risk PR (hang fix + R1 experiment + recovery hardening) before any teardown. *(strong recommendation: yes)*

---

## Appendix — harvested primitives (reuse, don't reinvent)

- `DaemonLockGuard` (`discovery.rs:76,98,123`) — fs2 exclusive advisory flock, descriptor-bound, kernel-released on crash, Windows `ERROR_LOCK_VIOLATION` mapped. **= the leader-election lock.**
- `recreate_index_with_lock` (`index.rs:1654`) — existing cross-process fs2 lock + atomic tmp-dir rename. Pattern for per-workspace write lock.
- SQLite WAL (`database/mod.rs:127`) + `canonical_revisions`/`projection_states` (`revisions.rs:50`, `projections.rs:79`) — the freshness oracle, already durable + WAL-visible.
- `ensure_current_from_database` (`projection.rs:40`) — revision-aware rebuild-from-SQLite, reused for handoff recovery.
- sidecar provider + envelope protocol + circuit breaker (`julie-pipeline`) — reused as-is inside the embedding-host.
