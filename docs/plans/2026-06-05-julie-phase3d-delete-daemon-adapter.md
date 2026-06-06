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
4. `tools::get_symbols_target_filtering_dogfood::test_target_minimal_mode_includes_body_for_child_symbols` — **deterministic pre-existing test bug** (since `5f086d10`, ancestor of base). Hardcoded `src/tools/symbols/mod.rs` (line 37), which the Phase 2 split relocated to `crates/julie-tools/src/symbols/mod.rs`. **FIXED in `a9a29621`** (folded into this PR at the owner's request): repointed the path + updated the body-presence marker `resolve_workspace_filter`→`resolve_workspace_target` (the current call in `call_tool`'s body); RED→GREEN verified in isolation (44s).

External review: codex (gpt-5.5, escalation tier) @ 5fd9c1ea, verdict needs-attention, 1 medium finding (F1) — verified real, fixed @ 6944029d. No dangling deleted symbols (codex-confirmed clean builds).

#### 3d.2b-i Verification Ledger

Gate authority: same per-crate superset as 3d.2a ([[feedback_prefer_fast_per_crate_gates]]) — the bucket tiers timeout-flake under load. 3d.2b-i is xtask-only code (i1) + one redundant-test deletion (i2) + docs; **the julie test crate does not depend on xtask, so i1 cannot affect any julie test**, and i2 deleted a test (`embedding_host_optin`) unrelated to either gate failure.

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| i1: dev-restart parse contract (takes no args; `--force` rejected) | `cargo nextest run -p xtask dev_restart` | worker | 99c68a2c | pass (3/3) | no |
| i1: xtask compiles without `lifecycle`/`DaemonPaths` (cross-crate gate) | `cargo build --workspace --bins` | branch-gate (cross-crate) | 5c6d45a0 | pass | 2026-06-05 | no |
| i2: test crate compiles after `embedding_host_optin` deletion | `cargo nextest run -p julie --no-run` | branch-gate (compile) | 5c6d45a0 | pass (only deferred `fd_limit` dead-code warns) | 2026-06-05 | no |
| i2: surviving coverage intact (host opt-in + `ensure_ready` invariant) | `cargo nextest run -p julie --lib inprocess_embedding` | worker | 5c6d45a0 | pass (3/3) | no |
| Whole-crate regression: 3d.2b-i introduces no new failures | `cargo nextest run -p julie --no-fail-fast -- --skip search_quality` | branch-gate (superset) | 5c6d45a0 | 1654/1656 pass; 2 fails BOTH pre-existing (isolate-verified), none from 3d.2b-i | 2026-06-05 | no |
| Exit invariant: doomed daemon-controller + `spawn_embedding_init` have zero live kept-code callers | grep/`fast_refs` on `lifecycle::{stop_daemon,check_status,DaemonStatus}` + `spawn_embedding_init` | branch-gate (exit) | 5c6d45a0 | pass (only -ii delete-set test files + defs remain; xtask consumer severed; `spawn_embedding_init` zero outside `app/**`) | no |

**The 2 superset failures @ 5c6d45a0 — both pre-existing (identical to the 3d.2a ledger #2/#3), neither reachable by 3d.2b-i:**
1. `harness::in_process::test_in_process_daemon_starts_and_shuts_down` — deterministic pre-existing (FAILS in isolation): `in_process.rs:231` asserts the transport wrote a bearer token to `daemon.token`, but the post-3c.3 in-process server serves over rmcp stdio and writes no bearer token. Self-resolves in 3d.2b-ii (deletes the `InProcessDaemon` harness).
2. `daemon::restart_listener::daemon_reaps_idle_session...` — load flake (PASSES in isolation, single-threaded). Documented daemon-reaper timing budget under load.

External review: codex (gpt-5.5, escalation tier) @ 5c6d45a0, verdict **needs-attention**, 1 medium finding. Codex confirmed the positives: xtask `dev-restart` parse/dispatch coherent (no dangling `--force`/lifecycle consumer), and the deleted test's `spawn_embedding_init` refs are confined to the doomed `app/**` path. (It could not run `inprocess_embedding` — Unix-socket bind is EPERM in its sandbox — but that coverage is locally verified 3/3.)
- **F1 (medium, real-improvement, partially in-scope) — dev-loop docs contradict retained daemon architecture docs:** my i1 dev-restart edit (now "no daemon") sharpened an internal contradiction with CLAUDE.md/AGENTS.md's still-present daemon/adapter sections (esp. the wrong "Adapter mode (default): auto-starts the daemon ... bridges stdio to HTTP" at §Architecture-3, the `~/.julie/daemon.log` LOG-LOCATIONS block, and the `julie daemon` reference). **Fixed in-scope** (codex's own "label as legacy" recommendation): added "⚠️ in transition (Phase 3d)" banners to the Mode key-fact, the LOG-LOCATIONS daemon block, and the Architecture-#3 daemon/adapter bullets in BOTH files, correcting the most-wrong claims and pointing to the in-process reality. The **full prose rewrite** of those sections (against the final post-daemon state) is tracked to 3d.3 — doing it now would describe a transient half-deleted state that 3d.2b-ii/3d.3 re-touch.

---

## 3d.2b — Server-core kill (DETAILED — reshaped 2026-06-05 after the server-core mapping workflow)

> **SCOPE CORRECTION (mapping workflow, 6 agents, verified by direct grep):** the original outline ("delete app/**, http_transport, transport, mcp_session, **session.rs, lifecycle.rs**, shutdown_event, token_file, singleton, fd_limit, pools; surgery on handler.rs + workspace_session_attachment.rs") was **materially wrong**. The 3c.3 cutover removed the daemon *runtime* but left many daemon *types* wired into code we KEEP. **`session.rs` and `lifecycle.rs` are NOT deletable** — they hold types with live consumers. The pools cannot be deleted without de-typing the **dashboard** (kept until 3d.3), `health/checker`, and a shared test helper. There are two couplings the outline missed entirely (xtask cross-crate; Windows `shutdown_event`). Because of this, 3d.2b is **split into 3d.2b-i (prep, no deletion) + 3d.2b-ii (delete)** — owner-approved 2026-06-05, mirroring the 2a/2b pattern.

### 3d.2b Map (verified 2026-06-05)

**Verified LIVE consumers in KEPT code (the blockers, confirmed by direct grep):**
- `src/dashboard/state.rs` (kept until 3d.3) imports + stores `WorkspacePool` (16/148), `WatcherPool` (15/148), `SessionTracker`+`SessionPhaseCounts` (14/64/130), `LifecyclePhase`+`LifecyclePhaseKind`+`ShutdownCause` (13/47/61/134) — used across 5 `dashboard/routes/*` files.
- `src/handler.rs:35` + `src/handler/session_workspace.rs:4` use `SessionLifecyclePhase`/`SessionLifecycleHandle` from `daemon::session` (live in-process handler).
- `src/tests/daemon/embedding_host_optin.rs:15` (KEPT Phase-3b feature test) uses `daemon::app::spawn_embedding_init` (in deleted `app/helpers.rs`) + `WatcherPool`.
- `xtask/src/dev_workflow.rs:164/165/187` uses `lifecycle::check_status`/`DaemonStatus`/`stop_daemon` (cross-crate; a `-p julie --no-run` lib gate will NOT catch this break — must `cargo build` workspace-wide).
- `src/health/checker.rs:256` `build_control_plane` reads `WatcherPool` methods; `src/tests/helpers/workspace.rs:167/174` builds+assigns `WorkspacePool` (shared by many non-daemon tool tests); `src/handler/tool_metrics.rs:227` puts `workspace_pool` in `MetricsTask`.
- `drain_sessions` (mod.rs:54) has a LIVE caller in KEPT `shutdown.rs:101` (`drain_with_markers`, the permanent RecoveryMarker family) — **must stay**.
- `stop_daemon`'s `cfg(windows)` branch (lifecycle.rs:475) calls `shutdown_event::signal_shutdown` (green-on-mac / red-on-Windows hazard).

**Classification (3d.2b end-state):**
- **KEEP-untouched (→3d.3):** `discovery.rs` (readers go orphaned-but-compiling; excised in 3d.3 — the leader lock `DaemonLockGuard` is fully severed from the readers, verified), `pid.rs` (discovery.rs:51 imports it; →3d.3), `shutdown.rs` RecoveryMarker family + `drain_sessions`, `database.rs`, `connection_pool.rs`, `embedding_service.rs`, `project_log.rs`, `workspace_registry_store.rs`.
- **KEEP-trimmed:** `session.rs` (keep `SessionTracker`/`SessionPhaseCounts`/`SessionLifecyclePhase`/`SessionLifecycleHandle`; drop only provably-dead HTTP-admission members — verify with `fast_refs`), `lifecycle.rs` (keep `LifecyclePhase`/`LifecyclePhaseKind`/`ShutdownCause`; delete the daemon-controller `stop_daemon`/`check_status`/`DaemonStatus`/restart machinery once the xtask caller is gone), `mod.rs` (keep `drain_sessions`; delete `run_daemon`/`ShutdownArtifacts`/`perform_shutdown_sequence`/`drain_timeout`+consts/`binary_mtime`/`backfill_all_vector_counts`/`migrate_stale_workspace_ids` + `pub use app::{…}` + `pub mod app` + orphaned imports), `handler.rs` (drop pool fields/params/guards), `dashboard/state.rs`+routes (de-type off pools), `health/checker.rs`, `tests/helpers/workspace.rs`, `tool_metrics.rs`, `workspace_session_attachment.rs`.
- **DELETE whole files (3d.2b-ii):** `app.rs`+`app/{handle,helpers,runtime}.rs`, `http_transport.rs`, `transport.rs`, `mcp_session.rs`, `token_file.rs` (the daemon one — NOT `DaemonPaths::token_file()`), `singleton.rs`, `fd_limit.rs`, `shutdown_event.rs`, `workspace_pool.rs`, `watcher_pool.rs`.
- **DIES, its test DELETED as redundant (i2, not repointed):** `spawn_embedding_init` (only live caller `app.rs:486`, deleted in -ii). `embedding_host_optin.rs` is DELETED in 3d.2b-i — its host-opt-in + `ensure_ready` invariant is already covered against the surviving `acquire_in_process_embedding_provider` by `inprocess_embedding.rs`, and the `EmbeddingService` settle machine is covered by `embedding_service.rs` tests; repointing would only duplicate existing coverage.

### 3d.2b-i — PREP (sever the two app-INDEPENDENT couplings; ZERO server-core deletion, ZERO pool de-typing)

**Goal:** land the two consumer-severs that do NOT require `app.rs` to be gone first — the cross-crate xtask coupling and the kept-3b-feature-test coupling — so 3d.2b-ii is a pure atomic delete with no surprise cross-crate or kept-test breakage. **Architecture Quality:** preserves all kept interfaces; both severs are behavior-preserving (xtask stops calling daemon-control symbols the post-cutover server no longer runs; the 3b test repoints onto the in-process init path it should already exercise).

> **CORRECTION (verified by direct grep 2026-06-05 — why ONLY these two tasks):** the dashboard pool de-type, the handler+health pool surgery, and the shared-test-helper pool assignment CANNOT compile while `app.rs` is present, so they are **NOT** prep — they are atomic with the `app.rs` deletion and live in **3d.2b-ii**. Evidence: `app.rs:291` is the **sole** live `DashboardState::new_with_watcher_pool` caller, and `app.rs:298/299/326` (+ `app/helpers.rs:219/220`, `app/handle.rs:164/165`) are the **sole** live threaders of `Some(pool)` into the handler ctors. Every *other* live pool reference (`registry/cleanup.rs`, `registry/mod.rs:28`, `indexing/route.rs:74/300`, `workspace_session_attachment.rs:73/79/113`, `health/checker.rs:256`) only READS an already-stored `self.*_pool` field. Remove a pool field/param and `app.rs` stops compiling first → de-typing is indivisible from the app deletion. (The two tasks below are exactly the cross-cutting couplings the mapping workflow flagged as the headline misses; landing them ahead de-risks the big delete without touching the atomic block.)

#### Task i1 (strategy/lead — xtask + tooling contract): remove the obsolete daemon-control path
**Verified consumer set (grep 2026-06-05):** the ONLY xtask-crate consumers of `lifecycle::{check_status,stop_daemon,DaemonStatus}` are `dev_workflow.rs:164/165/187` (inside `run_dev_restart`), threaded through `cli.rs` (the `--force` flag) and `main.rs:166` (the call site). (`PidFile::check_status` in `pid.rs` is a DIFFERENT method — leave it. The `src/tests/daemon/**` + `src/tests/integration/daemon_lifecycle.rs` consumers are in-crate and die in -ii.)
**Decision (lead, owner-aligned with "remove the daemon-control PATH"):** KEEP the `dev-restart` subcommand as an **advisory** maintainer convenience — post-cutover it usefully tells the maintainer the world changed (in-process server is per-MCP-session; there is no shared daemon to soft-restart or SIGTERM) — but gut its daemon control and DROP `--force` (an advisory command has nothing to force; the owner bundled `--force` into the removal). Removing the whole subcommand is intentionally NOT done in this prep PR (it would expand into a CLI-surface removal); it stays a coherent advisory command.
- Modify: `xtask/src/dev_workflow.rs` — rewrite `run_dev_restart` to drop the `lifecycle` + `DaemonPaths` imports and all `check_status`/`stop_daemon`/`DaemonStatus` calls; new signature `run_dev_restart(out)` (no `force` param) prints advisory guidance (rebuild done → restart the MCP client / start a new session to load the new binary; the per-workspace leader lock means the first new session becomes the writer). Simplify/replace `DevRestartReport` accordingly. The `lifecycle::{stop_daemon,check_status,DaemonStatus}` symbols STAY in `lifecycle.rs` until -ii — i1 only removes the cross-crate *consumer*, so deleting them in -ii causes no xtask break.
- Modify: `xtask/src/cli.rs` — `parse_dev_restart_command` drops `--force` (now an unknown arg); update the usage/`bail!` strings; update the `cli_tests_dev_restart_*` tests (`defaults_to_soft_mode`→takes-no-args, drop `accepts_force_flag`, keep `rejects_unknown_args`).
- Modify: `xtask/src/main.rs:166` — update the `run_dev_restart` call (drop the `force` arg).
- Modify: `CLAUDE.md` AND `AGENTS.md` (edit BOTH directly — the sync pre-commit hook is non-executable here) — the maintainer dev-loop section (`dev-restart` / `dev-restart --force` descriptions) → advisory semantics.
- **NOTE — changed.rs is NOT touched in -i:** `xtask/src/changed.rs:756-764` maps the lifecycle *file-path strings* → the `lifecycle` *bucket*; both the files and the bucket survive until -ii, so removing that mapping here would break `changed` routing for still-present files. Moved to -ii (with the bucket + file deletion).
- **Acceptance:** workspace-wide `cargo build` green (xtask included); no `lifecycle::{stop_daemon,check_status,DaemonStatus}` *consumer* refs anywhere outside `lifecycle.rs` + the -ii-doomed `src/tests/daemon|integration` files; `cargo nextest run -p xtask` (cli tests) green; CLAUDE.md + AGENTS.md accurate.

#### Task i2 (lead — delete the kept 3b feature test as redundant, severing the doomed-symbol consumer)
**Decision (lead-verified 2026-06-05):** `spawn_embedding_init` is daemon-only glue (sole live caller `app.rs:486`, deleted in -ii). Its only unique behavior — routing the `JULIE_EMBEDDING_USE_HOST` env to the host-vs-`create_embedding_provider` path — has NO in-process equivalent: `acquire_in_process_embedding_provider` is **default-on** (no such env) and is ALREADY tested for the same three invariants by `src/tests/daemon/inprocess_embedding.rs` (ready=true→Some; ready=false→None via the `ensure_ready` hard gate; `JULIE_EMBEDDING_PROVIDER=none`→None). The `EmbeddingService` settle state-machine that `embedding_host_optin.rs` also drives is independently + comprehensively covered by `src/daemon/embedding_service.rs` (inline units: Ready/Unavailable/Timeout/multi-waiter) and `src/tests/daemon/embedding_service.rs`, and `EmbeddingService` SURVIVES (still used by `handler.rs:251`, `handler/embedding_init.rs`, `tools/workspace/indexing/embeddings.rs`). → Repointing would only DUPLICATE `inprocess_embedding.rs`; the correct sever is **deletion**, zero coverage loss.
- Delete: `src/tests/daemon/embedding_host_optin.rs` (the sole kept-code consumer of `spawn_embedding_init` + `daemon::watcher_pool::WatcherPool` outside the -ii delete-set).
- Modify: `src/tests/daemon/mod.rs:10` — remove `pub mod embedding_host_optin;`.
- **Acceptance:** `cargo nextest run -p julie --no-run` GREEN; grep on `spawn_embedding_init` shows the only remaining caller is `app.rs:486` (in the -ii delete-set); `inprocess_embedding.rs` (the surviving coverage) still passes.

**3d.2b-i branch gate:** workspace-wide `cargo build` (xtask included — the cross-crate lesson) + `cargo nextest run -p julie --no-run` + the per-crate superset (`cargo nextest run -p julie --no-fail-fast -- --skip search_quality`) per [[feedback_prefer_fast_per_crate_gates]]. **Exit invariant:** `fast_refs` on `lifecycle::stop_daemon`/`lifecycle::check_status`/`DaemonStatus` and on `spawn_embedding_init` shows ZERO live callers outside the -ii delete-set — proving -ii's lifecycle-symbol + app deletions cause no cross-crate or kept-test break. (Pool `fast_refs` are deliberately NOT an -i exit invariant — pools stay live through `app.rs` until -ii.)

### 3d.2b-ii — DELETE + atomic pool de-type (ABSORBS the old i2/i3/i4a pool surgery; detail when 3d.2b-i lands)

> **ATOMIC BLOCK (the correction):** deleting `app/**` and de-typing the handler/dashboard/health/test-helper off the pools is ONE indivisible step in ONE commit — `app.rs` is the sole live pool threader, so neither side compiles without the other (verified grep 2026-06-05; see the -i correction note). Do NOT attempt to stage the de-type separately.

Deletion order (tier-first, per the crate-split orphaned-filter lesson):
1. **Tiers first:** `xtask/test_tiers.toml` — the `transport` + `lifecycle` system buckets orphan COMPLETELY (every command names a deleted module) → delete those buckets from system+full; the `daemon` dev bucket SURVIVES (connection_pool/database/discovery/embedding_service/lock/pid/paths/inprocess_embedding remain; `embedding_host_optin` is gone — deleted in -i — so drop it from the bucket too if listed). Update the xtask manifest-contract snapshot (`manifest_contract_expected.rs`). **Also (deferred from i1):** drop the `src/daemon/lifecycle.rs`/`src/tests/daemon/lifecycle.rs`/`src/tests/integration/daemon_lifecycle.rs` → `lifecycle` mappings in `xtask/src/changed.rs:756-764` — the bucket + files are deleted here, so the path→bucket map dies with them.
2. **Atomic delete + pool de-type (ONE commit):**
   - **2a. Delete whole files:** `app.rs`+`app/**`, `http_transport.rs`, `transport.rs`, `mcp_session.rs`, `token_file.rs`, `singleton.rs`, `fd_limit.rs`, `shutdown_event.rs` + their `daemon/mod.rs` decls/re-exports + now-orphaned `use` imports.
   - **2b. De-type the dashboard** (was i2): `src/dashboard/state.rs` — remove `WorkspacePool`/`WatcherPool` imports (15,16), fields (148,149), ctor params, accessors (502,507); **KEEP** `SessionTracker`/`SessionPhaseCounts`/`LifecyclePhase`/`LifecyclePhaseKind`/`ShutdownCause` (those types survive 3d.2b). Update the (~5) `src/dashboard/routes/*.rs` callers of `workspace_pool()`/`watcher_pool()` to the "no pools" (dark) state — the dashboard is already dark post-cutover.
   - **2c. De-type the handler** (was i3): `src/handler.rs` — remove `watcher_pool` (277) + `workspace_pool` (287) fields; drop pool params from the ctors (~821/849/938/960/982); collapse the match guards at 96 (`workspace_pool.is_some() && daemon_db.is_some()`) and 114 (`workspace_pool.is_none()`) to the in-process reality (pools always `None`). `src/handler/tool_metrics.rs:227` — drop `workspace_pool` from `MetricsTask`. `src/handler/workspace_session_attachment.rs:73/79/113` — collapse the `Some(pool)` reads to the `None` path. `src/tools/workspace/commands/registry/cleanup.rs` (67/75/92/99) + `registry/mod.rs:28` + `indexing/route.rs:74/300` — collapse their `if let Some(pool) = handler.*_pool` reads to the `None` path (`fast_refs`-verify each is a pure reader, not another threader).
   - **2d. De-type health** (was i3): `src/health/checker.rs:256` (`build_control_plane`) — drop the `WatcherPool` `Some` arm (in-process is always `None`; the else-branch already computes local watcher state).
   - **2e. De-type the shared test helper** (was i4a): `src/tests/helpers/workspace.rs:167/174` — remove the `WorkspacePool::new(...)` construction + `handler.workspace_pool = Some(pool)` assignment. Confirm the dependent tool tests (global_targeting, primary_rebind, editing, refactoring, search/line_mode, workspace/*) still compile + a sample passes (they exercise the in-process path, which never needed the pool).
   - **2f. Delete the pool files:** `workspace_pool.rs`, `watcher_pool.rs` + their `daemon/mod.rs` decls.
3. **Delete symbols from kept files:** `run_daemon`+`ShutdownArtifacts`+`perform_shutdown_sequence`+`drain_timeout`(+`DRAIN_TIMEOUT_ENV`/`DEFAULT|MIN|MAX_DRAIN_TIMEOUT_SECS`)+`binary_mtime`+`backfill_all_vector_counts`+`migrate_stale_workspace_ids` from mod.rs (**KEEP `drain_sessions`**); the lifecycle.rs daemon-controller `stop_daemon`/`check_status`/`DaemonStatus`/restart machinery (**KEEP** `LifecyclePhase`/`LifecyclePhaseKind`/`ShutdownCause`); session.rs provably-dead HTTP-admission members (`fast_refs`-verify each).
4. **Delete/trim tests + mod lines:** `harness/in_process.rs` (resolves the pre-existing `daemon-mcp.token` test bug by deletion), `app_test.rs`, `restart_listener.rs`, `shutdown_ordering.rs`, `drain_timeout.rs`, `server.rs`, `integration/daemon_lifecycle.rs`, the http_transport/transport/mcp_session/workspace_pool/watcher_pool/singleton test files; **trim** (don't delete) `daemon/{handler,roots,database,session}.rs` to the surviving surface; **KEEP** `shutdown_drain_test.rs`. Remove the `pub mod` lines in `src/tests/daemon/mod.rs` and the INLINE integration block in `src/tests/mod.rs:152` (there is NO `src/tests/integration/mod.rs`).
5. **Flip the tripwire:** `src/tests/integration/in_process_boundary.rs` — delete `_bypassed_entry_points_still_compile` (the `Option<DaemonApp>` compile-tripwire), drop `http_transport.rs`/`transport.rs`/`singleton.rs` from `section7_files`; **KEEP** `pid.rs`/`search_compare.rs`/`migration.rs` entries (3d.3).
**Verify:** `cargo build` workspace-wide + `-p julie --no-run` + the branch gate (dev+system+reliability — runs the `in_process_boundary`/`wiring_a1_8` tripwires `--no-run` skips). Windows: deleting `stop_daemon`+`shutdown_event` removes the `cfg(windows)` coupling (no shim needed once the caller is gone).

> **LEAD MAPPING VALIDATION (single-lead exhaustive grep, 2026-06-05 — confirms the approved atomic scope; corrections folded into the task split below).** Re-verified every file/symbol classification above by direct `rg` + consumer analysis. The approved atomic-block scope is **correct and executed as-is** (no deferral; `app.rs` is the sole live pool threader → de-type is indivisible from app deletion, confirmed). Cleared landmines: `pid.rs` is NOT orphaned (`discovery.rs:51` consumes `PidFile` → stays, →3d.3); `singleton.rs` only name-drops `PidFile` in doc comments (clean delete); `drain_sessions` KEPT (`shutdown.rs:101` consumes it). **Six execution corrections to the detail above:**
> 1. **`shutdown_drain_test.rs` → DELETE, not KEEP** (line 311 is wrong): it references `HttpTransportServer`/`http_transport`/`drain_timeout` (all deleted) and tests daemon-HTTP-shutdown-drain, which is gone. It cannot compile post-deletion and tests a now-dead path.
> 2. **`workspace-runtime` system bucket (test_tiers.toml:507) ALSO orphans** — step 1's "transport + lifecycle buckets orphan" MISSED it. Its `tests::daemon::workspace_pool` + `tests::daemon::watcher_pool` commands name deleted modules → **trim those two commands**, KEEP `tests::daemon::workspace_cleanup` (if it survives the pool de-type) + `tests::workspace::registry` (julie-runtime, untouched). If `workspace_cleanup` also dies, the bucket fully orphans → drop from system+full.
> 3. **Full WorkspacePool/WatcherPool test delete-set:** `workspace_pool.rs`, `workspace_pool_eviction.rs`, `workspace_pool_shutdown.rs`, `watcher_pool.rs`, `watcher_pool_shutdown.rs` (five files) + **trim** the pool-coupled tests in `daemon/handler.rs` (24 pool refs), `daemon/workspace_cleanup.rs` (3), `daemon/roots.rs` (1), `daemon/database.rs` (2). **KEEP** `connection_pool_test.rs` + `symbol_db_pooled_test.rs` — they exercise the **SQLite connection pool** (`connection_pool.rs`, KEPT), NOT WorkspacePool/WatcherPool.
> 4. **Additional pure-deleted-runtime test files → DELETE** (each references only deleted symbols, grep-verified): `lifecycle.rs` (controller/`transition`/`version_gate`/`stale_binary`/`restart_handoff`), `shutdown_ordering.rs` (`perform_shutdown_sequence`/`ShutdownArtifacts`/`HttpTransportServer`), `admit_initialize_short_circuit.rs` (`DaemonLifecycleController`/`HttpTransportServer`/`binary_mtime`/`mcp_session`), `singleton_lock.rs`, `token_file_test.rs`, plus the already-listed `app_test.rs`/`restart_listener.rs`/`server.rs`/`drain_timeout.rs`/`http_transport.rs`/`transport.rs`/`mcp_session.rs`. **TRIM** `pid_file_format.rs` (drop its `binary_mtime` refs; pid.rs stays). **KEEP** `embedding_host_multi_session.rs` + `lock_test.rs` (zero real doomed refs — earlier broad counts were `::transport`/`singleton`-substring false positives).
> 5. **lifecycle.rs precise delete-set** (KEEP `LifecyclePhase`/`LifecyclePhaseKind`/`ShutdownCause` + their impls; DELETE the rest): `DaemonStatus`, `DaemonLifecycleController`+impl, `check_status`, `stop_daemon`, `write_daemon_state`, `write_daemon_phase`, `transition`, `version_gate_action`, `stale_binary_accept_action`, `stale_binary_disconnect_action`, `restart_handoff_action`, `RestartReason`, `RestartHandoffAction`, `LifecycleEvent`, `IncomingSessionAction`, `DisconnectLifecycleAction`, `RestartPendingTransition`. (Verified: every one of these has NO kept consumer — only deleted `app`/`mcp_session`/tests + the already-gutted xtask.)
> 6. **Dead-code fallout (lead-owned at the gate):** after deletion, `mod.rs::drain_sessions` (KEPT) and `shutdown.rs::{drain_with_markers, append_recovery_marker}` lose their callers (`perform_shutdown_sequence` + `shutdown_drain_test` deleted; `RecoveryMarker` itself survives via the kept dashboard `state.rs:140`). Add `#[allow(dead_code)]` with a "kept for 3d.3 recovery/dashboard" note rather than deleting them (the plan keeps the RecoveryMarker family for 3d.3). The lead resolves any residual `dead_code` at the atomic compile.

#### 3d.2b-ii Verification Ledger

Gate authority: same per-crate superset as 3d.2a/3d.2b-i ([[feedback_prefer_fast_per_crate_gates]]) — the bucket tiers timeout-flake under cold-compile/load, so the branch gate is `cargo nextest run -p julie --no-fail-fast -E 'not test(search_quality)'` (note the `-E 'not test(...)'` filter — `--skip` is NOT a valid nextest top-level arg). 3d.2b-ii is the **atomic** server-core delete + pool de-type, so the superset is the only gate that proves the de-typed handler/dashboard/health/test-helper still link against every kept consumer AND that the deleted daemon-runtime tripwires no longer compile-trip.

**Production fix folded in (the de-pool storage-anchor read-path regression):** removing `WorkspacePool` severed the shared-`~/.julie/indexes/` root that the pool used to supply to every workspace's storage resolution. The de-pool wired `in_process_index_root` into the WRITE paths (`handler.rs` force-reindex + normal init) but NOT the read-path resolver `workspace_index_dir_for`. Fix: `workspace_index_dir_for` now consults `self.in_process_index_root` FIRST (returns `<shared_indexes>/{workspace_id}`) before falling back to `workspace_storage_anchor()`, so rebound + secondary/reference workspaces keep the shared anchor instead of collapsing to the current workspace's project-local `.julie` tree. This mirrors the existing `anchor_override.parent().join(workspace_id)` resolution.

**8 daemon multi-workspace session/roots/swap lifecycle tests `#[ignore]`d (owner decision, Phase 3d.3 reason):** these exercise pool-backed multi-workspace session attachment + roots binding that cannot be rewired without 3d.3's `daemon.db → registry.db` registry rework. Marked `#[ignore = "daemon multi-workspace session/roots lifecycle (pool-backed); reworked in Phase 3d.3 registry rework"]` (counted in the 113 skipped). Files: `daemon/roots/{secondary_targets,startup_deferral,initial_binding,deferred_auto_index_sensitive}.rs`, `tools/editing/rewrite_symbol_tests/stateful.rs`, `tools/workspace/deferred_open.rs`.

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| De-pool storage-anchor read-path fix: rebound primary + reference routing keep the shared `~/.julie/indexes/` anchor (not project-local `.julie`) | `cargo nextest run -p julie -E 'test(/refresh_routing/) | test(/stale_index_detection::reconnect/)'` | affected-change | working tree atop f39467a9 | pass | 2026-06-05 | no |
| Compile authority (cfg(test) included; de-typed handler/dashboard/health/test-helper link against all kept consumers) | `cargo nextest run -p julie --no-run` | branch-gate (compile) | working tree atop f39467a9 | pass (clean) | 2026-06-05 | no |
| Whole-crate regression: atomic delete + pool de-type introduces no new failures; the 2 long-standing pre-existing failures self-resolve | `cargo nextest run -p julie --no-fail-fast -E 'not test(search_quality)'` | branch-gate (superset) | working tree atop f39467a9 | 1452/1452 pass (174 slow, 1 leaky), 113 skipped, 0 fail | 2026-06-05 (995.634s) | no |

> **Gate ran against the exact pre-commit working tree** (parent `f39467a9`, no edits since; team quiescent, no live `--agent-id`/`cargo-nextest` process) — content-identical to this PR's HEAD. Per the verification-ledger contract the evidence binds to that tree state.

**The 2 pre-existing failures from the 3d.2a/3d.2b-i ledgers are now GONE — both self-resolved by 3d.2b-ii's deletions, exactly as those ledgers predicted:**
1. `harness::in_process::test_in_process_daemon_starts_and_shuts_down` (deterministic `daemon-mcp.token` vs `daemon.token` harness bug) — **resolved by deleting** `src/tests/harness/in_process.rs` (the `InProcessDaemon` HTTP-transport harness has no post-cutover meaning).
2. `daemon::restart_listener::daemon_reaps_idle_session...` (load-flake) — **resolved by deleting** the daemon reaper/restart-listener machinery + its test.

Result: the first fully-GREEN superset of the 3d teardown — **1452/1452, 0 failures** (vs 1655/1659 @ 3d.2a and 1654/1656 @ 3d.2b-i).

**Atomic block executed as mapped** (single commit): deleted `daemon/app/**` + `{http_transport,transport,mcp_session,token_file,singleton,fd_limit,shutdown_event,workspace_pool,watcher_pool}.rs`; de-typed `handler.rs` + `handler/{tool_metrics,workspace_session_attachment}.rs` + `dashboard/state.rs` (+5 routes) + `health/checker.rs` + `tests/helpers/workspace.rs` off `WorkspacePool`/`WatcherPool` (kept `SessionTracker`/`SessionPhaseCounts`/`LifecyclePhase`/`LifecyclePhaseKind`/`ShutdownCause` → 3d.3); pruned the daemon-controller symbols from `lifecycle.rs`/`mod.rs` (kept `drain_sessions` + RecoveryMarker family via `#[allow(dead_code)]`); flipped the `in_process_boundary` tripwire + repaired the `transport`/`lifecycle`/`workspace-runtime` xtask buckets + manifest-contract snapshot; stubbed `xtask/src/search_matrix.rs` `run_baseline_async` to a `bail!` (rewire → 3d.3, absorbed). Net diff ≈ −16.5k LOC across 118 files.

External review: codex (gpt-5.5, escalation tier) pre-merge on `main..HEAD` @ d8a872c1, verdict **needs-attention**, 2 findings — BOTH verified real but **pre-existing in-process behavior inherited from the 3c.3 cutover, NOT regressions introduced by 3d.2b-ii** → FLAGGED for human review (not fixed: each requires 3d.3-class infrastructure or a product-intent decision the deletion PR should not make), tracked to 3d.3. No code changed → branch-gate evidence above stays valid.
- **F1 (HIGH, conf 0.88, real-improvement / pre-existing / FLAGGED) — cleanup treats active in-process workspaces as inactive:** with the pools gone, `WorkspaceCleanupActivity` hardcodes watcher_refs=0 / live_indexing=None / remove_runtime=true, and `attach_workspace_resources` no longer increments `session_count`, so `manage_workspace remove` (manual delete) isn't blocked for the current/active workspace. **Verified pre-existing:** at BASE `attach_workspace_resources` early-returned on `workspace_pool=None` *before* the increment, and `watcher_ref_count`/`live_indexing_reason`/`remove_runtime_if_inactive` were already no-ops for the in-process server (pool=None) — so the in-process runtime behavior is byte-identical pre/post-PR; the de-pool only deletes the dead daemon arms. **Impact lower than codex's "high":** the removed guards were *multi-session daemon interlocks* with no meaning in a single-session in-process server; the delete is user-initiated; only the derived index is removed (source untouched, recoverable by re-index). Proper hardening (teach cleanup about handler current/active/session-attached IDs, or restore an in-process session-activity marker) is a product-intent + session_count-semantics decision → 3d.3 (which rebuilds session_count + registry semantics).
- **F2 (MEDIUM, conf 0.92, real-improvement / pre-existing / FLAGGED) — target-workspace telemetry records primary source_bytes:** cross-workspace tool calls (e.g. target `fast_refs`/`get_context`) write a tool_call row tagged with the target `workspace_id` but `source_bytes` computed from the loaded primary DB. **Verified pre-existing:** the old metrics test set up a `WorkspacePool` and exercised the daemon resolution path; the shipping in-process server never had a `workspace_pool`, so it has recorded primary bytes since the 3c.3 cutover. The test rewire (`Some(target_bytes)`→`Some(primary_bytes)`) documents the actual in-process reality rather than hiding a new regression. Proper fix (carry a target DB handle/path into `MetricsTask`, or compute source_bytes in the wrapper from the target snapshot) is telemetry/dashboard-rework infrastructure → 3d.3.
- Positives codex confirmed: the storage-anchor read-path fix is correctly anchored for live `new_in_process` sessions (`in_process_index_root` set by the in-process server; `workspace_index_dir_for` resolves siblings under the shared index root). No other material findings.

---

## 3d.3 — Data + standalone dashboard (OUTLINE — detail when reached)

`daemon.db` → `registry.db` rename/migration; add `registry.db.daemon_state.started_at_unix` (the one new write-site from §11); build the standalone registry-reader dashboard (reads `registry.db` + per-workspace `symbols.db` projection_states + `~/.julie/recovery-*.json`; drops all mutation routes, SSE, CSRF, live session/indexing/embedding signals per the §11 DROP list, plus `dashboard/routes/events.rs` + `projects_actions.rs` which have no read-only analogue); G7 dual-write cleanup — `fast_refs` the per-workspace `SymbolDatabase.tool_calls` read methods, and if the read-side is dead, delete `database/search_compare.rs` + the dual-write and make the central copy the source of truth; delete `src/migration.rs` (owner decision: delete now) + its `pub mod migration;` in `lib.rs`.

**Absorbed from 3d.2b-ii (unmapped `WorkspacePool` consumer surfaced during execution):** `xtask/src/search_matrix.rs` `run_baseline_async` was a multi-workspace search-bakeoff that iterated the daemon registry (`DaemonDatabase` + `WorkspacePool` + `new_with_shared_workspace`). 3d.2b-ii **stubs it to a clear `bail!`** (dev-only eval tooling, no branch-gate dependency, and its proper rewire is coupled to this phase's `daemon.db → registry.db` rename — rewiring against `daemon.db` now would be redone here). 3d.3 rewires it onto the in-process single-workspace path (`new_in_process` per resolved repo root) reading from `registry.db`, restoring the baseline/ablation command.

**Absorbed from 3d.2 (per the verified map):** excise the `discovery.json` reader bodies in `daemon/discovery.rs` (`DiscoveryRecord::for_current_process`, `DiscoveryFile::read_and_validate` — their only callers die with the server in 3d.2b; the leader-lock `DaemonLockGuard` itself does NOT use them), then remove the `use crate::daemon::pid::{...}` import (discovery.rs:51) and finally **delete `src/daemon/pid.rs`** (held out of 3d.2 because discovery.rs imports it) + its `section7_files` tripwire entry + pid tests. This is the last write-path file. If `daemon/shutdown.rs::publish_discovery_phase` is also dead post-3d.2b (its only caller was `app/handle.rs`), excise it too — but keep `RecoveryMarker` (permanent).

**Tracked from 3d.2b-ii codex pre-merge review (both pre-existing in-process behaviors since the 3c.3 cutover, surfaced when the dead daemon arms were deleted — fix here where the registry/session/telemetry semantics are rebuilt):**
- **F1 — in-process workspace-cleanup safety:** the cleanup activity guards (`watcher_ref_count`/`live_indexing_reason`/`remove_runtime_if_inactive`) are permanent no-ops in-process, and `attach_workspace_resources` no longer increments `session_count`, so `manage_workspace remove` / dashboard delete / auto-prune have no active-workspace interlock. When 3d.3 reworks `session_count` + registry semantics, decide the in-process safety model: either teach `cleanup_activity_for_handler` about the handler's current-primary + session-attached workspace IDs (block manual delete / auto-prune for the live set), or add a "refuse to remove the current primary" guard in `handle_remove_command`. This is a product-intent call (should removing your own current workspace be blocked?) — confirm with the owner during 3d.3.
- **F2 — cross-workspace metrics attribution:** `run_metrics_writer` computes `source_bytes` (and inserts the tool_call row) from the loaded primary workspace DB even when the tool call targets a different `workspace_id`. When 3d.3 rebuilds the telemetry/dashboard consumer, carry a target DB handle/path into `MetricsTask` (or compute `source_bytes` in the wrapper from the resolved target snapshot) so cross-workspace tool calls attribute bytes to the target DB. Update `test_fast_refs_target_workspace_uses_requested_binding_for_metrics_attribution` back to asserting `target_bytes` once the fix lands.

**Docs (tracked from 3d.2b-i codex F1):** rewrite the now-legacy daemon/adapter architecture prose in `CLAUDE.md` + `AGENTS.md` to describe the in-process-only reality — the `🚨 LOG LOCATIONS` daemon block, the `Architecture Principles #3` daemon/adapter/stale-binary/catch-up/stdio bullets, the `Mode` key-fact, and the §8 "in daemon mode the embedding provider is shared" note. 3d.2b-i added interim "⚠️ in transition (Phase 3d)" banners to those sections; replace them with the final description once the daemon is fully deleted here (so the rewrite is done once against the end state, not churned per sub-PR).

---

## Verification Strategy

**Project source of truth:** `CLAUDE.md` / `AGENTS.md` (xtask tiers), `docs/TESTING_GUIDE.md`, `RAZORBACK.md`.

**Worker red/green scope:** `cargo nextest run -p julie --lib <exact_test_name>` (narrow, named, ≤2 runs RED→GREEN). For compile-only confirmation after a deletion, `cargo nextest run -p julie --no-run` (NOT `cargo check` — it skips cfg(test)).

**Worker ceiling:** the named test(s) for the touched area + a `--no-run` compile. Workers do **not** run `cargo xtask test changed`/`dev`/`system`/`reliability` — the lead owns those.

**Worker gate invariant:** each worker states what its assigned test proves (e.g. "in_process_boundary asserts the post-3d.1 §7 set: adapter/http_client gone, server files still present").

**Lead affected-change scope:** `cargo xtask test changed` during the local loop after a coherent batch.

**Branch gate (lead, once per sub-PR before handoff):** `cargo xtask test dev` GREEN **with the daemon SERVER tests still present in 3d.1** (only the adapter/client surface is removed in 3d.1) + `cargo xtask test system` + `cargo xtask test reliability`. These tiers run the daemon/adapter integration tripwires that `--no-run` does NOT execute — always run the full tier before declaring a deletion PR done (the Phase-2b lesson). **3d.2a addendum (apply to 3d.2b):** the per-crate superset `cargo nextest run -p julie --no-fail-fast -- --skip search_quality` is the authoritative crate-wide gate (the bucket tiers timeout-flake under load per [[feedback_prefer_fast_per_crate_gates]]); root-cause every failure as flake / pre-existing / introduced.

**3d.2b cross-crate gate (NEW — the mapping workflow's headline miss):** 3d.2b touches `lifecycle.rs`, which `xtask/src/dev_workflow.rs` consumes. A `-p julie --no-run` lib gate will NOT catch an xtask-crate compile break. The branch gate for BOTH 3d.2b-i and 3d.2b-ii MUST include a workspace-wide `cargo build` (or `cargo build -p xtask`). Windows: `stop_daemon`'s `cfg(windows)` branch calls `shutdown_event` — once `stop_daemon` is deleted (3d.2b-ii) the coupling is gone, but verify no other `cfg(windows)` consumer of `shutdown_event` survives (macOS CI won't catch a Windows-only break).

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
