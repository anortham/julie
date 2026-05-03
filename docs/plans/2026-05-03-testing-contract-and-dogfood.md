# Testing Contract And Dogfood Workflow Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Make Julie's test workflow enforceable, measurable, and cheap enough that agents stop turning narrow changes into multi-hour verification loops.

**Architecture:** Keep `cargo xtask test` as the canonical test runner, but move more of the test policy from prose into runner output, bucket metadata, and plan templates. Treat CLI standalone dogfood as a separate verification lane from MCP transport/session tests so tool behavior can be checked quickly without pretending it covers daemon protocol behavior.

**Tech Stack:** Rust, `cargo nextest`, `cargo xtask`, TOML test-tier manifest, Julie standalone CLI, existing MCP daemon tests.

---

## File Structure

**Modify**
- `xtask/test_tiers.toml`  
  Add owner/scope metadata for buckets, tighten expected runtimes, and add small specialist buckets where current buckets are too broad for staged architecture work.
- `xtask/src/manifest.rs`  
  Parse optional bucket metadata such as scope label, owner, expensive flag, and changed-file rationale.
- `xtask/src/changed.rs`  
  Improve changed-file selection reporting so fallback-to-dev explains the exact file or prefix that caused it.
- `xtask/src/runner.rs`  
  Render expected versus actual bucket time, command count, scope label, and slow-bucket warnings in summaries.
- `xtask/tests/manifest_tests.rs`
- `xtask/tests/manifest_contract_tests.rs`
- `xtask/tests/changed_tests.rs`
- `xtask/tests/runner_tests.rs`
- `docs/TESTING_GUIDE.md`
- `AGENTS.md`
- `JULIE_AGENT_INSTRUCTIONS.md`

**Create**
- `docs/plans/verification-ledger-template.md`  
  Reusable ledger format for command, scope label, invariant, commit SHA, result, timestamp, and expensive-gate reuse.

## Implementation Tasks

### Task 1: Bucket Metadata And Summary Output

**Files:**
- Modify: `xtask/test_tiers.toml`
- Modify: `xtask/src/manifest.rs`
- Modify: `xtask/src/runner.rs`
- Test: `xtask/tests/manifest_tests.rs`
- Test: `xtask/tests/manifest_contract_tests.rs`
- Test: `xtask/tests/runner_tests.rs`

**What to build:** Add optional metadata to each bucket: `scope_label`, `owner`, `expensive`, and `notes`. The runner summary should show each bucket's expected time, actual time, command count, scope label, and whether the bucket exceeded its expected runtime by more than 50 percent.

**Approach:** Keep existing manifest fields backward-compatible. Missing metadata should default to `scope_label = "bucket"`, `owner = "lead"`, and `expensive = false`. Do not change the command execution model in this task. Tests should parse both old-style and metadata-rich bucket definitions.

**Acceptance criteria:**
- [ ] Existing `xtask/test_tiers.toml` remains valid after metadata is added.
- [ ] `cargo xtask test list` shows scope labels and expensive bucket markers.
- [ ] `render_summary` includes expected versus actual elapsed time per bucket.
- [ ] A bucket that exceeds expected runtime by more than 50 percent is marked as slow in the summary.
- [ ] Worker-scope verification passes with exact xtask test names.

### Task 2: Changed-Selection Rationale

**Files:**
- Modify: `xtask/src/changed.rs`
- Modify: `xtask/src/main.rs`
- Test: `xtask/tests/changed_tests.rs`

**What to build:** Make `cargo xtask test changed` explain exactly why it selected each bucket or fell back to `dev`. The output should name matched paths, bucket names, and the fallback rule that fired.

**Approach:** Extend `ChangedSelection` with a small rationale list. For normal bucket mode, record `path -> bucket` decisions. For fallback mode, record whether the fallback was exact file, prefix, manifest-level, or unknown. Keep ignored-path reporting intact.

**Acceptance criteria:**
- [ ] A change under `src/adapter/` reports fallback to `dev` because `src/adapter/` is a fallback prefix.
- [ ] A change under `src/tools/search/` reports the selected search bucket rather than a vague bucket list.
- [ ] Ignored docs-only paths still report no test run when no source path changed.
- [ ] The command output is concise enough to paste into a verification ledger.
- [ ] Worker-scope verification passes with exact xtask test names.

### Task 3: Specialist Buckets For Architecture Work

**Files:**
- Modify: `xtask/test_tiers.toml`
- Modify: `xtask/src/changed.rs`
- Test: `xtask/tests/changed_tests.rs`
- Test: `xtask/tests/manifest_contract_tests.rs`

**What to build:** Add smaller buckets for staged architecture work so lead-owned gates can be precise without defaulting to `dev` or `full`.

**Approach:** Add buckets with honest expected timings and route changed paths to them:
- `projection`: projection state, canonical revisions, Tantivy rebuild, and projection health tests.
- `transport`: adapter, IPC, HTTP transport shim, and protocol parity tests once HTTP exists.
- `lifecycle`: daemon lifecycle controller, restart handoff, session drain, and readiness tests.
- `workspace-runtime`: workspace pool, registry activation, watcher ownership, and cleanup liveness tests.

These buckets may initially reuse existing test filters while the architecture plans create more focused tests. That is acceptable only if the manifest says which current filter backs the bucket.

**Acceptance criteria:**
- [ ] `cargo xtask test list` shows the new buckets and the tiers that include them.
- [ ] `src/search/projection.rs` changes select `projection`.
- [ ] `src/daemon/lifecycle.rs` changes select `lifecycle`.
- [ ] `src/adapter/mod.rs` changes select `transport`.
- [ ] `src/daemon/workspace_pool.rs` changes select `workspace-runtime`.
- [ ] Worker-scope verification passes with exact xtask test names.

### Task 4: Verification Ledger Template

**Files:**
- Create: `docs/plans/verification-ledger-template.md`
- Modify: `docs/TESTING_GUIDE.md`
- Modify: `AGENTS.md`

**What to build:** Create a standard ledger section every plan can copy. It must record invariant, command, scope label, commit SHA, result, timestamp, and whether the evidence was reused.

**Approach:** Keep the template short and operational. Include one example for a worker exact-test run, one for `cargo xtask test changed`, and one for an expensive gate such as `cargo xtask test dogfood`.

**Acceptance criteria:**
- [ ] The template has no placeholders that would be left in committed plan evidence.
- [ ] `docs/TESTING_GUIDE.md` explains when evidence can be reused at the same HEAD.
- [ ] `AGENTS.md` points to the ledger template and preserves the existing TDD rules.
- [ ] Worker-scope verification passes with exact docs or xtask contract tests.

### Task 5: CLI Dogfood Contract

**Files:**
- Modify: `docs/TESTING_GUIDE.md`
- Modify: `JULIE_AGENT_INSTRUCTIONS.md`
- Modify: `src/cli_tools/mod.rs` only if output metadata is needed for the test contract.
- Test: `src/tests/cli_execution_tests.rs`
- Test: `src/tests/cli_tools_tests.rs`

**What to build:** Document a clear standalone dogfood contract: CLI standalone proves tool behavior against a local handler; it does not prove daemon transport, restart, or session routing. Preserve `CliExecutionMode` in CLI output evidence so ledger entries can cite which execution path ran.

**Approach:** Prefer documentation and existing output fields first. If code changes are needed, add a small `mode` or warning field to CLI JSON output when `--standalone` is used, reusing `CliExecutionMode`.

**Acceptance criteria:**
- [ ] `docs/TESTING_GUIDE.md` names which defects standalone CLI can catch.
- [ ] `docs/TESTING_GUIDE.md` names which defects require daemon/MCP or transport tests.
- [ ] Agent instructions recommend standalone CLI for quick tool behavior checks before live MCP tests.
- [ ] CLI JSON output keeps or exposes enough mode metadata to cite in dogfood evidence.
- [ ] Worker-scope verification passes with exact CLI tests.

## Verification Strategy

**Project source of truth:** `AGENTS.md`, `RAZORBACK.md`, `docs/TESTING_GUIDE.md`, and `xtask/test_tiers.toml`.

**Worker red/green scope:** Workers run exact tests only, for example `cargo nextest run -p xtask changed_tests_changed_selection_reports_fallback_prefix` or the exact `cargo nextest run --lib <test_name> 2>&1 | tail -10` command for CLI behavior.

**Worker ceiling:** Workers may run exact tests in `xtask` or exact Julie lib tests assigned to their task. Workers do not run `cargo xtask test changed`, `cargo xtask test dev`, `cargo xtask test full`, or broad `cargo nextest run --lib`.

**Worker gate invariant:** The exact test must prove the policy change it owns: metadata parsing, changed-selection rationale, specialist bucket routing, ledger documentation contract, or CLI dogfood mode semantics.

**Lead affected-change scope:** After each coherent batch, run `cargo xtask test changed`. The lead records the changed-selection rationale in the ledger.

**Branch gate:** Run `cargo xtask test dev` once before handoff.

**Replay/metric evidence:** Bucket elapsed time is report-only unless a contract test asserts it. Changed-selection correctness and manifest parsing are hard gates.

**Escalation triggers:** Run `cargo xtask test system` if CLI bootstrap, daemon startup, or workspace initialization behavior changes. Run `cargo xtask test dogfood` only if this plan changes search quality fixtures or dogfood indexing behavior, which it should avoid.

**Assigned verification failure:** Workers stop and report when assigned verification fails, unless this plan explicitly says to update that gate.

**Verification ledger:** Record invariant, command, scope label, commit SHA, result, and timestamp. For reused expensive evidence, record the original ledger entry and confirm the commit SHA matches.

## Model Routing

**Project source of truth:** `RAZORBACK.md`. Do not copy the global model table into this plan. If a local sentence conflicts with `RAZORBACK.md`, `RAZORBACK.md` wins.

**Plan-specific overrides:** Verification policy, xtask bucket routing, changed-file selection, ledger semantics, specialist-gate ownership, and test timing policy are strategy or gate-review work. Use Codex `gpt-5.5 high` for lead-owned verification-contract decisions and `gpt-5.3-codex high` for reviewing runner or bucket-routing diffs. Implementation-tier workers may edit runner or manifest code only after the strategy contract names the invariant and exact worker ceiling.

**Worker eligibility:** Implementation-tier workers are eligible when file ownership is narrow, the task owns exact xtask tests, and the task does not reinterpret project-wide verification policy.

**Escalation triggers:** Escalate if a worker proposes broad test execution, if a manifest change silently moves a bucket between tiers, or if a test timing change suggests the bucket model is stale.

**Mechanical exclusion:** Mechanical workers cannot own failing tests, timing gate interpretation, changed-selection routing, or branch acceptance evidence.

**Unsupported harness behavior:** If the harness cannot choose models per agent, use `inherit`, note it in the worker report, and continue.

## Task Decomposition

- Task 1 owns manifest metadata and summary output.
- Task 2 owns changed-selection rationale.
- Task 3 owns new specialist buckets and changed-path routing.
- Task 4 owns the ledger template and documentation contract.
- Task 5 owns CLI dogfood contract language and small output metadata only if documentation is insufficient.

Tasks 1 and 2 can run in parallel. Task 3 should run after Task 1 so it can use the new metadata. Task 4 can run in parallel with Task 1. Task 5 can run after Task 4 so it uses the ledger language consistently.

## Risks

- Adding bucket metadata without contract tests could make the manifest look stricter while remaining unenforced.
- Splitting buckets too aggressively can produce false confidence if changed-path routing misses shared infrastructure.
- CLI standalone dogfood is useful but narrower than live MCP behavior. The docs must say that plainly.
- Runtime warnings must be concise. If `xtask` output becomes noisy, agents will stop reading it, and then we are back in policy theater.
