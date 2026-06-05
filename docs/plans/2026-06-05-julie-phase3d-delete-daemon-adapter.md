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
- **3d.2 — Tear down the HTTP daemon server.** SPLIT into **3d.2a (entry kill)** + **3d.2b (server-core kill)** after the mapping workflow (2026-06-05) proved one PR is too large and surfaced three compile-couplings. 3d.2a deletes the daemon *entry* (the `julie daemon` subcommand + `julie-daemon` bin + `run_daemon` + the entry-only write-path `legacy_migration.rs`/`fd_limit.rs`), leaving a compiling tree with no way to start the HTTP daemon. 3d.2b deletes the now-orphaned *server core* (`app.rs`+`app/**`, `http_transport.rs`, `transport.rs`, `mcp_session.rs`, `session.rs`, `lifecycle.rs`, `shutdown_event.rs`, `token_file.rs`, `singleton.rs`) + the `InProcessDaemon` test harness + the handler.rs pool-field surgery that gates deleting `workspace_pool.rs`/`watcher_pool.rs`. **`pid.rs` is NOT deleted in 3d.2** — `discovery.rs` (the kept leader lock) imports it; it moves to 3d.3 with the discovery.json-reader excision. *(3d.2a detailed below; 3d.2b outlined.)*
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

## 3d.2 Map (verified 2026-06-05 — 9-reader workflow + completeness critic + lead spot-check)

A mapping workflow classified every `src/daemon` file and proved the keep-set is severed from the deletion targets. Lead re-verified the three plan-reshaping findings via `rg` (discovery.rs:51 pid import; handler.rs:277/287 pool fields; in_process.rs:120 DaemonApp build). Digest:

**Keep-set (do NOT delete in any 3d.2 sub-PR — verified load-bearing or permanent):**
- `daemon/discovery.rs` (DaemonLockGuard leader lock) — PERMANENT. *Imports `pid.rs` at module level (line 51) → see pid.rs note.*
- `paths.rs` DaemonPaths, `daemon/shutdown.rs` RecoveryMarker, `daemon/embedding_service.rs` — PERMANENT.
- `daemon/connection_pool.rs` (re-exports `julie_core` WorkspaceConnectionPool; used by `handler.rs:2456` + `tool_metrics.rs:65`) — PERMANENT. *(Not a multi-session pool — name collision with workspace_pool/watcher_pool.)*
- `daemon/project_log.rs` — PERMANENT (per-project log writer; in-process path uses it).
- `server_in_process.rs`, `handler.rs`, `leadership.rs`, both kept bins (`julie-server` via main.rs, `julie-embedding-host`) — KEEP. Verified: `run_in_process_server`'s dependency closure touches ONLY discovery, DaemonPaths, shutdown/RecoveryMarker, embedding_service, connection_pool, project_log — **zero** imports of app/**, http_transport, transport, the pools, sessions, or lifecycle. **The plan's main risk is retired.**

**Keep-until-3d3 (data/dashboard, or pid coupling):**
- `daemon/database.rs` + `daemon/database/**` (DaemonDatabase = the workspace registry; becomes registry.db in 3d.3), `daemon/workspace_registry_store.rs`.
- `daemon/pid.rs` — **RECLASSIFIED from the outline's delete-3d2.** `discovery.rs:51` (kept) imports `PidFile`/`process_creation_time_micros` for the discovery.json reader. pid.rs can only die once that reader is excised, which `discovery.rs`'s own doc-comment schedules for 3d.3. Deleting it in 3d.2 is a compile-break of the kept in-process path.
- `database/search_compare.rs` + dashboard files — 3d.3.

**Delete-3d2a (ENTRY) and delete-3d2b (SERVER CORE):** see the two task sections below.

**Three compile-couplings 3d.2b must handle atomically (lead-verified):**
1. **handler.rs pool surgery gates pool deletion.** `handler.rs:277,287` hold `Option<Arc<WatcherPool>>`/`Option<Arc<WorkspacePool>>` fields (+ None inits at 788/791, + `.is_some()`/`.is_none()` sites at 96/114, + the `new_in_process`/daemon ctors). Runtime-safe (always None in-process) but the TYPES must exist or be removed in lockstep. `workspace_session_attachment.rs` also imports both pool types. → drop the fields/params/call-sites + de-type `workspace_session_attachment.rs` BEFORE deleting `workspace_pool.rs`/`watcher_pool.rs`.
2. **The `InProcessDaemon` test harness dies with the server.** `src/tests/harness/in_process.rs:120` builds `DaemonApp::new + serve` (HTTP path) — it must be deleted/rewritten in the SAME commit as `app.rs`/`http_transport.rs`, or the test tree won't compile. (It has no live instantiators outside itself.)
3. **`singleton.rs` and `shutdown_event.rs` are server-coupled, not entry-coupled.** `singleton.rs` is also used by `app/helpers.rs`; `shutdown_event.rs`'s only caller is `lifecycle.rs`. Both must go in 3d.2b (with the server), NOT 3d.2a.

**Dashboard sequencing (recommendation — owner confirm at approval):** the dashboard's sole production mount is `app.rs:356` (`crate::dashboard::create_router`); the in-process path starts no HTTP listener. After 3d.2a deletes the daemon entry, the dashboard is already unreachable in normal operation (no daemon runs post-cutover). **Lead recommendation: keep the standalone dashboard reader in 3d.3 (server-deletion-first), accept the near-theoretical gap** — the dashboard is already dark in normal post-cutover use, and the standalone reader depends on `registry.db` (the 3d.3 data rename), so building it first would invert the clean deletion ordering. The critic's alternative (reader-first, zero-gap, coexists with the live daemon) is sound if a dashboard outage window is unacceptable; given the owner's "no install base to break" stance, the gap is acceptable. *This is the one decomposition call left for the owner.*

---

## 3d.2a — Entry kill (DETAILED)

**Goal:** Remove every way to *start* the HTTP daemon, plus the write-path code reachable only from that entry. Leaves a compiling tree (`app.rs`/server core still present but unreachable — deleted in 3d.2b). Self-contained, low-risk.

### Task A1: Delete the daemon entry (bin + subcommand + run_daemon)

**Files:**
- Delete: `src/bin/julie-daemon.rs`, `src/daemon/cli.rs`
- Modify: `src/daemon/mod.rs` (delete `run_daemon` fn ~line 298 + its `pub` export + `pub mod cli;`), `src/main.rs` (drop the `Command::Daemon`/`Stop`/`Status`/`Restart` match arms, lines 35-77), `src/cli.rs` (drop the `Daemon`/`Stop`/`Status`/`Restart` `Command` enum variants + `cli_command_needs_workspace_startup_hint` handling), `Cargo.toml` (remove `[[bin]] julie-daemon`)
- Modify (tripwire): `src/tests/integration/in_process_boundary.rs` — re-point the `_bypassed_entry_points_still_compile` ref from `crate::daemon::cli::start_daemon` (line ~36, now deleted) to a still-bypassed server symbol (e.g. `crate::daemon::DaemonApp` or `crate::daemon::http_transport::HttpTransportServer`); leave the `section7_files` server entries.

**What to build:** Sever all external entry into the HTTP daemon in one atomic multi-file commit. `Command::Dashboard` STAYS (it just opens a URL; handled in 3d.3 with the dashboard). After this, `julie-server` exposes only the in-process server + tool subcommands; `julie daemon`/`stop`/`status`/`restart` are gone.

**Approach:** `fast_refs`/`rg` `start_daemon`, `stop_daemon`, `status_daemon`, `run_daemon` first to confirm `main.rs` + `cli.rs` + the tripwire are the only consumers. The four `Command` arms and the enum variants must be dropped together (compile-coupled). Update the `main.rs` doc-comment (it currently says "`julie daemon` subcommands remain during the 3d transition" — now they don't).

**Acceptance criteria:**
- [ ] `julie-daemon` bin + `daemon/cli.rs` deleted; `run_daemon` gone from `daemon/mod.rs`; no `Daemon`/`Stop`/`Status`/`Restart` in `src/cli.rs`.
- [ ] `cargo build --bins` builds only `julie-server` + `julie-embedding-host`.
- [ ] `cargo nextest run -p julie --no-run` compiles.
- [ ] `in_process_boundary.rs` tripwire tests pass (re-pointed ref).
- [ ] Committed.

### Task A2: Delete the entry-only write-path (`legacy_migration.rs`) + orphaned tests

> **Re-scoped 2026-06-05 (lead deletion-safety check):** `fd_limit.rs` was REMOVED from A2 → moved to 3d.2b. Its `desired_nofile_soft_limit` helper is unit-tested in `src/tests/daemon/app_test.rs`, a `DaemonApp` test that 3d.2b deletes. Deleting `fd_limit.rs` in A2 would force editing a 3d.2b test file, breaking the split. Same "surviving test consumer" pattern A1 hit with `run_daemon`. Verified: `legacy_migration`'s symbols (`MigrationDecision`/`check_or_refuse`/`detect_and_attach`) are used ONLY by its own test file — clean to delete now; `wiring_a1_8.rs:31` is a comment, not a use.

**Files:**
- Delete: `src/daemon/legacy_migration.rs`; `src/tests/integration/legacy_migration.rs` (the 9 tests kept in 3d.1 — they exercise the A1.5 migration gate that lived in `start_daemon`, now deleted)
- Modify: `src/daemon/mod.rs` (remove `pub mod legacy_migration;`, line 14), `src/tests/mod.rs` (drop the `pub mod legacy_migration;` registration, line ~156), `xtask/test_tiers.toml` (no `legacy_migration` filter exists — verified — so nothing to remove; if a bucket changed, update `xtask/tests/support/manifest_contract_expected.rs`)
- Modify (tripwire): `in_process_boundary.rs` `section7_files` — remove the `"src/daemon/legacy_migration.rs"` line (line 115; now deleted, was bypassed-present in 3d.1). Leave `fd_limit` untouched (it has no section7 entry; deferred to 3d.2b).

**What to build:** `legacy_migration.rs`'s only caller was `cli.rs:94` (deleted in A1); confirmed zero remaining non-test consumers. Delete it + its test file + the two mod declarations + the tripwire line.

**Approach:** Before deleting, `rg "legacy_migration|MigrationDecision|check_or_refuse|detect_and_attach"` across `src/` + `crates/*/tests` to re-confirm only the test file consumes it (lead already verified, but re-confirm at HEAD). Run the orphaned-filter scan: `cargo nextest list -p julie --lib` must show no zero-match filters after the edit. Do NOT touch `singleton.rs` (used by `app/helpers.rs` → 3d.2b) or `fd_limit.rs` (test consumer `app_test.rs` → 3d.2b).

**Acceptance criteria:**
- [ ] `legacy_migration.rs` + `src/tests/integration/legacy_migration.rs` deleted; both mod decls removed; tripwire `section7_files` line removed.
- [ ] No orphaned `--lib` filter: `cargo nextest list -p julie --lib` shows no zero-match filters; manifest-contract snapshot updated only if a bucket actually changed.
- [ ] `cargo nextest run -p julie --no-run` compiles.
- [ ] `in_process_boundary.rs` tests pass.
- [ ] Committed.

### Task A3 (lead): 3d.2a branch-gate + ledger

Same as 3d.1's Task 4: `cargo xtask test dev` + `system` + `reliability` GREEN at the 3d.2a HEAD (these run the daemon integration tripwires `--no-run` skips); record the ledger; codex pre-merge review; push; PR for **human merge**.

#### 3d.2a Verification Ledger

Gate authority: per [[feedback_prefer_fast_per_crate_gates]] the `cargo xtask test dev/system/reliability` bucket runner flakes on per-bucket timeouts under cold-compile/load, so the branch gate is the **per-crate superset** `cargo nextest run -p julie --no-fail-fast -- --skip search_quality` (runs all 1659 tests incl. the daemon integration tripwires that `--no-run` skips and the daemon-harness tests the buckets exclude) plus a focused re-verify of the F1-changed path and the 3d.2a structural tripwires.

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| 3d.2a structural tripwires: no-args serves in-process (not daemon), §7 DAG files bypassed-not-deleted, e2e no-fork | `cargo nextest run -p julie -E 'test(/in_process_boundary/) | test(/wiring_a1_8/)'` | branch-gate (tripwires) | 6944029d | pass (3/3) | no |
| F1 fix: registry errors no longer reference removed `julie daemon`; dashboard register-error renders new message + guard | `cargo nextest run -p julie -E 'test(/dashboard::projects_actions/) | test(/in_process_boundary/) | test(/wiring_a1_8/)'` | affected-change | 6944029d | pass (15/15) | no |
| Whole-crate regression: 3d.2a introduces no new failures | `cargo nextest run -p julie --no-fail-fast -- --skip search_quality` | branch-gate (superset) | 6944029d | 1655/1659 pass; 4 fails ALL pre-existing/env (see below), none from 3d.2a | 2026-06-05T17:02:35Z | no |
| Compile authority (cfg(test) included) | `cargo nextest run -p julie --no-run` | branch-gate (compile) | 6944029d | pass (clean; only deferred `fd_limit` dead-code warns) | 2026-06-05T17:02:35Z | no |
| No dangling deleted symbols in live builds | codex: `cargo build --bins` + `cargo nextest run -p julie --no-run` + `cargo check -p xtask` | escalation review | 5fd9c1ea | pass (codex-confirmed) | no |

**The 4 whole-crate failures @ 6944029d — all root-caused, none introduced by 3d.2a (registry strings + daemon-entry deletion), all OUTSIDE the documented gate tiers:**
1. `daemon::embedding_host_optin::host_unavailable_when_health_not_ready` — **flake** (passes in isolation). Binary-resolution: test spawns `julie-embedding-host` "next to current executable" (= `target/debug/deps/`), missing under parallel load; [[project_e2e_daemon_tests_stale_binary_flake]]. Untouched by branch; passed at 5fd9c1ea.
2. `daemon::restart_listener::daemon_reaps_idle_session...` — **flake** (passes in isolation). Documented daemon-reaper timing budget under load.
3. `harness::in_process::test_in_process_daemon_starts_and_shuts_down` — **deterministic pre-existing test bug** (since `781884b7`, ancestor of base). Reads `daemon_mcp_token()` = `daemon-mcp.token`, but the HTTP transport writes `token_file()` = `daemon.token` (nothing writes `daemon-mcp.token`). Product fine (readiness probe passes). Self-resolves in 3d.2b (deletes the `InProcessDaemon` harness).
4. `tools::get_symbols_target_filtering_dogfood::test_target_minimal_mode_includes_body_for_child_symbols` — **deterministic pre-existing test bug** (since `5f086d10`, ancestor of base). Hardcodes `src/tools/symbols/mod.rs` (line 37), refactored away. Unrelated dogfood-fixture path drift; flagged for separate maintenance fix.

External review: codex (gpt-5.5, escalation tier) @ 5fd9c1ea, verdict needs-attention, 1 medium finding (F1) — verified real, fixed @ 6944029d. No dangling deleted symbols (codex-confirmed clean builds).

---

## 3d.2b — Server-core kill (OUTLINE — detail when reached)

Delete the now-orphaned HTTP server core + the multi-session pools, handling the three compile-couplings above. Deletion order (from the verified map): (1) `xtask/test_tiers.toml` bucket filters for `transport`/`http_transport`/`lifecycle`/`workspace_pool`/`watcher_pool`/`workspace_cleanup`/`mcp_session` + snapshot — FIRST, before any test file. (2) handler.rs field surgery (drop pool fields/params/call-sites) + de-type `workspace_session_attachment.rs`. (3) Delete `app.rs`+`app/**` (DaemonApp/DaemonHandle/DaemonRuntimeContext), `http_transport.rs`, `transport.rs`, `mcp_session.rs`, `session.rs`, `lifecycle.rs`, `shutdown_event.rs`, `token_file.rs`, `singleton.rs`, **`fd_limit.rs`** (deferred from A2 — its `desired_nofile_soft_limit` helper is unit-tested in `app_test.rs`, deleted here) + their `daemon/mod.rs` decls/re-exports (`pub use app::{DaemonApp,...}` at mod.rs:42; `pub(crate) mod fd_limit;` at mod.rs:12) + the `InProcessDaemon` harness (`src/tests/harness/in_process.rs`) + `src/tests/daemon/app_test.rs` (incl. its 3 `fd_limit::desired_nofile_soft_limit` assertions) in the SAME commit. (4) Delete `workspace_pool.rs`/`watcher_pool.rs`. (5) Delete the ~17 HTTP-transport/session/pool/lifecycle test files + their mod lines. **Also delete `run_daemon`** (`daemon/mod.rs:292`, demoted to `pub(crate)` in A1 because `server.rs`/`daemon_lifecycle.rs` tests drive it — those tests die here). (6) Flip the remaining `section7_files` tripwire entries (`http_transport.rs`, `transport.rs`, `singleton.rs`) from bypassed-present to deleted; keep `pid.rs` + `search_compare.rs` + `migration.rs`. **Verify** at each step with `cargo nextest run -p julie --no-run`; the branch gate (dev+system+reliability) is mandatory (it runs the integration tripwires `--no-run` skips). Largest, riskiest sub-PR.

## 3d.3 — Data + standalone dashboard (OUTLINE — detail when reached)

`daemon.db` → `registry.db` rename/migration; add `registry.db.daemon_state.started_at_unix` (the one new write-site from §11); build the standalone registry-reader dashboard (reads `registry.db` + per-workspace `symbols.db` projection_states + `~/.julie/recovery-*.json`; drops all mutation routes, SSE, CSRF, live session/indexing/embedding signals per the §11 DROP list, plus `dashboard/routes/events.rs` + `projects_actions.rs` which have no read-only analogue); G7 dual-write cleanup — `fast_refs` the per-workspace `SymbolDatabase.tool_calls` read methods, and if the read-side is dead, delete `database/search_compare.rs` + the dual-write and make the central copy the source of truth; delete `src/migration.rs` (owner decision: delete now) + its `pub mod migration;` in `lib.rs`.

**Absorbed from 3d.2 (per the verified map):** excise the `discovery.json` reader bodies in `daemon/discovery.rs` (`DiscoveryRecord::for_current_process`, `DiscoveryFile::read_and_validate` — their only callers die with the server in 3d.2b; the leader-lock `DaemonLockGuard` itself does NOT use them), then remove the `use crate::daemon::pid::{...}` import (discovery.rs:51) and finally **delete `src/daemon/pid.rs`** (held out of 3d.2 because discovery.rs imports it) + its `section7_files` tripwire entry + pid tests. This is the last write-path file. If `daemon/shutdown.rs::publish_discovery_phase` is also dead post-3d.2b (its only caller was `app/handle.rs`), excise it too — but keep `RecoveryMarker` (permanent).

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
