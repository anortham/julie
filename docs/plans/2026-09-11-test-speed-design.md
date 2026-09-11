# Test speed design

Owner sequence item 3. "We spend 90 percent of our time waiting on tests."
Brief: `.memories/briefs/machine-service-one-rust-service-per-machine-repla.md`.

## Baseline (main `ab6b8077`, 24 cores, 62 GB, btrfs home, tmpfs `/tmp`)

Every number below is one warm run on this machine unless the row says otherwise.

| What | Command | Wall |
|---|---|---|
| No-op or one-line edit, main crate test binary rebuild | `touch src/lib.rs && cargo test --no-run --lib -p julie` | 3.5 s |
| Fresh worktree sharing `target/`, first test build | same, in `.worktrees/probe` | 33 s |
| Full dependency rebuild after a fingerprint change (rustflags, `-Z` flags) | `cargo nextest run --lib …` | 163 s |
| All workspace test binaries, build only | `cargo test --no-run --workspace` | 17 s |
| Whole workspace, one nextest call, 2355 tests in 34 binaries | `cargo nextest run --workspace -E 'not test(/search_quality/) and not test(/dogfood/)'` | 14.5 s |
| Main crate only, one nextest call, 1183 tests | `cargo nextest run --lib -p julie -E …` | 18 s |
| `dev` tier, 30 buckets, 72 serial commands | `cargo xtask test dev` | 50.5 s warm + 6.7 s prebuild |
| `full` tier, 47 buckets, 113 serial commands (ledger, 2026-09-11) | `NEXTEST_TEST_THREADS=6 cargo xtask test full` | 94 s warm |
| `changed` for one file with no mapping (`src/service/client.rs`) | `cargo xtask test changed` | 57 s (falls back to `dev`) |
| `dogfood` tier | `NEXTEST_TEST_THREADS=6 cargo xtask test dogfood` | 124 s = 42 s fixture build + 81 s tests |
| search-quality tests alone, 64 tests, 6 threads | `cargo nextest run --lib -p julie search_quality tests::fixtures::julie_db` | 81 s, median test 10.3 s |
| Same, with `opt-level = 1` on `julie-index` only | same | 29 s (one-time rebuild 32 s) |
| clippy, stale after a worktree build / incremental | `cargo clippy --workspace --all-targets` | 60 s / 4.8 s |

Coverage facts from `cargo nextest list --workspace` against `xtask/test_tiers.toml`:

- 2450 tests listed. 319 (13 percent) are in no bucket, so no tier ever runs them. The eight `store_open` tests item 2 added are among them. 113 tests sit in more than one bucket.
- One uncovered test already fails on main: `tests::request_scenarios::test_scenario_2_refactoring_pipeline` (CLI `edit` apply step exits 1). Three `tests::fixtures::julie_db` tests need the dogfood fixture and fail without it.
- The manifest changed 16 times since 2026-08-01. `cargo xtask test inventory` is broken for the `-j 1` and `--test-threads 1` commands. The `-j 1` commands do not serialize tests at all; `-j` is cargo's build-jobs flag.
- Declared `expected_seconds` for `dev` sum to 546 s against 50 s measured, so the `changed` budget check trips on stale numbers.

## Where the time goes

1. **Serial bucket execution.** The runner runs 72 to 113 nextest commands one after another. Each loads the 880 MB test binary and runs a small filtered set, so 24 cores idle. The same tests in one nextest call take 14.5 s. That is a 3.5x (dev) to 6.5x (full) penalty, and agents run these tiers many times per task (`changed`, then `dev`, then `full`).
2. **`changed` falls back to `dev` for any unmapped path.** One edit in `src/service/` costs the full 57 s tier. The mapping is 1169 lines of policy that a 14.5 s whole-workspace run makes unnecessary.
3. **Dogfood store open.** Each search-quality test copies the 185 MB fixture (0.05 s) and then opens the store. The open runs `Graph::build` re-export resolution in an unoptimized build: 8.3 s per test for 76,541 symbols (4,201 imports). `direct_targets` scans every import for every unresolved reference and allocates a `module_path` vector per check. The live service pays the same cost in release (`graph_load_millis` 318 for 32k symbols).
4. **Fixed waits set the floor.** Three tests wait 10 s: `race_condition::test_search_after_indexing_complete` sleeps 10 s, and two `service::client` tests wait out the full 10 s `connect_or_start` deadline for a hook that never starts.
5. **Not the cause.** The edit loop is 3.5 s. The linker is lld and links in 0.9 s. Incremental compilation works. A fresh worktree costs 33 s once, not 130 s. The 163 s cold number only appears when rustflags or the toolchain change.

## Design

Run everything, once, in parallel. Delete the bucket system.

### Commands after the change

| Command | Runs | Expected wall |
|---|---|---|
| `cargo nextest run --lib <name>` | one test, unchanged edit loop | 3.5 s + test |
| `cargo xtask test dev` | `cargo build -p julie --bin julie-server`, then one `cargo nextest run --workspace` excluding the dogfood set, then the ignored `tests::cli::` set with `--run-ignored only` | about 20 s warm |
| `cargo xtask test dogfood` | fixture ensure, then one nextest call over `search_quality`, `fixtures::julie_db`, and `dogfood` tests | about 30 s after the profile change, plus 42 s when the fixture is rebuilt |
| `cargo xtask test full` | `dev` then `dogfood` | about 50 s |

`nano`, `fast`, `smoke`, `system`, `benchmark`, `reliability`, `bucket <name>`, `list`, `inventory`, `changed`, and `--scale` go away. The `dev` run is faster than today's `fast` bucket set plus prebuild, so there is no smaller tier to keep.

The dogfood filter lives in one place, a nextest filterset string in xtask, used positively by `dogfood` and negated by `dev`. Thread cap for dogfood: a `[test-groups]` entry in `.config/nextest.toml` with `max-threads = 6` for the dogfood filterset, so the tmpfs cap no longer depends on an environment variable.

### Deletions (net negative)

- `xtask/test_tiers.toml` (613 lines) and the manifest, bucket, tier, budget, prebuild, inventory, and changed modules under `xtask/src/` with their tests. Those modules plus the runner and their tests are 7,201 lines today; the runner that stays runs a fixed list of commands and prints wall time per command. The xtask contract tests that do not touch tiers (dispatch, docs contract, extractor dependency, lean deps, toolchain) stay.
- CLAUDE.md and AGENTS.md sections on tiers, buckets, `changed`, OverBudget, warm/cold accounting, and the subagent bucket rules. The subagent rule that survives: run only your exact test; the lead runs `dev` once per batch.
- `docs/TESTING_GUIDE.md` tier tables. The verification ledger template keeps its columns; scope labels become `worker-red-green`, `dev`, `dogfood`, `full`, `live`.
- Hooks or skills that name a deleted command (checked during the plan).

### Product-side fixes with numbers

- `[profile.dev.package.julie-index] opt-level = 1` in `Cargo.toml`. Measured: search-quality 81 s to 29 s. Guard: none needed; the dogfood wall is the guard.
- Re-export resolution: compute each import's module path once and index imports by module, so `direct_targets` looks up a qualifier instead of scanning 4,201 imports per reference. Measure `Graph::build` on the fixture before and after in debug and release. Target: under 1 s debug on the fixture. This also cuts service startup and reopen time. If the measured gain is under 2x on the fixture, drop it and keep the profile change alone.
- `connect_or_start` takes its deadline as a parameter; the two client tests pass 200 ms. The race-condition test polls readiness instead of sleeping 10 s. Floor drops from 10 s to the 5 s lock-wait tests.

### Tests that surface

- The 319 uncovered tests run in `dev`. The four known failures get fixed first: the three fixture tests move under the dogfood filter, and `test_scenario_2_refactoring_pipeline` is diagnosed and fixed or deleted with the reason recorded.
- Any new failure among the 319 is a real regression or a rotten test. Each gets a fix or a deletion with its reason in the finding. No `#[ignore]` to make the run green.

### Serialization

The whole-workspace run passed twice at 24 threads, including the `service::process` tests the manifest ran with `--test-threads 1`. No test group is added for them. If they flake in the gate, add a nextest test group with `max-threads = 1` for `test(/service::process/)` and record the flake.

### Acceptance criteria

- [ ] `cargo xtask test dev` runs every non-dogfood test in the workspace in one nextest call and finishes under 25 s warm on this machine, three runs recorded.
- [ ] `cargo xtask test dogfood` finishes under 40 s warm with the fixture present, three runs recorded.
- [ ] `cargo xtask test full` equals `dev` then `dogfood`, and passes on the branch.
- [ ] `cargo nextest list --workspace` count equals the sum of tests the two filters select. Zero uncovered tests.
- [ ] `test_scenario_2_refactoring_pipeline` passes or is deleted with the reason in the finding.
- [ ] No test sleeps or waits 10 s; slowest test in `dev` under 6 s.
- [ ] `Graph::build` on the fixture: before and after numbers in debug and release recorded in the finding.
- [ ] `xtask` line count is lower than on main. CLAUDE.md, AGENTS.md, TESTING_GUIDE.md, hooks, and skills name no deleted command.
- [ ] Finding at `docs/findings/2026-09-11-test-speed.md` with the before and after table, same commands, same machine.

## Rejected

- **Keep buckets, run them in parallel, fix the mapping.** Keeps 3,800 lines of runner and mapping to maintain, plus the manifest that already drifted 16 times in six weeks and left 13 percent of tests unrun. nextest already schedules tests across cores better than a bucket runner can.
- **Only tune (profile, waits) and keep tiers.** Leaves the 3.5x to 6.5x serial penalty and the coverage gap.
- **Split `src/tests` into an integration-test crate to cut compile time.** The measured edit loop is 3.5 s; compile time is not the cost.
- **Share one opened fixture store across search-quality tests.** Cheaper to make the open fast; the graph fix helps the product too.

## Architecture impact

xtask loses its manifest, policy, and inventory modules and becomes a thin command list. No product module boundaries change; `connect_or_start` gains a deadline parameter and `reexports.rs` gains an import index built once per graph load.
