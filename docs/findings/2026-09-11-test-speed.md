# Finding: test speed (owner sequence item 3)

Branch `test-speed` over main `ab6b8077`. Design: `docs/plans/2026-09-11-test-speed-design.md`. Plan: `docs/plans/2026-09-11-test-speed-plan.md`. Ledger: `docs/plans/2026-09-11-test-speed-ledger.md`.

## What changed

- `cargo xtask test` has three tiers: `dev`, `dogfood`, `full`. Each is a fixed command list in `xtask/src/runner.rs`. `dev` builds `julie-server`, runs one `cargo nextest run --workspace` over everything except the dogfood set, then the ignored `tests::cli::` tests. `dogfood` ensures the fixture and runs the dogfood set. `full` is `dev` then `dogfood`.
- Deleted: the bucket manifest (`xtask/test_tiers.toml`, 52 buckets, 121 commands), tiers `nano`, `fast`, `smoke`, `system`, `benchmark`, `reliability`, the `changed` diff mapping, `inventory`, `bucket`, `list`, budgets, prebuild and warm/cold accounting, `--coverage`, `--timeout-multiplier`. `xtask/` went from 11,470 lines to 2,233.
- `.config/nextest.toml` caps the dogfood set at six parallel tests (test group `dogfood`). No environment variable.
- `Cargo.toml`: `julie-index` compiles at `opt-level = 1` in dev and test builds.
- `crates/julie-index/src/graph/reexports.rs`: `ReexportIndex` precomputes import ids and module paths once per `resolve`. Each unresolved `crate::` reference no longer walks every symbol.
- `src/service/client.rs`: `connect_or_start_within(paths, spawn, deadline)`; `connect_or_start` keeps the 10 s default.
- Tests: the two client tests use a 200 ms deadline; the race test polls `search_ready` and asserts the search result; the scenario apply step runs the CLI `--standalone`; the process fixture sets `JULIE_SERVICE_IDLE_SECS=2` for its children.
- Docs: CLAUDE.md and AGENTS.md test section cut from about 130 lines to about 65; README, TESTING_GUIDE, DEVELOPMENT, DEPENDENCIES, ADDING_NEW_LANGUAGES, TREE_SITTER_UPGRADES, TREE_SITTER_QUALITY_BAR (commands table), the ledger template, two hooks, the dead-code-audit skill, and one doc comment name only the new commands. The docs contract test in xtask enforces this.

## Before and after

Same machine (24 cores, 62 GB), same commands, warm, one run each unless noted. Before is main `ab6b8077`; after is `dd0ebe7a`.

| Measure | Before | After |
|---|---|---|
| Batch gate (`dev`) | 30 buckets, 72 serial commands: 50.5 s warm + 6.7 s prebuild | 3 commands, one workspace run: 12.2 s median of three (19.3, 12.2, 11.9) |
| Broad gate (`full`) | 47 buckets, 113 commands: 94.3 s warm, dogfood not included | 5 commands: 26.4 s, dogfood included |
| Dogfood (`search_quality` set) | 124 s (42 s fixture build + 81 s tests); median test 10.3 s | 14.6 s median of three with the fixture present; median test 1.76 s |
| One-file change gate | `changed` fell back to `dev`: 57 s | `dev`: 12 s |
| Tests run by the gates | 2131 of 2450 listed (319 in no bucket) | 2275 of 2275 listed (2206 dev + 69 dogfood); 29 ignored by design |
| Dogfood store open on the fixture (76,541 symbols, debug) | 8.4 to 9.7 s per test | 1.4 s (3.1 s from the index alone; 1.4 s with `opt-level = 1`) |
| Slowest test in the batch gate | 10.0 s (three fixed waits) | 7.2 s (`workspace_isolation_smoke`, real indexing) |
| Edit loop, one-line change, main crate test binary | 3.5 s | 3.5 s (unchanged; was never the cost) |
| xtask lines | 11,470 | 2,233 |
| Service graph load, julie workspace, release (report-only) | no measurement on main for this workspace; 318 ms for 32,720 symbols on nlohmann-json | 638 ms for 76,541 symbols (julie), 416 ms for 32,720 (nlohmann-json), 3,040 ms for 104,082 (Alamofire) |

Where the old time went: the runner ran 72 to 113 nextest commands one after another, each loading the 880 MB test binary for a small filtered set, so 24 cores idled (3.5x to 6.5x penalty). `changed` fell back to `dev` for any unmapped path. Each dogfood test paid 8.3 s of re-export resolution in an unoptimized build. Three tests waited out fixed 10 s deadlines. Compile time was not the cost: a no-op or one-line edit rebuilds the test binary in 3.5 s, a fresh worktree costs 33 s once, and the 160 s cold rebuild appears only when rustflags or the toolchain change.

## Tests that surfaced

319 tests ran in no bucket before this branch. Running them found:

- `tests::request_scenarios::test_scenario_2_refactoring_pipeline` failed on main. The apply step ran the CLI without `--standalone`, so it opened the index store the fixture's MCP child was still creating. Fixed in the test; the CLI flags were correct.
- `tests::fixtures::julie_db` (three tests) need the dogfood fixture. They are in the dogfood set now.
- `tests::request_transport_parity` (14 tests) and `tests::request_scenarios` (3 tests) each left a detached `julie-server service` running for the 30 minute default idle timeout. Seven gate runs left 129 processes holding 8.2 GB. The fixture now sets `JULIE_SERVICE_IDLE_SECS=2`; a parity test leaves zero services after 4 s.
- No other failures among the 319.

## Side findings

- `cargo xtask test inventory` was broken for the `-j 1` and `--test-threads 1` bucket commands (nextest `list` rejects both flags). The `-j 1` commands never serialized tests; `-j` is cargo's build-jobs flag. Both are gone with the manifest.
- The `service::process` tests the manifest ran with `--test-threads 1` passed at 24 threads in every run on this branch. No test group was added; add one with `max-threads = 1` if they flake.
- A test binary built in a worktree that shares `target/` bakes that worktree's path into `CARGO_MANIFEST_DIR`, and the main checkout reuses it because the fingerprint does not include the path. Fixture-path tests then fail until a rebuild. Touch `src/lib.rs` after switching checkouts, or build in the checkout you run in.
- The docs-contract grep still matches the historical ledger rows in `docs/TREE_SITTER_QUALITY_BAR.md:200-245` (dated 2026-05). They are history and stay.
- `.agents/skills/dead-code-audit/SKILL.md` is tracked and updated; the brief's note that `.agents` copies drift still applies to other skills there.

## Deferred

- `workspace_isolation_smoke` tests take 5 to 14 s each because they index a real workspace and start a service. They are now the batch-gate floor.
- `store_open` lock-wait tests hold a 5 s busy timeout each (three tests). A shorter timeout for tests would cut the floor further.
- Dogfood tests copy 185 MB each and run six at a time on tmpfs; with the store open at 1.4 s the cap now costs about 9 s of the 14.6 s. Raising `max-threads` trades tmpfs memory for wall time.
- Service graph load for Alamofire is 3 s in release. `Graph::build` still resolves every reference; the index removed the import walk but not the per-reference candidate scan.
- Other scenario and parity tests call the CLI without `--standalone` after `rpc` calls. They pass but could race under heavy load.
- The dashboard error buffer, partial vector backfill resume, and RSS follow-ups from item 2 are unchanged.
