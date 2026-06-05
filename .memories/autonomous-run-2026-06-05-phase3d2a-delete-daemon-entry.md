# Autonomous Execution Report - Phase 3d.2a (daemon entry kill)

**Status:** Complete
**Plan:** docs/plans/2026-06-05-julie-phase3d-delete-daemon-adapter.md (3d.2a — entry kill)
**Branch:** julie-rescue-phase3d2a
**PR:** https://github.com/anortham/julie/pull/35 (awaiting HUMAN merge — gated on 3c.3 live dogfood smoke)
**Base:** main @ 69f45178 → HEAD d87ba88b
**Tasks:** A1, A2 (workers), A3 (lead gate+review+PR) — 3/3 complete

## What shipped
- **A1 — daemon entry deletion** (`39dcc06d`): deleted `src/bin/julie-daemon.rs`, `src/daemon/cli.rs` (196 LOC); removed `julie daemon|stop|status|restart` from `cli.rs` + `main.rs`; removed `[[bin]] julie-daemon` from `Cargo.toml`; demoted `run_daemon` `pub`→`pub(crate)` (kept: `server.rs` + `daemon_lifecycle.rs` tests still drive it → deleted in 3d.2b). Entry-dev fixed `cli_tests.rs` (adapter→in_process rename) and re-pointed `in_process_boundary` bypassed-entry assertion.
- **A2 — legacy_migration write-path deletion** (`6311079c`): deleted `src/daemon/legacy_migration.rs` (261) + `src/tests/integration/legacy_migration.rs` (547) + its mod line; removed `pub mod legacy_migration;`; dropped `legacy_migration.rs` from the `in_process_boundary` §7 list. `fd_limit.rs` re-scoped OUT → 3d.2b (its `desired_nofile_soft_limit` is unit-tested in `app_test.rs`, a DaemonApp test 3d.2b deletes).
- **Wiring to 2-binary set** (`ed2dc092`, `5fd9c1ea`): `release.yml` rewritten (julie-server + julie-embedding-host; dropped all julie-daemon build/strip/codesign/archive/notes); `xtask dev_workflow` split-binary set 3→2 + 5 dev_link unit tests; `cli/mod.rs` help test; `wiring_a1_8` stale panic message.
- **F1 fix** (`6944029d`): see External review.
- **Ledger** (`d87ba88b`): 3d.2a verification ledger recorded in the plan doc.

## Judgment calls (non-blocking decisions made)
- **run_daemon kept as `pub(crate)` (A1)** — deleting it would break 5 test compile errors (`server.rs`/`daemon_lifecycle.rs` drive it). Verified zero PRODUCTION callers via rg; the demotion is the minimal entry-deletion. run_daemon dies with those tests in 3d.2b. Sound deviation from a literal "delete run_daemon" reading.
- **fd_limit.rs re-scoped A2→3d.2b** — lead deletion-safety check found `app_test.rs` (a DaemonApp/3d.2b test) unit-tests it; deleting now would force editing a 3d.2b file, breaking the split.
- **F1 message wording** — chose "requires the workspace registry, which is not available in the in-process server" over pointing at the dashboard or wiring registry.db. Rationale: registry.db wiring is explicitly 3d.3 scope; the honest minimal fix is to remove the dead-command reference, not over-promise a recovery path. Internal comments say "the in-process server does not wire a workspace registry" (matches codebase phase-comment style).
- **Branch gate = per-crate superset, not the xtask bucket tiers** — per [[feedback_prefer_fast_per_crate_gates]] the dev/system/reliability bucket runner flakes on per-bucket timeouts under cold-compile/load. Used `cargo nextest run -p julie --no-fail-fast -- --skip search_quality` (a SUPERSET: all 1659 tests incl. the integration tripwires the buckets/`--no-run` skip) + focused tripwire/F1 re-verify. Stronger coverage, no timeout flake.
- **2 deterministic pre-existing test failures NOT fixed in this PR** — both are out of daemon-entry scope: the `in_process` token-filename bug self-resolves in 3d.2b (harness deleted); the `get_symbols` dogfood stale-path is unrelated fixture maintenance. Fixing them here would muddy a clean teardown diff and risk a wrong gate-interpretation call (the token one is genuinely ambiguous: test-wrong vs product-wrong). Flagged, not deferred-by-neglect.

## External review (codex, gpt-5.5 high reasoning, escalation tier)
- **Findings:** 1 (verdict: needs-attention). Codex independently ran `cargo build --bins`, `cargo nextest run -p julie --no-run`, `cargo check -p xtask` — all clean; confirmed **no dangling deleted symbols** and consistent release/dev-link wiring.
- **Verified real, fixed:** 1 (commit `6944029d`)
  - **F1 — registry MCP errors referenced the removed `julie daemon` subcommand** (medium, conf 0.88). The in-process server builds `JulieServerHandler` with `daemon_db=None` (handler.rs:1066), so every registry-backed `manage_workspace` op (open/register/remove/refresh/stats) hit the no-registry branch and told users to "Start the daemon with `julie daemon`" — a command 3d.2a removes. Verified reachable: `registry_store_for_handler` returns None when `daemon_db.is_none()`; the in-process path is the only live MCP path post-cutover. Fixed across `open.rs`/`register_remove.rs`/`refresh_stats.rs` (5 sites) + dashboard register-error test updated + guard added (`!html.contains("julie daemon")`). Re-verified 15/15 @ 6944029d.
- **Dismissed:** 0
- **Flagged for your review:** 0

## Tests — branch gate @ HEAD d87ba88b (verify commit 6944029d)
- **3d.2a structural tripwires** (`in_process_boundary` ×2 + `wiring_a1_8`): **3/3 pass** — no-args serves in-process not daemon; §7 DAG files bypassed-not-deleted; e2e no-fork.
- **F1 path + tripwire re-verify** (`dashboard::projects_actions` + tripwires): **15/15 pass**.
- **Whole-crate superset** `cargo nextest run -p julie --no-fail-fast -- --skip search_quality`: **1655/1659 pass**, 4 failures — ALL pre-existing/environmental, none introduced by 3d.2a, all OUTSIDE the documented gate tiers:
  1. `daemon::embedding_host_optin::host_unavailable_when_health_not_ready` — **flake** (passes in isolation). Binary-resolution: spawns `julie-embedding-host` "next to current executable" (`target/debug/deps/`), missing under parallel load. [[project_e2e_daemon_tests_stale_binary_flake]]. Untouched by branch; passed at 5fd9c1ea.
  2. `daemon::restart_listener::daemon_reaps_idle_session...` — **flake** (passes in isolation). Documented reaper timing budget under load.
  3. `harness::in_process::test_in_process_daemon_starts_and_shuts_down` — **deterministic pre-existing bug** since `781884b7` (ancestor of base). Reads `daemon_mcp_token()`=`daemon-mcp.token`; transport writes `token_file()`=`daemon.token` (nothing writes daemon-mcp.token). Product fine (readiness probe passes). Self-resolves in 3d.2b (harness deleted).
  4. `tools::get_symbols_target_filtering_dogfood::test_target_minimal_mode_includes_body_for_child_symbols` — **deterministic pre-existing bug** since `5f086d10`. Hardcodes `src/tools/symbols/mod.rs` (line 37), refactored away. Unrelated dogfood-fixture path drift.
- **Compile authority**: `cargo nextest run -p julie --no-run` clean (only the deferred `fd_limit` dead-code warnings, expected per 3d.2b deferral).

## Blockers hit
- None. PR opened for human merge per the standing never-auto-merge constraint + the 3c.3 live-smoke merge gate (user-only).

## Files changed
20 files, +163 / −1208 (net −1045). Largest deletions: `legacy_migration` test (−547), `legacy_migration.rs` (−261), `cli.rs` daemon (−196). See `git diff --stat 69f45178..d87ba88b`.

## Next steps
- Review + merge PR #35 after the 3c.3 live dogfood smoke passes (rebuild `--release`, restart MCP client, confirm no daemon fork + no discovery.json, live round-trip).
- Follow-up (not 3d.2a): fix the dogfood `get_symbols` hardcoded stale path (`src/tools/symbols/mod.rs`); the `in_process` token-test bug self-resolves in 3d.2b.
- Cross-repo (carried from 3d.1): julie-plugin `run.cjs` + `update-binaries.yml` still reference julie-adapter → must move to julie-server.
- Then: Phase 3d.2b (server-core kill).
