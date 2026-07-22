# Xtask Runner Module Boundary Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Complete Phase 2A by decomposing `xtask/src/runner.rs` into focused modules without changing its public API, CLI output, timing semantics, or process-timeout behavior.

**Architecture:** Keep `xtask::runner` as the facade and home of the public runner types and orchestration entry points. Move prebuild derivation, process/bucket execution, and summary rendering into private child modules, re-exporting the same public functions and executor type from the existing path.

**Tech Stack:** Rust, Cargo xtask, cargo-nextest, lld.

**Architecture Quality:** Affected module is `xtask::runner`; caller-facing interfaces remain `CommandExecutor`, `ProcessCommandExecutor`, `run_tier`, `run_named_buckets`, `run_bucket`, `render_*`, `resolve_bucket_plan`, and `declared_expected_seconds`. The facade stays deep by hiding command parsing, process control, streaming output, and presentation details; risk is medium because timeout and output ordering are caller-visible.

## Global Constraints

- Preserve all existing public names, signatures, re-exports, return values, error text, CLI text, output ordering, timing accumulation, and process-tree termination behavior.
- Do not combine the file split with runner behavior changes.
- Keep every new implementation file at or below 500 lines and the test module at or below 1000 lines.
- Keep tests on the existing caller-facing `xtask::runner` interface; structural coverage may inspect file boundaries.
- Use the pinned toolchain from `rust-toolchain.toml` after the release-toolchain prerequisite lands.
- Follow TDD: add the failing module-size boundary test before moving implementation.

---

## Verification Strategy

**Project source of truth:** `AGENTS.md`, `docs/TESTING_GUIDE.md`, `xtask/test_tiers.toml`, and the approved Julie improvement roadmap.

**Worker red/green scope:** `cargo nextest run -p xtask --test runner_boundary_tests runner_implementation_files_stay_within_limit`.

**Worker ceiling:** The worker runs only the exact new boundary test once red and once green. The lead owns the existing runner suites and broader regression gates.

**Worker gate invariant:** The runner facade and each production child module stay at or below 500 lines while the expected private module boundaries exist.

**Lead affected-change scope:** `cargo xtask test changed` after the complete split; this must select the `xtask-runner` bucket.

**Branch gate:** `cargo xtask test dev` once after affected-change verification passes.

**Replay/metric evidence:** Existing runner and coverage suites are hard behavior gates. File line counts and exact public symbol paths are hard structural gates; compile and test durations are report-only.

**Escalation triggers:** Any public API change, output snapshot change, timeout/process-tree failure, changed-bucket routing change, or file above its limit blocks completion. No dogfood or system tier is required unless live impact analysis discovers product-code changes.

**Assigned verification failure:** Workers stop and report when assigned verification fails, unless this plan explicitly says to update that gate.

**Verification ledger:** Record invariant, command, scope label, commit SHA, result, and timestamp in `docs/plans/2026-07-22-runner-module-boundary-verification.md`. Evidence is reusable only at the exact recorded HEAD and scope.

## Parallel Execution Contract

| Task | Parallel batch | File ownership | Serialization required | Dependency reason |
|---|---|---|---|---|
| Task 1: Split the runner behind its facade | None - serial | Create `xtask/src/runner/prebuild.rs`, `xtask/src/runner/execution.rs`, `xtask/src/runner/rendering.rs`, `xtask/src/runner/tests.rs`, `xtask/tests/runner_boundary_tests.rs`, and `docs/plans/2026-07-22-runner-module-boundary-verification.md`; modify `xtask/src/runner.rs` only | Not applicable - single task. | Not applicable - single task. |

### Task 1: Split the runner behind its facade

**Files:**
- Create: `xtask/src/runner/prebuild.rs`
- Create: `xtask/src/runner/execution.rs`
- Create: `xtask/src/runner/rendering.rs`
- Create: `xtask/src/runner/tests.rs`
- Create: `xtask/tests/runner_boundary_tests.rs`
- Create: `docs/plans/2026-07-22-runner-module-boundary-verification.md`
- Modify: `xtask/src/runner.rs:1-1242`

**Interfaces:**
- Consumes: `TestManifest`, `BucketConfig`, `CommandExecutor`, `CommandOutcome`, `CommandResult`, and the current `xtask::runner` public surface.
- Produces: the identical `xtask::runner` public paths and behavior, backed by private `prebuild`, `execution`, and `rendering` modules.

**Contract inputs:** `xtask/src/main.rs`, `xtask/src/changed.rs`, `xtask/src/lib.rs`, `xtask/tests/runner_tests.rs`, and `xtask/tests/runner_coverage_tests.rs` must compile unchanged. `run_named_buckets` must prebuild before executing buckets; failures must preserve partial structured summaries; elapsed time and captured output rules must not move across semantic boundaries.

**File ownership:** Create `xtask/src/runner/prebuild.rs`, `xtask/src/runner/execution.rs`, `xtask/src/runner/rendering.rs`, `xtask/src/runner/tests.rs`, `xtask/tests/runner_boundary_tests.rs`, and `docs/plans/2026-07-22-runner-module-boundary-verification.md`; modify `xtask/src/runner.rs` only

**Serialization required:** Not applicable - single task.

**Dependency reason:** Not applicable - single task.

**What to build:** Add a failing structural contract for the 500-line implementation limit, then turn `runner.rs` into a facade. Move coverage-command and prebuild derivation/execution into `prebuild.rs`; process launching, timeout termination, bucket execution, and streaming markers into `execution.rs`; manifest/summary/result presentation into `rendering.rs`; and the current inline test module into `tests.rs` without changing assertions.

**Approach:** Keep public data types, `PROGRAM_TIERS`, tier/bucket orchestration, bucket-plan resolution, and declared-budget pricing in the facade. Use private `pub(super)` seams only where the facade or a sibling module must call implementation; re-export the existing public executor and rendering/coverage functions so callers require no edits. Apply rustfmt only to touched runner files under the pinned formatter.

**Acceptance criteria:**
- [ ] The exact boundary test fails on the current 1242-line `runner.rs` and passes after the split.
- [ ] `xtask/src/runner.rs`, `prebuild.rs`, `execution.rs`, and `rendering.rs` are each at or below 500 lines; `tests.rs` is at or below 1000 lines.
- [ ] `xtask/src/main.rs`, `xtask/src/changed.rs`, `xtask/src/lib.rs`, and existing integration tests require no caller-facing changes.
- [ ] `cargo nextest run -p xtask --test runner_tests` passes unchanged.
- [ ] `cargo nextest run -p xtask --test runner_coverage_tests` passes unchanged.
- [ ] `cargo nextest run -p xtask` passes as the lead-owned focused gate.
- [ ] `cargo xtask test changed` selects and passes the `xtask-runner` bucket.
- [ ] The verification ledger records all hard gates at the exact implementation commit.
- [ ] Worker-scope verification passes and the change is either committed by the worker or handed to the lead per commit mode.
