# Test Runtime Gate Reliability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Cut wasted test wall-clock time by making `xtask` buckets smaller, observable, directly runnable, and strict about duplicate or hidden coverage.

**Architecture:** Keep `cargo xtask test` as the only canonical test entry point, but make the runner enforce the testing contract instead of relying on prose. The runner should expose per-command timing, single-bucket execution, duplicate command protection, and bucket splits that map to real ownership boundaries. Verification policy stays in `RAZORBACK.md`, `AGENTS.md`, and `docs/TESTING_GUIDE.md`; this plan makes that policy harder to accidentally violate.

**Tech Stack:** Rust, cargo nextest, `xtask`, TOML test-tier manifest, Julie test hierarchy, Razorback subagent workflow.

---

## Problem Statement

The projection-invariants merge was a useful but painful diagnostic. The production fix was small; most of the wall-clock went into repeated broad gates and a `tools-search` bucket that timed out because it was too broad and too tightly calibrated. We also discovered that `cargo xtask test bucket <name>` already exists, but `cargo xtask test tools-search` fails because bare names are treated as tiers. That is acceptable only if the explicit bucket path is documented, validated early, and obvious in runner output.

The immediate failure is not "tests are bad." The failure is that buckets are too coarse, some filters overlap, elapsed time is visible only after the wait is over, and agents can still choose wasteful commands despite the docs saying not to.

## File Structure

**Modify**
- `xtask/test_tiers.toml`
  - Split slow tool buckets into smaller buckets with honest `expected_seconds` and `timeout_seconds`.
  - Add a dedicated `xtask-runner` bucket so xtask-only changes do not drag through the whole CLI bucket.
- `xtask/src/cli.rs`
  - Validate `TestCommand::Bucket` names before prebuild or execution.
  - Keep explicit bucket syntax as `cargo xtask test bucket <name>`.
- `xtask/src/main.rs`
  - Route any new `TestCommand` variants if the implementation adds inventory or audit output.
- `xtask/src/runner.rs`
  - Emit command-level timing and status inside each bucket.
  - Preserve captured output suppression for passing commands.
- `xtask/src/manifest.rs`
  - Reject exact duplicate command strings across checked-in buckets.
  - Keep metadata parsing backward-compatible.
- `xtask/src/changed.rs`
  - Route changed paths to the split buckets.
  - Stop routing `xtask/` changes through the broad CLI bucket when `xtask-runner` is enough.
- `xtask/tests/runner_tests.rs`
- `xtask/tests/manifest_tests.rs`
- `xtask/tests/manifest_contract_tests.rs`
- `xtask/tests/changed_tests.rs`
- `docs/TESTING_GUIDE.md`
- `AGENTS.md`
- `CLAUDE.md`

**Create**
- `xtask/src/inventory.rs`
  - Implement the `cargo xtask test inventory` command. Keep it read-only and report-only. It must list tests; it must not run tests.
- `xtask/tests/inventory_tests.rs`
  - Test inventory parsing, duplicate selected-test reporting, and non-inventoryable command reporting.

## Implementation Tasks

### Task 1: Make Single-Bucket Execution Safe And Obvious

**Files:**
- Modify: `xtask/src/cli.rs`
- Modify: `xtask/src/runner.rs`
- Modify: `xtask/tests/runner_tests.rs`

**What to build:** Harden the existing explicit bucket path, `cargo xtask test bucket <name>`. Unknown bucket names should fail during command validation before prebuild. The user-facing error should name the unknown bucket and suggest `cargo xtask test list`.

**Approach:** Do not add `cargo xtask test <bucket>` as a bare alias in this task. Bare names already mean tiers, and overloading them would make tier and bucket errors muddy. Update `validate_test_command` so `TestCommand::Bucket` checks `manifest.buckets.contains_key(&name)` and rejects unknown names. Keep `run_bucket` as the execution path; refactor it to delegate to `run_named_buckets` only if the behavior stays byte-for-byte compatible in tests.

**Acceptance criteria:**
- `cargo xtask test bucket tools-search-tantivy` or another real split bucket runs one bucket.
- `cargo xtask test bucket not-a-bucket` fails before prebuild and says the bucket is unknown.
- `cargo xtask test tools-search-tantivy` continues to be rejected unless a tier with that name exists.
- The command remains compatible with `--coverage` and `--timeout-multiplier`.
- Passing bucket commands still suppress captured stdout.

**Worker verification ceiling:**
- `cargo nextest run -p xtask runner_tests_cli_rejects_unknown_bucket_name 2>&1 | tail -10`
- `cargo nextest run -p xtask runner_tests_cli_contract_supports_tiers_list_and_bucket 2>&1 | tail -10`
- `cargo nextest run -p xtask runner_tests_run_bucket_returns_structured_result 2>&1 | tail -10`

### Task 2: Add Command-Level Timing And Bucket Diagnostics

**Files:**
- Modify: `xtask/src/runner.rs`
- Modify: `xtask/tests/runner_tests.rs`

**What to build:** Print one concise line per command inside each bucket with command index, status, elapsed time, and the command string. Keep final bucket summaries, but stop making the lead wait until timeout or bucket end to see which command is eating the clock.

**Approach:** Use the existing elapsed data in `CommandOutcome::{Passed, Failed, TimedOut}`. Add output from `execute_bucket` after each command finishes:

```text
COMMAND tools-search-tantivy 1/3 PASS (12.4s) cargo nextest run --lib ...
```

For failed or timed-out commands, keep the existing captured output markers. For passing commands, emit only the one timing line. Avoid verbose per-test output.

**Acceptance criteria:**
- Passing commands print `COMMAND <bucket> <index>/<total> PASS`.
- Failing commands print the command timing line before captured output markers.
- Timed-out commands print `TIMEOUT` and keep the existing timeout error details.
- The final summary still includes bucket-level expected versus actual timing.
- Existing failure-output tests keep proving that captured output is shown only on failure and timeout.

**Worker verification ceiling:**
- `cargo nextest run -p xtask runner_tests_bucket_output_includes_command_timings 2>&1 | tail -10`
- `cargo nextest run -p xtask runner_tests_failed_bucket_writes_captured_output_between_markers 2>&1 | tail -10`
- `cargo nextest run -p xtask runner_tests_timed_out_bucket_writes_captured_output_between_markers 2>&1 | tail -10`

### Task 3: Reject Exact Duplicate Manifest Commands

**Files:**
- Modify: `xtask/src/manifest.rs`
- Modify: `xtask/tests/manifest_tests.rs`

**What to build:** Manifest validation should reject exact duplicate command strings across all checked-in bucket definitions. This will not catch all overlapping nextest filters, but it kills the cheap class of accidental duplicate work and gives future overlap detection a clear home.

**Approach:** Extend `TestManifest::validate` with a `BTreeMap<String, String>` from command string to first bucket name. When another bucket uses the same command string, return an error that names both buckets and the command. Do not reject commands duplicated only in test fixtures unless the fixture is intentionally asserting the error.

**Acceptance criteria:**
- A manifest with the same command in two buckets fails validation.
- The error names the first bucket, second bucket, and duplicate command.
- The checked-in `xtask/test_tiers.toml` passes validation.
- Existing manifest default metadata tests still pass.

**Worker verification ceiling:**
- `cargo nextest run -p xtask manifest_tests_reject_duplicate_bucket_commands 2>&1 | tail -10`
- `cargo nextest run -p xtask manifest_contract_tests_checked_in_manifest_uses_exact_bucket_specs 2>&1 | tail -10`

### Task 4: Split The Slow Tool Buckets

**Files:**
- Modify: `xtask/test_tiers.toml`
- Modify: `xtask/tests/manifest_contract_tests.rs`
- Modify: `xtask/src/changed.rs`
- Modify: `xtask/tests/changed_tests.rs`

**What to build:** Replace the coarse `tools-search` and `tools-misc` dev buckets with smaller buckets that reveal where time is going and avoid timeout cliffs.

**Approach:** Use the existing test module boundaries. Do not invent new Rust test modules in this task unless a filter cannot target the intended group.

Split search coverage into buckets similar to:
- `tools-search-tantivy`: Tantivy tokenizer, stemming, scoring, variants, and Tantivy integration tests.
- `tools-search-line-file`: line mode, file mode, file pattern, zero-hit live line/file behavior.
- `tools-search-ranking-format`: promotion, definition search, content scoring, lean formatting, quality, and ranking behavior.
- `tools-search-context`: `tests::tools::search_context_lines`.
- `tools-search-text`: `tests::tools::text_search_tantivy`.
- `tools-search-hybrid`: `tests::tools::hybrid_search_tests`.

Split misc tool coverage into buckets similar to:
- `tools-get-symbols`: get-symbols variants, with duplicate get-symbols filters removed.
- `tools-editing`: editing tests.
- `tools-navigation`: deep dive, fast refs, call path, target workspace navigation.
- `tools-refactoring`: refactoring tests.
- `tools-metrics`: metrics and spillover tests if they are currently uncovered.
- `tools-format-filter`: formatting, filtering, query classification, token-savings tests.

Move `tools::search::query_preprocessor::tests` out of misc and into a search bucket or its own small search-query bucket. Update `dev` and `full` tiers to list the split buckets instead of the old coarse buckets. Remove the old `tools-search` and `tools-misc` buckets unless the implementation makes them explicit aliases, and do not add aliases unless `xtask` supports alias buckets without re-running tests.

**Acceptance criteria:**
- `cargo xtask test list` shows split buckets with scope labels, expected times, command counts, and expensive markers.
- No command in the split buckets intentionally reruns tests already selected by another command in the same bucket.
- `dev` includes the split buckets and no longer includes coarse `tools-search` or `tools-misc`.
- `full` includes the same split buckets.
- Changed files under search, get-symbols, editing, navigation, metrics, formatting/filtering, and query-preprocessor paths select the smallest relevant bucket set.

**Worker verification ceiling:**
- `cargo nextest run -p xtask manifest_contract_tests_checked_in_manifest_uses_exact_bucket_specs 2>&1 | tail -10`
- `cargo nextest run -p xtask changed_tests_search_paths_select_split_search_buckets 2>&1 | tail -10`
- `cargo nextest run -p xtask changed_tests_xtask_paths_select_xtask_runner_bucket 2>&1 | tail -10`

### Task 5: Add A Report-Only Test Inventory Audit

**Files:**
- Create: `xtask/src/inventory.rs`
- Modify: `xtask/src/lib.rs`
- Modify: `xtask/src/cli.rs`
- Modify: `xtask/src/main.rs`
- Test: `xtask/tests/inventory_tests.rs` or `xtask/tests/runner_tests.rs`

**What to build:** Add a report-only inventory command that lists selected tests for a tier or bucket and identifies duplicate selected tests. It should use `cargo nextest list`, not `cargo nextest run`, and it should be explicit that this is an audit command, not a verification gate.

**Approach:** Prefer a command shaped like:

```bash
cargo xtask test inventory --tier dev
cargo xtask test inventory --bucket tools-search-tantivy
```

The command should transform bucket commands from `cargo nextest run ...` to `cargo nextest list ...` when possible. If a command cannot be transformed safely, report it as "not inventoryable" instead of guessing. Parse nextest list output into test names, then print:
- total selected tests
- duplicate selected tests across commands
- commands that could not be inventoried
- optional missing-test comparison for `tests::tools` if `--scope tests::tools` is passed

Keep this command out of `dev` and out of worker gates unless a worker is explicitly assigned inventory behavior. It is a diagnostic tool to stop us from rediscovering overlap manually.

**Acceptance criteria:**
- Inventory can run against a fake executor in tests without invoking real cargo.
- Exact duplicate selected test names are reported with both commands that selected them.
- Non-inventoryable commands are reported, not ignored.
- The command output is stable enough to paste into a plan ledger.
- The implementation does not run tests.

**Worker verification ceiling:**
- `cargo nextest run -p xtask inventory_tests_reports_duplicate_selected_tests 2>&1 | tail -10`
- `cargo nextest run -p xtask inventory_tests_reports_non_inventoryable_commands 2>&1 | tail -10`
- `cargo nextest run -p xtask runner_tests_cli_parses_inventory_command 2>&1 | tail -10`

### Task 6: Update The Testing Contract Docs

**Files:**
- Modify: `docs/TESTING_GUIDE.md`
- Modify: `AGENTS.md`
- Modify: `CLAUDE.md`

**What to build:** Document the actual post-plan workflow so agents stop reaching for broad suites. The docs should say how to run one bucket, when workers may run exact tests, how the lead uses split buckets, and when inventory is report-only.

**Approach:** Keep `RAZORBACK.md` as the source of truth for model routing and gate ownership. Update harness-visible docs only where agents need command syntax during a run. Do not duplicate the full Razorback routing table.

Required doc points:
- Workers run exact tests only and never run xtask tiers.
- Leads may run `cargo xtask test bucket <name>` for a focused bucket.
- `cargo xtask test changed` is lead-owned and used after a coherent batch.
- `cargo xtask test dev` is branch-gate evidence, run once.
- `dogfood`, `system`, `reliability`, and `full` require explicit plan triggers.
- Inventory output is diagnostic evidence, not a passing test gate.
- Evidence reuse requires matching commit SHA and scope label.

**Acceptance criteria:**
- `AGENTS.md` and `CLAUDE.md` remain synchronized where they duplicate harness-visible test rules.
- `docs/TESTING_GUIDE.md` names `cargo xtask test bucket <name>` and the inventory command if implemented.
- The docs warn against rerunning branch gates after metadata-only commits when same-HEAD scoped evidence is reusable.
- No doc section tells workers to run broad xtask tiers.

**Worker verification ceiling:**
- `cargo nextest run -p xtask docs_contract_tests_agent_docs_stay_in_sync 2>&1 | tail -10`
- `cargo nextest run -p xtask docs_contract_tests_testing_guide_documents_bucket_command 2>&1 | tail -10`

## Verification Strategy

**Project source of truth:** `AGENTS.md`, `CLAUDE.md`, `RAZORBACK.md`, `docs/TESTING_GUIDE.md`, and `xtask/test_tiers.toml`.

**Worker red/green scope:** Workers write failing tests first and run only their assigned exact xtask test filter:
`cargo nextest run -p xtask <exact_test_name> 2>&1 | tail -10`

For docs-only contract tests, workers still use exact xtask test names. For any Julie lib test touched by bucket split validation, workers use:
`cargo nextest run --lib <exact_test_name> 2>&1 | tail -10`

**Worker ceiling:** Workers may run at most their exact assigned test twice per fix cycle: once RED and once GREEN. Workers must not run `cargo xtask test changed`, `cargo xtask test dev`, `cargo xtask test dogfood`, `cargo xtask test system`, `cargo xtask test full`, broad `cargo nextest run --lib`, sleeps, polls, or retries.

**Worker gate invariant:** Each worker report must state the invariant it proved, such as "unknown bucket names fail before prebuild" or "split search path selects only the search split buckets named by changed routing."

**Lead affected-change scope:** After each coherent batch, the lead runs:
`cargo xtask test changed`

If `changed` falls back to `dev`, the lead records the fallback reason and decides whether to fix routing before accepting the broad run. Do not silently accept fallback-to-dev for known test-harness paths.

**Branch gate:** Before handoff, run:
`cargo xtask test dev`

**Replay/metric evidence:** Bucket elapsed time and inventory counts are hard evidence for calibration but not pass/fail gates unless a contract test asserts output shape. The branch is acceptable only if split-bucket `dev` passes and the plan ledger records observed per-bucket timings.

**Expensive gates:** No `dogfood`, `system`, `reliability`, or `full` gate is required for this plan unless implementation touches daemon behavior, workspace initialization behavior, search quality fixtures, or production Julie tool behavior. This plan should stay inside xtask and docs.

**Escalation triggers:** Escalate to strategy or gate-review tier if:
- a worker proposes broad test execution,
- a split bucket drops known tests without documenting the exclusion,
- inventory reveals overlapping selected tests that the plan does not account for,
- changed-file routing maps a source path to no bucket,
- a timing change suggests the branch gate is still brittle.

**Assigned verification failure:** Workers stop and report when assigned verification fails. They do not update the gate or broaden scope unless the lead revises the plan.

**Verification ledger:** Record invariant, command, scope label, commit SHA, result, timestamp, and whether evidence was reused. Evidence reuse is allowed only when commit SHA and scope label match exactly. If evidence is reused, add a new ledger row with `Evidence Reused = yes` and point to the original command result.

## Model Routing

**Project source of truth:** `RAZORBACK.md`. Do not copy the global model table into this plan. If a local sentence conflicts with `RAZORBACK.md`, `RAZORBACK.md` wins.

**Plan-specific overrides:** Verification policy, xtask bucket routing, changed-file selection, inventory semantics, ledger semantics, specialist-gate ownership, and test timing policy are strategy or gate-review work by default.

**Codex routing:**
- Use `gpt-5.5 high` for lead planning, architecture decisions, and final integration review.
- Use `gpt-5.3-codex high` for gate-review workers and failed-worker diagnosis.
- Use `gpt-5.3-codex high` for implementation workers that edit runner, manifest, changed routing, or inventory code.
- Use `gpt-5.4-mini medium` only for mechanical docs edits that do not own failing tests, timing evidence, inventory interpretation, or acceptance gates.

**Worker eligibility:** Implementation-tier workers are eligible only when file ownership is narrow, the task owns exact xtask tests, and the task does not reinterpret project-wide verification policy.

**Mechanical exclusion:** Mechanical workers cannot own failing tests, timing gate interpretation, inventory output, changed-selection routing, or branch acceptance evidence.

**Unsupported harness behavior:** If the harness cannot choose models per agent, use `inherit`, note it in the worker report, and continue.

## Task Decomposition

- Worker A: Task 1 single-bucket validation and existing bucket command hardening. Owns `xtask/src/cli.rs` and related runner CLI tests.
- Worker B: Task 2 command-level timing output. Owns `xtask/src/runner.rs` timing output and runner tests.
- Worker C: Task 3 duplicate command validation. Owns `xtask/src/manifest.rs` and manifest tests.
- Worker D: Task 4 bucket split manifest and changed-routing updates. Owns `xtask/test_tiers.toml`, `xtask/tests/manifest_contract_tests.rs`, `xtask/src/changed.rs`, and changed tests. This worker should start after Tasks 1-3 clarify output contracts.
- Worker E: Task 5 inventory audit command. Owns inventory files and CLI routing. This task ships in the same plan so future bucket overlap can be measured instead of rediscovered manually.
- Worker F: Task 6 docs contract updates. Owns docs only and should run after Tasks 1-5 settle command syntax.
- Lead: Integrates the split buckets, inspects tests for meaningful assertions, runs `cargo xtask test changed`, runs `cargo xtask test dev`, records observed timing evidence, and blocks broad specialist gates unless the plan trigger is met.

## Verification Ledger

Use the format from `docs/plans/verification-ledger-template.md` during implementation. The first implementation batch should record the worker exact-test evidence before the lead runs affected-change verification. Do not copy example rows from the template into this plan.

## Risks

- Splitting buckets can reduce runtime while accidentally dropping tests. The manifest contract and inventory task exist to catch that.
- Exact duplicate command detection is useful but not sufficient. Overlapping nextest filters require inventory output or careful command selection.
- Smaller buckets can still produce broad `changed` runs if `changed.rs` routes shared paths to too many buckets. Changed-routing tests must assert exact bucket sets for common paths.
- Timing calibration can become policy theater if expected values are copied without observed evidence. The lead must record post-split `dev` timings before handoff.
- Adding too much inventory machinery could become another slow tool. Keep inventory report-only and out of normal gates.
- Docs can drift. `AGENTS.md` and `CLAUDE.md` must stay synchronized for harness-visible test rules, while `RAZORBACK.md` remains the source for model routing and gate ownership.
