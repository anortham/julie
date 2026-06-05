# Julie Phase 3d — Delete Daemon + Adapter (Implementation Plan)

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Remove the now-bypassed HTTP daemon + stdio adapter so `julie-server` is a single in-process MCP server (leader-locked) plus the resident embedding-host, and the dashboard becomes a standalone `registry.db` reader.

**Architecture:** Phase 3c.3 flipped no-args `julie-server` to serve `JulieServerHandler` in-process over rmcp stdio, guarded by the per-workspace OS leader lock. The daemon's HTTP serve path, the stdio adapter, and the multi-session pools are now dead weight reachable only via the explicit `julie daemon` subcommand. 3d deletes them in dependency order (clients → server → data/dashboard), preserving the leader lock (`daemon/discovery.rs`), the embedding-host glue, the per-workspace SQLite/Tantivy stores, and `RecoveryMarker`.

**Tech Stack:** Rust, rmcp (stdio serve), fs2 advisory flock (leader lock), SQLite WAL (registry + per-workspace), axum/Tera (dashboard, being demoted to a read-only reader), tree-sitter (unaffected).

**Architecture Quality:** Approved shape — one resident process per machine wins the leader lock and is the sole watcher + Tantivy writer; the embedding-host is the only *other* resident process; everything else is a transient in-process session or a read-only reader. The main architecture risk is **deleting code that is still load-bearing for the in-process or embedding-host path** (especially `daemon/discovery.rs` = the leader lock, `DaemonPaths`, `daemon/shutdown.rs` = `RecoveryMarker`, `daemon/embedding_service.rs`). Every deletion task must `fast_refs` the symbol from the *kept* surface before removing it. If code reality contradicts this shape (e.g. the in-process path turns out to depend on a pool), the worker reports a plan mismatch rather than redesigning locally.

---

## Source design

`docs/plans/2026-06-04-julie-phase3-daemon-teardown-design.md` — §7 deletion DAG (ordered edges), §11 dashboard persist-vs-drop classification (Option B standalone reader). This plan executes that design.

**Owner decisions (2026-06-05):**
1. `src/migration.rs` (pre-daemon per-project→shared index mover): **delete now** (no install base to break; do not gate behind a deprecation release).
2. Dashboard: **standalone `registry.db` reader (Option B)** per §11.

---

## Decomposition (three human-merge-gated sub-PRs, executed in order)

Refinement from the original "3d.1 = delete adapter + HTTP transport": orientation showed "HTTP transport" splits into a **client side** (truly bypassed by the cutover, safe to delete now) and a **server side** (still compiles and is reachable via `julie daemon`, a much larger teardown). So:

- **3d.1 — Cut the daemon-client surface.** Delete `adapter/**`, `bin/julie-adapter.rs`, the CLI's daemon-IPC mode (`cli_tools/daemon.rs`), and `daemon/http_client.rs`; route the CLI to standalone-only. This is the "adapter is gone" milestone — pure deletion of cutover-bypassed code. **This plan details 3d.1 in full.**
- **3d.2 — Tear down the HTTP daemon server.** Delete `daemon/app.rs` + `daemon/app/**` + `http_transport.rs` + `transport.rs` + `mcp_session.rs` + `lifecycle.rs` + `workspace_pool.rs` + `watcher_pool.rs` + `session.rs` + `token_file.rs` + `shutdown_event.rs` + `fd_limit.rs` + the `julie daemon` subcommand (`cli.rs` `Command::Daemon`, `daemon/cli.rs`, `daemon/mod.rs::run_daemon`), then `legacy_migration.rs` → `singleton.rs` → `pid.rs` write-path. Decide the `julie-daemon` bin's fate. *(Outline below; detailed when reached.)*
- **3d.3 — Data + dashboard.** `daemon.db` → `registry.db`; standalone registry-reader dashboard per §11 (add `daemon_state.started_at_unix`, drop mutation routes + live signals); delete `database/search_compare.rs` + the G7 dual-write; delete `src/migration.rs`. *(Outline below; detailed when reached.)*

**Hard sequencing gate:** §7 DAG step 1 requires "the in-process server is live and proven" before deletions. That proof is the **3c.3 live dogfood smoke** (user-only: rebuild `--release`, restart the MCP client, confirm no `julie-daemon`/`julie-adapter` fork via `ps` and no `~/.julie/.../discovery.json`, then a live `fast_search` + `edit_file` + re-search round-trip). **3d.1 may be implemented and branch-gated, but its PR must not merge until that smoke passes.**

---

## File Structure (3d.1)

**Delete:**
- `src/adapter/mod.rs`, `src/adapter/forwarder.rs`, `src/adapter/http_stdio.rs`, `src/adapter/launcher.rs` — the stdio↔daemon bridge (now bypassed; `main.rs` None arm serves in-process).
- `src/bin/julie-adapter.rs` — the standalone adapter binary.
- `src/cli_tools/daemon.rs` — `DaemonClient` (HTTP MCP client), `run_via_daemon` glue, `ensure_daemon_ready`, `try_connect_daemon`.
- `src/daemon/http_client.rs` — `http_client_config_for_endpoint`; consumed only by the deleted clients (adapter + cli_tools/daemon).

**Modify:**
- `src/lib.rs` — remove `pub mod adapter;` (line 27).
- `src/daemon/mod.rs` — remove `pub mod http_client;` (line 14).
- `src/cli_tools/mod.rs` — `run_cli_tool` routes to `bootstrap_standalone_handler` only; remove `run_via_daemon`/`DaemonClient` usage and the daemon-first-then-standalone fallback (there is no resident HTTP daemon to call post-cutover).
- `Cargo.toml` — remove the `[[bin]] name = "julie-adapter"` target.
- `src/tests/integration/in_process_boundary.rs` — T12 §7-files list: remove the four `adapter/**` paths, `src/bin/julie-adapter.rs`, and `src/daemon/http_client.rs` (those are now deleted, not "bypassed"); keep the still-bypassed server files (`http_transport.rs`, `transport.rs`, `singleton.rs`, `legacy_migration.rs`, `pid.rs`, `search_compare.rs`, `migration.rs`). Remove the `crate::adapter::run_adapter` reference in `_bypassed_entry_points_still_compile` (keep the `start_daemon` one — that survives until 3d.2).
- `xtask/test_tiers.toml` — the `transport` bucket runs `cargo nextest run --lib tests::adapter`; that module is being deleted. Re-point or remove that command so the bucket does not orphan to a zero-match nextest exit 4 (the documented orphaned-filter failure mode).

**Delete (tests):**
- `src/tests/adapter/**` and any `src/tests/cli_tools/**` test asserting daemon-IPC CLI behavior. Update `src/tests/mod.rs` / parent `mod` declarations accordingly.

**Keep (must NOT be deleted in 3d.1 — verify before touching):**
- `src/daemon/transport.rs` (`TransportEndpoint`) — still used by the daemon server (`http_transport.rs`) and `legacy_migration.rs`; goes in 3d.2.
- `src/daemon/http_transport.rs` — the server; goes in 3d.2.
- `src/daemon/discovery.rs` (`DaemonLockGuard`) — the leader lock; the in-process path depends on it. **Permanent keep.**

---

## Tasks (3d.1)

### Task 1: Route the CLI to standalone-only; delete the daemon-IPC client

**Files:**
- Delete: `src/cli_tools/daemon.rs`
- Modify: `src/cli_tools/mod.rs` (`run_cli_tool` dispatch ~line 200-275; remove `run_via_daemon`, `mod daemon;`, and the daemon-then-standalone fallback)
- Test: `src/tests/cli_tools/**` (update any test that exercised the daemon-IPC path; CLI-standalone tests stay)

**What to build:** After the cutover there is no resident HTTP daemon for the CLI to call, so `run_cli_tool` must execute every tool via `bootstrap_standalone_handler` directly. Remove the `run_via_daemon` branch, the `DaemonClient`/`ensure_daemon_ready`/`try_connect_daemon` surface, and the "try daemon, fall back to standalone" logic.

**Approach:** `run_cli_tool` becomes "build the startup hint → `bootstrap_standalone_handler` → call the tool → render". Preserve the existing standalone output/exit-code semantics (tool-error → exit 1). Drop `DaemonCallError` and its `Transport`/`ToolError` split — standalone has no transport layer. Keep `build_cli_startup_hint`. Use `fast_refs` on `DaemonClient`, `run_via_daemon`, `ensure_daemon_ready`, `try_connect_daemon`, `daemon_status` before deleting, to catch any caller outside `cli_tools/`.

**Acceptance criteria:**
- [ ] `src/cli_tools/daemon.rs` deleted; no `mod daemon;` in `cli_tools/mod.rs`.
- [ ] `run_cli_tool` executes tools standalone with no daemon-connect attempt.
- [ ] `./target/debug/julie-server search "@test" --target definitions --workspace . --standalone --json` returns results (manual dogfood by the worker, reported as diagnostic).
- [ ] `cargo nextest run -p julie --no-run` compiles (no orphaned references).
- [ ] Worker-scope tests for the touched CLI path pass; committed.

### Task 2: Delete the adapter crate-module and `http_client.rs`; drop the bin target

**Files:**
- Delete: `src/adapter/mod.rs`, `src/adapter/forwarder.rs`, `src/adapter/http_stdio.rs`, `src/adapter/launcher.rs`, `src/bin/julie-adapter.rs`, `src/daemon/http_client.rs`
- Modify: `src/lib.rs` (remove `pub mod adapter;`), `src/daemon/mod.rs` (remove `pub mod http_client;`), `Cargo.toml` (remove `[[bin]]` julie-adapter)

**What to build:** Physically remove the adapter module tree, its binary, and the client-only `http_client.rs`. This is the "adapter is gone" milestone.

**Approach:** Depends on Task 1 (the CLI's `adapter::launcher::DaemonLauncher` reference is gone). Before deleting `http_client.rs`, `fast_refs` `http_client_config_for_endpoint` to confirm only the (now-deleted) adapter + cli_tools/daemon imported it. After removing `pub mod adapter;` and `pub mod http_client;`, `cargo nextest run -p julie --no-run` is the authority (`cargo check` skips cfg(test) — do not trust it alone). Watch for `MCP_PATH`/`READINESS_PATH` constants: `adapter/launcher.rs` imported them from `http_transport`; deleting the adapter removes those importers but `http_transport` (kept) may still define them — leave `http_transport` intact.

**Acceptance criteria:**
- [ ] All six files deleted; `src/adapter/` directory gone.
- [ ] No `[[bin]]` julie-adapter in `Cargo.toml`; `cargo build --bins` builds only `julie-server`, `julie-daemon`, `julie-embedding-host`.
- [ ] `cargo nextest run -p julie --no-run` compiles.
- [ ] `cargo build --bin julie-server` succeeds (production compiles).
- [ ] Committed.

### Task 3: Flip the T12 tripwire and repair the `transport` test bucket

**Files:**
- Modify: `src/tests/integration/in_process_boundary.rs` (the `section7_files` list + `_bypassed_entry_points_still_compile`)
- Modify: `xtask/test_tiers.toml` (the `transport` bucket's `tests::adapter` command)
- Delete/Modify: `src/tests/adapter/**` + `src/tests/mod.rs` registration

**What to build:** Update the boundary tripwire so it asserts the *remaining* §7 server files still exist (bypassed-not-deleted) while the now-deleted adapter/`http_client` files are removed from the list. Repair the dev-tier `transport` bucket whose `tests::adapter` filter just lost its module.

**Approach:**
- In `in_process_boundary.rs`: remove from `section7_files` the four `src/adapter/*.rs`, `src/bin/julie-adapter.rs`, and `src/daemon/http_client.rs`. Keep `http_transport.rs`, `transport.rs`, `singleton.rs`, `legacy_migration.rs`, `pid.rs`, `database/search_compare.rs`, `migration.rs`. In `_bypassed_entry_points_still_compile`, delete the `crate::adapter::run_adapter` line; keep `crate::daemon::cli::start_daemon`. The `no_args_main_serves_in_process_not_adapter` test (source-scans `main.rs`) is unchanged — `main.rs` still must not reference `run_adapter`/`DaemonLauncher`.
- Delete `src/tests/adapter/**`; remove its `mod` registration.
- In `test_tiers.toml`, the `transport` bucket currently runs `cargo nextest run --lib tests::adapter` as one of its commands. Remove that command (the module is gone). **Then run the orphaned-filter scan:** `cargo nextest list -p julie --lib tests::adapter` must report zero, and no other bucket filter may name a deleted module. (This is the documented Phase-1/2 failure mode: a stale `--lib` filter exits nextest 4 and silently reds the tier.)

**Acceptance criteria:**
- [ ] `in_process_boundary.rs` tests pass: `cargo nextest run -p julie --lib tests::integration::in_process_boundary`.
- [ ] No `tests::adapter` references remain in `test_tiers.toml`; `cargo nextest list -p julie --lib` shows no zero-match filters.
- [ ] `src/tests/adapter/` deleted and de-registered.
- [ ] Committed.

### Task 4 (lead): 3d.1 branch-gate + ledger

**Owner:** lead (not a worker).

**What to do:** After Tasks 1-3 are reviewed and committed, run the branch gate and record the ledger.

- `cargo xtask test dev` (must stay GREEN with the *server* daemon tests still present — only the adapter/client surface was removed).
- `cargo xtask test system` (startup/serve path).
- `cargo xtask test reliability` (lifecycle).
- Record invariant/command/scope/SHA/result/timestamp per `docs/plans/verification-ledger-template.md`.

**Acceptance criteria:**
- [ ] dev + system + reliability GREEN at the 3d.1 HEAD; ledger rows recorded.
- [ ] Then: codex pre-merge review → push → PR for **human merge**, gated on the 3c.3 live smoke.

---

## 3d.2 — Tear down the HTTP daemon server (OUTLINE — detail when reached)

Delete the multi-session HTTP server and the `julie daemon` subcommand now that nothing launches them (the adapter that auto-spawned the daemon is gone after 3d.1; the in-process path replaced it). Targets: `daemon/app.rs` + `daemon/app/**`, `http_transport.rs`, `transport.rs`, `mcp_session.rs`, `lifecycle.rs`, `workspace_pool.rs`, `watcher_pool.rs`, `session.rs`, `token_file.rs`, `shutdown_event.rs`, `fd_limit.rs`; the `Command::Daemon` arm + `daemon/cli.rs` + `daemon/mod.rs::run_daemon`; then `legacy_migration.rs` → `singleton.rs` → `pid.rs` write-path (DAG step 3). Decide the `julie-daemon` bin (likely deleted, or repurposed as a thin embedding-host launcher). **Must verify** the in-process path and embedding-host do not depend on the pools/lifecycle/session before deleting (orientation suggests they do not — `run_in_process_server` uses only `discovery` + `DaemonPaths` + embedding glue). Update the remaining half of the T12 tripwire (`start_daemon` reference + the server files in `section7_files`). This is the largest sub-PR; it may itself split if the gate surface is too broad.

## 3d.3 — Data + standalone dashboard (OUTLINE — detail when reached)

`daemon.db` → `registry.db` rename/migration; add `registry.db.daemon_state.started_at_unix` (the one new write-site from §11); build the standalone registry-reader dashboard (reads `registry.db` + per-workspace `symbols.db` projection_states + `~/.julie/recovery-*.json`; drops all mutation routes, SSE, CSRF, live session/indexing/embedding signals per the §11 DROP list); G7 dual-write cleanup — `fast_refs` the per-workspace `SymbolDatabase.tool_calls` read methods, and if the read-side is dead, delete `database/search_compare.rs` + the dual-write and make the central copy the source of truth; delete `src/migration.rs` (owner decision: delete now) + its `pub mod migration;` in `lib.rs` + the legacy-migration gate in `start_daemon` (already removed with 3d.2).

---

## Verification Strategy

**Project source of truth:** `CLAUDE.md` / `AGENTS.md` (xtask tiers), `docs/TESTING_GUIDE.md`, `RAZORBACK.md`.

**Worker red/green scope:** `cargo nextest run -p julie --lib <exact_test_name>` (narrow, named, ≤2 runs RED→GREEN). For compile-only confirmation after a deletion, `cargo nextest run -p julie --no-run` (NOT `cargo check` — it skips cfg(test)).

**Worker ceiling:** the named test(s) for the touched area + a `--no-run` compile. Workers do **not** run `cargo xtask test changed`/`dev`/`system`/`reliability` — the lead owns those.

**Worker gate invariant:** each worker states what its assigned test proves (e.g. "in_process_boundary asserts the post-3d.1 §7 set: adapter/http_client gone, server files still present").

**Lead affected-change scope:** `cargo xtask test changed` during the local loop after a coherent batch.

**Branch gate (lead, once per sub-PR before handoff):** `cargo xtask test dev` GREEN **with the daemon SERVER tests still present in 3d.1** (only the adapter/client surface is removed in 3d.1) + `cargo xtask test system` + `cargo xtask test reliability`. These tiers run the daemon/adapter integration tripwires that `--no-run` does NOT execute — always run the full tier before declaring a deletion PR done (the Phase-2b lesson).

**Replay/metric evidence:** none (deletion phase). The 3c.3 kill-the-writer HARD GATE already proves recovery; do not re-litigate it here.

**Escalation triggers:** any deletion that fails to compile because a *kept* symbol depended on it (plan mismatch — stop and report); the `transport` bucket orphaning; the dev tier going red on a daemon-server test after a client-only deletion (means the cut was not actually client-only).

**Assigned verification failure:** workers stop and report; they do not "fix" a red dev tier by deleting more than their task scope.

**Verification ledger:** `docs/plans/verification-ledger-template.md`. Reuse a passing `branch-gate` row only when scope label + HEAD SHA match exactly.

**Live-smoke merge gate (3d.1):** the PR does not merge until the user confirms the 3c.3 live dogfood smoke passes.

---

## Model Routing

**Project source of truth:** `RAZORBACK.md` (full table not copied here per its instruction; plan-specific overrides below).

Per `RAZORBACK.md`, **daemon lifecycle, transport/security policy, watcher/session reference ownership, and public CLI protocol compatibility are shared-invariant work.** 3d touches all of these, so:

- **Strategy / escalation tier (lead, Opus):** decomposition, every "is this symbol still load-bearing?" call, the CLI-standalone routing contract (Task 1 — it changes the CLI's public execution model), the branch gate, codex finding triage. The lead owns Task 1's contract before any worker edits.
- **Coupled-implementation tier (Sonnet high):** Task 2 (multi-file deletion + module/bin de-registration) and Task 3 (tripwire + bucket repair) — bounded, but cross-file and touching the test-tier manifest, so above plain implementation tier.
- **Mechanical tier:** none here — every task touches a gate (T12 tripwire, the `transport` bucket, or the CLI contract). No mechanical-only lane in 3d.1.

**Worker eligibility:** workers may take Task 2 and Task 3 only after the lead has confirmed (via `fast_refs`) the deletion set is fully severed from kept code and fixed Task 1's CLI contract. Disjoint write scopes: Task 2 owns `src/adapter/**` + `http_client.rs` + manifests; Task 3 owns the tripwire + `test_tiers.toml` + `src/tests/adapter/**`. Task 1 is lead-led or a single coupled-tier worker (it must land before 2/3).

**Escalation triggers:** a deletion breaks a kept consumer; the dev tier reds on a daemon-server test; the CLI standalone path changes observable output. Any of these → lead/strategy tier.

**Unsupported harness behavior:** Claude Code Agent `model` accepts `opus`/`sonnet`/`haiku` — map coupled-implementation → `sonnet`, strategy/escalation → `opus`.

---

## Team note (user preference)

Per standing preference, execution uses a **TeamCreate visible Sonnet team + Opus lead** (not background agents). Freeze each worker after acceptance. Stage owned files by explicit path (never `git add -A`; `.codex/config.toml` and `.miller/` must never be committed; `.memories/` always committed).
