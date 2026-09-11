# Test Speed Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development whenever delegation is available and permitted, including for one task; serialize dependent tasks. Use razorback:executing-plans only when delegation is unavailable or the user/session explicitly selected single-agent execution.

**Goal:** Cut the time agents wait on tests by running the whole workspace in one nextest call, deleting the bucket runner, and removing the measured fixed costs (dogfood graph build, 10 s waits).

**Architecture:** xtask keeps `dev`, `dogfood`, and `full` as fixed command lists over `cargo nextest run --workspace` with one shared dogfood filterset. The manifest, tiers, budgets, prebuild accounting, `changed` mapping, and inventory are deleted. Product changes: `connect_or_start_within` with an explicit deadline, and a re-export index built once per graph load.

**Tech Stack:** Rust, cargo-nextest 0.9.143 (filtersets, `.config/nextest.toml` test groups), cargo profiles.

**Architecture Quality:** Approved shape: xtask becomes a thin command list (`xtask/src/main.rs` + `xtask/src/runner.rs` + `xtask/src/cli.rs`) with no manifest. `crates/julie-index/src/graph/reexports.rs` gains a `ReexportIndex` built once in `resolve` and passed down; `resolve_reexport` keeps its result contract. Risk: the 319 previously unrun tests may expose more than the four known failures; each gets a fix or a recorded deletion, never `#[ignore]`.

Design: `docs/plans/2026-09-11-test-speed-design.md` (baseline table and rejected options).

## Global Constraints

- Dogfood filterset, one string, used positively by `dogfood` and negated by `dev`: `test(/search_quality/) | test(/fixtures::julie_db/) | test(/dogfood/)`.
- `dev` must run `cargo build -p julie --bin julie-server` first; `tests::request_transport_parity` and friends spawn `target/debug/julie-server`.
- Ignored tests: `dev` also runs `cargo nextest run --lib -p julie --run-ignored only -E 'test(/tests::cli::/)'`; `dogfood` runs `cargo test --lib -p julie ensure_julie_fixture -- --ignored --nocapture` before its nextest call. No other ignored test is run by a tier.
- Dogfood thread cap: `.config/nextest.toml` test group `dogfood` with `max-threads = 6`, override filter equal to the dogfood filterset. No `NEXTEST_TEST_THREADS` in docs.
- No `#[ignore]`, no deleted assertion, no widened timeout to make a run green. A test that is wrong is fixed or deleted with the reason in the finding.
- Every performance change records a before and an after number from the same command on this machine (24 cores, main `ab6b8077` baseline in the design).
- CLAUDE.md and AGENTS.md stay byte-identical (`hooks/pre-commit` enforces it).
- Simplified Technical English in every doc and comment change. No narration comments.
- Conventional commits: `perf(...)`, `fix(...)`, `refactor(xtask)`, `docs(...)`.

---

## Verification Strategy

**Project source of truth:** `CLAUDE.md` (rewritten by Task 6), `docs/TESTING_GUIDE.md`.

**Worker red/green scope:** `cargo nextest run --lib -p <crate> <exact_test_name>`; for xtask, `cargo nextest run -p xtask <exact_test_name>`.

**Worker ceiling:** exact test names only, at most two runs per change. Workers never run `cargo xtask test …` or unfiltered nextest.

**Worker gate invariant:** Task 1: the two client tests finish under 1 s and the race test finishes under 6 s. Task 2: the single dogfood test `test_stemming_estimation_finds_estimator` wall time before and after. Task 3: `test_scenario_2_refactoring_pipeline` passes. Task 4: `cargo nextest run -p xtask` passes and `cargo xtask test dev` runs one nextest invocation. Task 5: `cargo nextest run -p xtask docs_contract` passes.

**Lead affected-change scope:** after batch A, `cargo nextest run --workspace -E 'not (test(/search_quality/) | test(/fixtures::julie_db/) | test(/dogfood/))'` (this is what Task 4 turns into `cargo xtask test dev`). After Task 4, `cargo xtask test dev`.

**Branch gate:** `cargo fmt --check`, `cargo clippy --workspace --all-targets`, `cargo xtask test full` (new definition), three timed runs each of `dev` and `dogfood` recorded in the finding.

**Security scope:** none declared.

**Replay/metric evidence:** Hard gates: `dev` under 25 s warm, `dogfood` under 40 s warm with the fixture present, zero uncovered tests (`cargo nextest list --workspace` count equals dev count plus dogfood count plus ignored count). Report-only: release `graph_load_millis` from `julie-server service status` after restart.

**Escalation triggers:** any new failure among the previously unrun tests; any flake in `tests::service::process` under 24 threads (then add a `max-threads = 1` test group and record it).

**Assigned verification failure:** Workers stop and report when assigned verification fails, unless this plan explicitly says to update that gate.

**Verification ledger:** `docs/plans/2026-09-11-test-speed-ledger.md`, template `docs/plans/verification-ledger-template.md`. Record invariant, command, scope label, commit SHA, result, and timestamp. Reuse a passing entry for the same HEAD and scope instead of rerunning.

## Parallel Execution Contract

| Task | Parallel batch | File ownership | Serialization required | Dependency reason |
|---|---|---|---|---|
| Task 1: Deadlines instead of 10 s waits | Batch A | Modify `src/service/client.rs`, `src/tests/service/client.rs`, `src/tests/tools/search/race_condition.rs` | No | None - safe parallel batch. |
| Task 2: Re-export index and index crate opt-level | Batch A | Modify `crates/julie-index/src/graph/reexports.rs`, `crates/julie-index/src/graph/resolve.rs`, `Cargo.toml`; create tests under `crates/julie-index/src/tests/graph/` | No | None - safe parallel batch. |
| Task 3: Fix the failing uncovered scenario test | Batch A | Modify `src/tests/request_scenarios.rs`; if the cause is in the CLI, `src/cli_tools/**` | No | None - safe parallel batch. |
| Task 4: Replace the bucket runner | None - serial | Delete `xtask/test_tiers.toml`, `xtask/src/manifest.rs`, `xtask/src/inventory.rs`, `xtask/src/changed.rs`, `xtask/src/changed/`, `xtask/src/runner/prebuild.rs`, `xtask/src/runner/tests.rs`, `xtask/tests/changed_tests.rs`, `xtask/tests/changed_boundary_tests.rs`, `xtask/tests/inventory_tests.rs`, `xtask/tests/manifest_tests.rs`, `xtask/tests/manifest_contract_tests.rs`, `xtask/tests/runner_tests.rs`, `xtask/tests/runner_boundary_tests.rs`, `xtask/tests/runner_coverage_tests.rs`; modify `xtask/src/main.rs`, `xtask/src/cli.rs`, `xtask/src/lib.rs`, `xtask/src/runner.rs`, `xtask/src/process.rs`, `xtask/Cargo.toml`, `xtask/tests/docs_contract_tests.rs`, `xtask/tests/support/**`; create `.config/nextest.toml` | Yes | Its acceptance criterion `cargo xtask test dev` passes needs Tasks 1 to 3 landed. |
| Task 5: Instructions and docs name only the new commands | None - serial | Modify `CLAUDE.md`, `AGENTS.md`, `README.md`, `docs/TESTING_GUIDE.md`, `docs/DEVELOPMENT.md`, `docs/DEPENDENCIES.md`, `docs/ADDING_NEW_LANGUAGES.md`, `docs/TREE_SITTER_QUALITY_BAR.md`, `docs/TREE_SITTER_UPGRADES.md`, `docs/plans/verification-ledger-template.md`, `.claude/hooks/session-start-tests.cjs`, `.claude/hooks/julie-routing-block.md`, `.claude/skills/dead-code-audit/SKILL.md`, `.agents/skills/dead-code-audit/SKILL.md`, `src/handler/tools/mod.rs` | Yes | The docs contract test rewritten in Task 4 defines the strings these files must carry. |
| Task 6: Gate, finding, ledger | None - serial | Create `docs/findings/2026-09-11-test-speed.md`, `docs/plans/2026-09-11-test-speed-ledger.md` | Yes | Needs every other task landed. Lead task. |

Commit mode: Batch A uses `parallel-lead-commit`. Tasks 4 to 6 use `serial-worker-commit` (Task 6 is the lead).

---

### Task 1: Deadlines instead of 10 s waits

**Files:**
- Modify: `src/service/client.rs:114-132` (`connect_or_start`)
- Modify: `src/tests/service/client.rs:24-52` and `:120-153` (`stale_record_is_removed_and_the_spawn_hook_runs_once`, `try_connect_removes_the_record_when_its_pid_is_dead`)
- Modify: `src/tests/tools/search/race_condition.rs:182-183` (`test_search_after_indexing_complete`)

**Interfaces:**
- Consumes: `connect_or_start(paths, spawn) -> Result<ServiceClient, ConnectError>` with its six callers in `src/main.rs`, `src/dashboard/standalone.rs`, `src/service/shim.rs` (unchanged).
- Produces: `pub async fn connect_or_start_within(paths: &RegistryPaths, spawn: impl Fn() -> std::io::Result<()>, deadline: Duration) -> Result<ServiceClient, ConnectError>`. `connect_or_start` becomes a one-line wrapper passing `Duration::from_secs(10)`. The error text keeps the form `service did not start within N s` with the real deadline.

**Contract inputs:** `handler.indexing_status.search_ready: AtomicBool` (pattern in `src/tests/integration/target_workspace.rs:30-40`).

**File ownership:** Modify `src/service/client.rs`, `src/tests/service/client.rs`, `src/tests/tools/search/race_condition.rs`

**Serialization required:** No

**Dependency reason:** None - safe parallel batch.

**What to build:** The two client tests spawn a hook that never starts a service and then wait out the full 10 s production deadline. Give the wait an explicit deadline parameter and let those tests pass 200 ms. The race-condition test sleeps a flat 10 s after `initialize_workspace_with_force`; replace the sleep with a poll of `search_ready` every 50 ms with a 10 s ceiling.

**Approach:** Add `connect_or_start_within`; keep `connect_or_start` for production callers. In the tests call `connect_or_start_within(&paths, hook, Duration::from_millis(200))`. In the race test, loop `while !handler.indexing_status.search_ready.load(Ordering::Acquire)` with `tokio::time::sleep(50ms)` and a `tokio::time::timeout(10 s)` around the loop so a real regression still fails. TDD: first change the tests, see them still take 10 s (RED on the wall-time expectation you assert with `Instant`), then implement.

**Acceptance criteria:**
- [ ] `cargo nextest run --lib -p julie stale_record_is_removed_and_the_spawn_hook_runs_once` and `try_connect_removes_the_record_when_its_pid_is_dead` each report under 1 s.
- [ ] `cargo nextest run --lib -p julie test_search_after_indexing_complete` reports under 6 s and still asserts the search result.
- [ ] `connect_or_start` callers unchanged; `cargo check` clean.
- [ ] Worker-scope verification passes and the change is handed to the lead per commit mode.

### Task 2: Re-export index and index crate opt-level

**Files:**
- Modify: `crates/julie-index/src/graph/reexports.rs:11-60` (`resolve_reexport`, `import_ids`, `module_path`) and `:193-240` (`in_reexport_module`, `direct_targets`, `glob_targets`)
- Modify: `crates/julie-index/src/graph/resolve.rs:14-40` (`resolve`) and `:164-196` (`resolve_target`)
- Modify: `Cargo.toml` (add `[profile.dev.package.julie-index] opt-level = 1` after `[profile.dev.package."*"]`)
- Test: a new file under `crates/julie-index/src/tests/graph/` (follow the existing module layout there)

**Interfaces:**
- Consumes: `SymbolTable` (`symbols.files()`, `symbols_in_file`, `symbol(id).kind/path/signature`), `Graph::load(reader, paths)` and `Graph.load_millis` in `crates/julie-index/src/graph/load.rs:77-130`.
- Produces: `pub(super) struct ReexportIndex { imports: Vec<(SymbolId, Vec<String>)> }` built once by `ReexportIndex::build(&SymbolTable)` in `resolve` and passed by reference to `resolve_target` and `resolve_reexport`. Public signatures of `Graph::load` and `resolve` do not change.

**Contract inputs:** Baseline: `cargo nextest run --lib -p julie test_stemming_estimation_finds_estimator` wall 8.4 s, of which 8.3 s is `Graph::build` on the fixture (76,541 symbols, 4,201 imports). Fixture must exist: `cargo test --lib -p julie ensure_julie_fixture -- --ignored --nocapture` (42 s once).

**File ownership:** Modify `crates/julie-index/src/graph/reexports.rs`, `crates/julie-index/src/graph/resolve.rs`, `Cargo.toml`; create tests under `crates/julie-index/src/tests/graph/`

**Serialization required:** No

**Dependency reason:** None - safe parallel batch.

**What to build:** Today every unresolved `crate::…` reference rebuilds the import list by walking all symbols and recomputes an allocating `module_path` for each import. Build the list of imports with their module paths once per `resolve` call and reuse it. Then add the profile override so the index crate compiles with `opt-level = 1` in dev and test builds.

**Approach:** Measure first: run the baseline command three times and record the walls. Write a unit test in the index crate that builds a small `SymbolTable` with a `crate::a::B` re-export and asserts `resolve_reexport` still returns the same target through the index (RED: the index type does not exist). Implement `ReexportIndex`, thread it through `resolve` -> `resolve_target` -> `resolve_reexport`, and make `direct_targets` and `glob_targets` read precomputed paths. Re-measure at `opt-level = 0`, record. Then add the `Cargo.toml` override and measure a third time. If the algorithm change alone gives under 2x on the fixture, revert it and keep only the profile override, and say so in the report. Keep `module_path` behavior identical (`src`/`Sources`, `mod.rs`, `lib.rs`, `main.rs` handling).

**Acceptance criteria:**
- [ ] New index-crate test passes: `cargo nextest run -p julie-index <test_name>`.
- [ ] Existing graph tests pass: `cargo nextest run -p julie-index tests::graph`.
- [ ] Three walls recorded for `test_stemming_estimation_finds_estimator` at each of: baseline, index only, index plus profile. Reported in the worker summary.
- [ ] `Cargo.toml` carries `[profile.dev.package.julie-index] opt-level = 1`.
- [ ] Worker-scope verification passes and the change is handed to the lead per commit mode.

### Task 3: Fix the failing uncovered scenario test

**Files:**
- Modify: `src/tests/request_scenarios.rs:225-260` (`test_scenario_2_refactoring_pipeline`)
- Modify if the cause is a product defect: `src/cli_tools/**` (edit subcommand argument handling)

**Interfaces:**
- Consumes: `ProcessFixture::cli_json` in `src/tests/request_process_helpers.rs:518`, `target/debug/julie-server edit --help` (`--dry-run <DRY_RUN>`, `--occurrence <OCCURRENCE>`).
- Produces: nothing new.

**Contract inputs:** The test fails on main with `assert_eq!(apply_exit, 0)` seeing exit 1 at line 253. The apply call passes `--dry-run=false` and `--workspace <root>` without `--standalone`, unlike `cli_subcommand`, which adds `--standalone --json`. Build the binary first: `cargo build -p julie --bin julie-server`.

**File ownership:** Modify `src/tests/request_scenarios.rs`; if the cause is in the CLI, `src/cli_tools/**`

**Serialization required:** No

**Dependency reason:** None - safe parallel batch.

**What to build:** Find why the apply step exits 1 (capture `out.stderr` from `self.cli(args)` in a temporary print, or run the command by hand against a temp workspace). If the test drifted from the CLI contract, fix the test. If the CLI misbehaves for a documented flag, fix the CLI with a test that names the defect. If the scenario duplicates `tests::edit_recovery_contract` or `tests::cli_execution_tests` coverage and cannot be made meaningful, delete it and write the reason in the worker report for the finding.

**Approach:** Diagnose before editing. Do not add `--standalone` blindly; the scenario's step 4 says "apply edit in a fresh workspace" so the intended path matters. Keep the dry-run assertions at lines 226-228.

**Acceptance criteria:**
- [ ] `cargo nextest run --lib -p julie test_scenario_2_refactoring_pipeline` passes, or the test is deleted and the report states the reason.
- [ ] `cargo nextest run --lib -p julie tests::request_scenarios` passes.
- [ ] Worker-scope verification passes and the change is handed to the lead per commit mode.

### Task 4: Replace the bucket runner

**Files:**
- Delete: `xtask/test_tiers.toml`, `xtask/src/manifest.rs`, `xtask/src/inventory.rs`, `xtask/src/changed.rs`, `xtask/src/changed/` (all files), `xtask/src/runner/prebuild.rs`, `xtask/src/runner/tests.rs`, `xtask/tests/changed_tests.rs`, `xtask/tests/changed_boundary_tests.rs`, `xtask/tests/inventory_tests.rs`, `xtask/tests/manifest_tests.rs`, `xtask/tests/manifest_contract_tests.rs`, `xtask/tests/runner_tests.rs`, `xtask/tests/runner_boundary_tests.rs`, `xtask/tests/runner_coverage_tests.rs`
- Modify: `xtask/src/main.rs`, `xtask/src/cli.rs:11-60` (`TestCommand`, `CliCommand`), `xtask/src/lib.rs`, `xtask/src/runner.rs`, `xtask/src/process.rs`, `xtask/Cargo.toml` (drop `serde` and `toml` from `[dependencies]` if nothing else uses them; `xtask/tests/lean_deps_allowlist_tests.rs` asserts the exact dependency sets and uses `toml` as a dev-dependency, so update its allowlist in the same change), `xtask/tests/docs_contract_tests.rs`, `xtask/tests/support/**` (remove `manifest_contract_expected.rs`)
- Create: `.config/nextest.toml`
- Keep unchanged: `xtask/src/dev_workflow.rs`, `xtask/src/sync_plugin.rs`, `xtask/tests/dispatch_tests.rs`, `xtask/tests/extractor_dependency_contract_tests.rs`, `xtask/tests/lean_deps_allowlist_tests.rs`, `xtask/tests/toolchain_contract_tests.rs` (edit these only where they reference deleted symbols)

**Interfaces:**
- Consumes: the Global Constraints command list.
- Produces: `cargo xtask test dev|dogfood|full` only. `cargo xtask test` with no tier, or any other word, prints the three tiers and exits 2. Output per command: `RUN <command>` before, `PASS <command> (<seconds>s)` or `FAIL <command> (<seconds>s)` after, then `SUMMARY: <tier> <passed>/<total> commands in <seconds>s`. Non-zero exit on the first failing command. `--timeout-multiplier` and `--coverage` are removed (coverage is `cargo tarpaulin`, see TESTING_GUIDE).

**Contract inputs:**
- `dev` commands, in order:
  1. `cargo build -p julie --bin julie-server`
  2. `cargo nextest run --workspace -E 'not (test(/search_quality/) | test(/fixtures::julie_db/) | test(/dogfood/))'`
  3. `cargo nextest run --lib -p julie --run-ignored only -E 'test(/tests::cli::/)'`
- `dogfood` commands, in order:
  1. `cargo test --lib -p julie ensure_julie_fixture -- --ignored --nocapture`
  2. `cargo nextest run --workspace -E 'test(/search_quality/) | test(/fixtures::julie_db/) | test(/dogfood/)'`
- `full` = `dev` commands then `dogfood` commands.
- `.config/nextest.toml`:
  ```toml
  [test-groups.dogfood]
  max-threads = 6

  [[profile.default.overrides]]
  filter = 'test(/search_quality/) | test(/fixtures::julie_db/) | test(/dogfood/)'
  test-group = 'dogfood'
  ```
- The rewritten `docs_contract_tests.rs` asserts: `CLAUDE.md` equals `AGENTS.md`; both and `README.md` contain `cargo xtask test dev`, `cargo xtask test dogfood`, `cargo xtask test full`, `cargo nextest run --lib <name>`; none of `CLAUDE.md`, `AGENTS.md`, `README.md`, `docs/TESTING_GUIDE.md`, `docs/plans/verification-ledger-template.md` contains `xtask test changed`, `xtask test nano`, `xtask test fast`, `xtask test smoke`, `xtask test system`, `xtask test bucket`, `xtask test inventory`, `xtask test list`, `OverBudget`, `PREBUILD`, `COLD WALL`, or `NEXTEST_TEST_THREADS`; `.cargo/config.toml` keeps the two aliases. This test is RED until Task 5 lands; that is expected and the worker reports it as the handoff to Task 5, not as a failure.

**File ownership:** as listed in the Parallel Execution Contract row for Task 4.

**Serialization required:** Yes

**Dependency reason:** Its acceptance criterion `cargo xtask test dev` passes needs Tasks 1 to 3 landed.

**What to build:** Delete the manifest-driven runner and its policy, budget, prebuild, and inventory machinery. Replace with three fixed command lists and a runner that executes them in order with wall time per command. Add the nextest test group that caps dogfood parallelism.

**Approach:** Start from the tests: rewrite `docs_contract_tests.rs` to the new assertions and write a small runner test in `xtask/tests/` that feeds a fake executor (the `CommandExecutor` trait in `runner.rs` already exists; keep it) and asserts the `dev` command list and the summary format. Then delete files and fix compile errors until `cargo nextest run -p xtask` passes except the docs contract test. Keep `process.rs` command splitting (quoted filtersets must survive; there is an existing test for that at `process.rs:146`). Do not keep dead flags "for later". `git rm` the deleted files.

**Acceptance criteria:**
- [ ] `cargo nextest run -p xtask` passes except `docs_contract_tests` (documented handoff to Task 5).
- [ ] `cargo xtask test dev` runs exactly the three `dev` commands, passes, and prints one `SUMMARY` line.
- [ ] `cargo xtask test dogfood` runs exactly the two `dogfood` commands and passes with the fixture present.
- [ ] `cargo xtask test nano` exits 2 with the tier list.
- [ ] `wc -l` over `xtask/` is lower than on `main` (7,201 lines in the runner, manifest, changed, inventory modules and tests today); report both numbers.
- [ ] Worker-scope verification passes and the change is committed by the worker (`refactor(xtask): one nextest run per tier`) with the SHA reported.

### Task 5: Instructions and docs name only the new commands

**Files:**
- Modify: `CLAUDE.md` (sections "Quick Reference", "RUNNING TESTS", "Subagent & Worker Agent Test Rules", "Verification Ledger Contract", "Narrowing Failures", "Known Pre-Existing Failures", "Why Dogfood Is Slow", "Rebuilding Fixture Database", and the "TDD" step 6), `AGENTS.md` (copy of `CLAUDE.md`)
- Modify: `README.md:650-685`, `docs/TESTING_GUIDE.md:40-115` and `:142-156`, `docs/DEVELOPMENT.md:25`, `docs/DEPENDENCIES.md:23`, `docs/ADDING_NEW_LANGUAGES.md:14`, `docs/TREE_SITTER_QUALITY_BAR.md:158-166` (the commands table only; the ledger rows below are history), `docs/TREE_SITTER_UPGRADES.md:17,105-116`, `docs/plans/verification-ledger-template.md:20-21`, `.claude/hooks/session-start-tests.cjs:4`, `.claude/hooks/julie-routing-block.md:25`, `.claude/skills/dead-code-audit/SKILL.md:132`, `.agents/skills/dead-code-audit/SKILL.md` (same line), `src/handler/tools/mod.rs:6` (doc comment)

**Interfaces:**
- Consumes: the command list and the `docs_contract_tests.rs` assertions from Task 4.
- Produces: the new testing contract text agents read.

**Contract inputs:** New rules to state, in Simplified Technical English:
- Edit loop: `cargo check`, then `cargo nextest run --lib <exact_test_name>` (3.5 s incremental rebuild).
- After a batch, or before handoff: `cargo xtask test dev` (whole workspace, one nextest run, about 20 s warm). After search, scoring, ranking, or graph changes: `cargo xtask test dogfood` (about 30 s plus 42 s when the fixture rebuilds). Before merge: `cargo xtask test full`.
- Subagents run exact tests only, at most two runs per change; the lead runs `dev` once per batch. Never run more than one test command at once.
- Extractor dependency re-pin gate is `cargo xtask test dev` (its three contract tests are in it).
- Verification ledger scope labels: `worker-red-green`, `dev`, `dogfood`, `full`, `live`.
- Known failures section: "All tiers are green. A failure is a regression." Keep the `$TMPDIR` marker warning.

**File ownership:** as listed in the Parallel Execution Contract row for Task 5.

**Serialization required:** Yes

**Dependency reason:** The docs contract test rewritten in Task 4 defines the strings these files must carry.

**What to build:** Every live instruction file names only `dev`, `dogfood`, `full`, and the exact-test edit loop. Remove tiers, buckets, budgets, warm/cold accounting, `changed`, `--scale`, inventory, `NEXTEST_TEST_THREADS`, and the bucket-name examples. Historical files under `docs/plans/`, `docs/findings/`, `docs/release-notes/`, `docs/releases/`, and `.agents/*/handoff.md` are not touched.

**Approach:** Edit `CLAUDE.md`, then `cp CLAUDE.md AGENTS.md`. Shorten rather than rewrite: the "RUNNING TESTS" section becomes one table with three tiers and the edit-loop rule. Keep the subagent rules block but drop the bucket sentences. Check with `/usr/bin/grep -rnE 'xtask test (changed|nano|fast|smoke|system|bucket|inventory|list|benchmark|reliability)|OverBudget|PREBUILD|COLD WALL|NEXTEST_TEST_THREADS' --exclude-dir=target --exclude-dir=.git --exclude-dir=.agents --exclude-dir=.memories --exclude-dir=.worktrees . | grep -vE '^\./docs/(plans|findings|release-notes|releases)/'` returning only the two dead-code-audit skill files after they are edited, then zero.

**Acceptance criteria:**
- [ ] `cargo nextest run -p xtask docs_contract` passes.
- [ ] `diff CLAUDE.md AGENTS.md` is empty.
- [ ] The grep in Approach returns nothing.
- [ ] Worker-scope verification passes and the change is committed by the worker (`docs(testing): three tiers, one nextest run each`) with the SHA reported.

### Task 6: Gate, finding, ledger

**Files:**
- Create: `docs/findings/2026-09-11-test-speed.md`, `docs/plans/2026-09-11-test-speed-ledger.md`

**Interfaces:**
- Consumes: everything above.
- Produces: the finding with the before and after table.

**Contract inputs:** Baseline table in `docs/plans/2026-09-11-test-speed-design.md`. Same machine, same commands, warm, three runs, report each run and the median.

**File ownership:** Create `docs/findings/2026-09-11-test-speed.md`, `docs/plans/2026-09-11-test-speed-ledger.md`

**Serialization required:** Yes

**Dependency reason:** Needs every other task landed. Lead task.

**What to build:** Run the branch gate. Record three timed runs of `cargo xtask test dev` and `cargo xtask test dogfood`, one `cargo xtask test full`, `cargo nextest list --workspace` counts against the two filters plus ignored, clippy and fmt. Rebuild main's release binary from the branch, copy the sidecar, restart the service, and read `graph_load_millis` for the julie workspace from `julie-server service status` (report-only). Write the finding: what changed, before and after table, the tests that surfaced among the 319 and what happened to each, deferred items, and the side findings from the design (inventory breakage, `-j 1`, shared-target `CARGO_MANIFEST_DIR` trap).

**Approach:** Lead runs it in the worktree. Checkpoint before the commit.

**Acceptance criteria:**
- [ ] `dev` median under 25 s warm; `dogfood` median under 40 s warm; `full` passes.
- [ ] Zero uncovered tests by count.
- [ ] `cargo fmt --check` and `cargo clippy --workspace --all-targets` clean of errors.
- [ ] Finding and ledger committed (`docs(testing): test speed gate finding and ledger`).
