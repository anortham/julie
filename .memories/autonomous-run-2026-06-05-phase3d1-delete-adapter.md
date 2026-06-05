# Autonomous Execution Report - Phase 3d.1: Delete adapter + daemon-IPC client

**Status:** Complete
**Plan:** docs/plans/2026-06-05-julie-phase3d-delete-daemon-adapter.md
**Branch:** julie-rescue-phase3d1
**PR:** https://github.com/anortham/julie/pull/34
**Duration:** ~2h (work commits 08:12→09:38, branch gate 09:40→10:09)
**Phases:** 1/3 sub-PRs of Phase 3d (3d.1 shipped; 3d.2 + 3d.3 remain)
**Tasks:** 2/2 complete (T1, T2; T3 merged into T2 as one atomic compile-coupled task; T4 = lead branch-gate)

## What shipped
- **T1 — CLI → standalone-only:** `run_cli_tool(command, cli_workspace, _standalone)` now calls `command.validate_standalone()?` then `run_standalone()` unconditionally; the `_standalone` flag is ignored. Deleted `src/cli_tools/daemon.rs` (DaemonClient, run_via_daemon, ensure_daemon_ready, try_connect_daemon, DaemonCallError — 190 lines). `CliExecutionMode` collapsed to `Standalone` only. (`aaa831a9`)
- **T2 — adapter deletion + tripwire + bucket (atomic):** deleted `src/adapter/{mod,forwarder,http_stdio,launcher}.rs`, `src/bin/julie-adapter.rs`, `src/daemon/http_client.rs`, and `src/tests/adapter/{mod,http_stdio,launcher,retry_resilience}.rs`. Removed `pub mod adapter;` (lib.rs), `pub mod http_client;` (daemon/mod.rs), the `[[bin]] julie-adapter` (Cargo.toml). Flipped the T12 tripwire in `in_process_boundary.rs` (dropped `crate::adapter::run_adapter` + adapter/http_client files from the bypassed-entry-point + section7 file sets; kept `daemon::cli::start_daemon`, http_transport, transport, legacy_migration for 3d.2). Removed the `tests::adapter` command from the `transport` bucket in test_tiers.toml. (`7d483b1a`)
- **T2 fix — obsolete E2E spawn tests:** deleted 5 tests that spawn `binary_path("julie-adapter")` (test_e2e_julie_adapter_direct_spawns_daemon, _attaches_to_running_daemon, _daemon_survives_adapter_exit, _adapter_exits_when_daemon_dies, test_e2e_legacy_daemon_attached_by_adapter) + unused helpers, in wiring_a1_8.rs + legacy_migration.rs. Kept Test #1 (in-process no-args) + round_trip_initialize + 9 legacy_migration tests. (`c3e7e20a`)
- **gate fix — manifest contract snapshot:** updated `manifest_contract_expected.rs` so the `transport` bucket snapshot matches test_tiers.toml (removed the `tests::adapter` line; notes → "HTTP transport coverage (backed by daemon transport tests)"). (`d9cc118d`)
- **Post-3d.1 binary set:** julie-server (in-process MCP entry), julie-daemon (kept until 3d.2), julie-embedding-host. julie-adapter gone.

## Judgment calls (non-blocking decisions made)
- `src/tests/integration/in_process_boundary.rs` — Merged T2 (adapter delete) + T3 (tripwire flip) into ONE atomic task because in_process_boundary.rs references `crate::adapter::run_adapter`; deleting the adapter without editing the tripwire in the same commit would not compile. Atomic > sequenced when compile-coupled.
- `src/cli_tools/commands.rs:378-396` — Chose to rewrite `WorkspaceArgs::validate_standalone()` to redirect users to the `manage_workspace` MCP tool (not to implement workspace registry ops in standalone CLI). Registry ops belong to the in-process server's shared registry; standalone CLI has no business owning them. (codex F2.)
- `.github/workflows/release.yml` — Chose to add `julie-embedding-host` to the release build/bundle while fixing the adapter removal (codex F1), since the in-process server locates that sibling binary at runtime; shipping server-without-host would break semantic embeddings for end users.
- `.codex/config.toml` — Left UNcommitted (machine-local) per standing constraint; flagged F4 to the user rather than "fixing" it in the PR.

## External review (codex, adversarial)
- **Findings:** 4
- **Verified real, fixed:** 3 (commit `cc6b610c`)
  - **F1 (critical)** — release.yml still ran `cargo build ... --bin julie-adapter` then stripped/signed/archived/documented it; `Cargo.toml` no longer defines that bin, so a tagged release would fail before producing artifacts. Fixed: workflow now builds `julie-server` + `julie-daemon` + `julie-embedding-host` across build/strip/codesign/archive/release-notes/Quick-Start/Verify. YAML validated.
  - **F2 (high)** — CLI rejected `workspace open/register/remove/refresh/stats` with "requires daemon mode. Start the daemon with `julie daemon`" — but the daemon client path was deleted, so starting the daemon can't help. Fixed: message now points at the `manage_workspace` MCP tool ("workspace registry operations run in the in-process server"). Renamed the test to `..._not_available_via_cli` with assertions on the new message.
  - **F3 (high)** — `xtask dev-link` split-binary list still expected `julie-adapter`; clean build would fail on the missing binary (or link a stale one). Fixed: `split_binary_names()` returns `[julie-server, julie-daemon, julie-embedding-host]` (+ .exe variants).
- **Dismissed:** 0
- **Flagged for your review:** 1
  - **F4 (low)** — working-tree `.codex/config.toml` (untracked) spawns `target/release/julie-adapter`, which no longer exists on a clean build. Why flagged not fixed: it's machine-local config under the never-commit constraint. Repoint it at `target/release/julie-server` before the live smoke.
- codex also suggested (next_steps) a static grep tripwire for deleted binary names in the release workflow — real improvement, out of scope for 3d.1; candidate for a follow-up.
- Note: codex/claude do not surface per-request token counts in JSON output (only gemini does), so no token cost line for this review.

## Tests
- branch-gate ALL_GREEN @ `cc6b610c`: dev (37 buckets) exit=0 ~21m · system (7 buckets) exit=0 ~5m · reliability (3 buckets) exit=0 ~3m. Fresh bins (julie-server + julie-daemon + julie-embedding-host) compiled exit=0.
- Prior gate @ `d9cc118d` was also ALL_GREEN; re-ran the full gate after the codex-fix commit (`cc6b610c`) for a clean single-SHA ledger entry rather than reusing stale evidence.

## Verification ledger
| Scope | Invariant | Command | Commit | Result | Time |
|-------|-----------|---------|--------|--------|------|
| branch-gate | full dev+system+reliability GREEN; bins compile; no adapter regressions | cargo build (3 bins) + cargo xtask test dev + system + reliability | cc6b610c | ALL_GREEN (dev=0 sys=0 rel=0) | 2026-06-05 09:40–10:09 |
| branch-gate (superseded) | same | same | d9cc118d | ALL_GREEN (pre-codex-fix; invalidated by cc6b610c) | 2026-06-05 08:55–09:25 |
| worker compile | -p julie cfg(test) compiles after adapter delete | cargo nextest run -p julie --no-run | 7d483b1a | OK | 2026-06-05 |

## Blockers hit
None. **Merge precondition (not a blocker):** PR #34 merge is gated on the user-only 3c.3 live dogfood smoke — the in-process cutover (#33) needs a real MCP-client round-trip confirmation before deleting the adapter fallback.

## Files changed (git diff --stat c3c03816..cc6b610c)
- 30 files changed, +451 / −4375 (net ~3.9k LOC of adapter/IPC removed)
- Deletions: src/adapter/{mod,forwarder,http_stdio,launcher}.rs, src/bin/julie-adapter.rs, src/daemon/http_client.rs, src/cli_tools/daemon.rs, src/tests/adapter/{mod,http_stdio,launcher,retry_resilience}.rs
- Modifications: src/cli_tools/{mod,commands}.rs, src/main.rs, src/lib.rs, src/daemon/mod.rs, Cargo.toml, src/tests/{mod,cli_execution_tests}.rs, src/tests/daemon/app_test.rs, src/tests/integration/{in_process_boundary,legacy_migration,wiring_a1_8}.rs, xtask/{src/dev_workflow.rs,test_tiers.toml,tests/support/manifest_contract_expected.rs}, .github/workflows/release.yml
- Adds: docs plan + 2 goldfish checkpoints

## Next steps
- Review + merge PR: https://github.com/anortham/julie/pull/34 (after 3c.3 live smoke)
- Repoint local `.codex/config.toml` → `target/release/julie-server` (codex F4)
- Cross-repo follow-up before next release: julie-plugin `run.cjs` (spawns julie-adapter) + `update-binaries.yml` (hardcoded binary list) must be updated to julie-server
- Phase 3d.2: collapse daemon entry — trim cli.rs, tear down HTTP daemon server (http_transport) + legacy_migration→singleton→pid write-path, decide julie-daemon bin
- Phase 3d.3: daemon.db→registry.db + standalone dashboard reader (Option B) + G7/search_compare cleanup + delete migration.rs
