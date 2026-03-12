# Test Runner and Tiering Design

## Goal

Replace Julie's drifting, docs-only test-tier folklore with a real repo-managed test runner, explicit tier definitions, and documentation that points to one source of truth.

## Problem

The current test strategy is built around hand-written `cargo test` commands copied into `CLAUDE.md`, `AGENTS.md`, `README.md`, plans, and memories. That has broken down.

Observed problems:

- The documented `fast` tier is `cargo test --lib -- --skip search_quality`, but that is no longer predictably fast.
- Documentation still claims the fast tier is around 15 seconds, while observed buckets are much slower:
  - `tests::tools` is about 51 seconds
  - `tests::integration` is about 29 seconds
  - `tests::core` is bad enough that it exceeded a 240 second probe
  - `tests::core::workspace_init` alone exceeded a 120 second probe
- The current split is based on one exclusion (`search_quality`) instead of real execution cost or purpose.
- When a large bucket runs, it is hard to tell whether the process is merely slow or genuinely stuck.
- There is no checked-in runner or manifest to keep `CLAUDE.md`, `AGENTS.md`, and `README.md` honest.

## Goals

- Define test tiers in one repo-managed source of truth.
- Make the default development tier predictable and bounded.
- Print progress and per-bucket timing so slow is distinguishable from broken.
- Reclassify tests by runtime and purpose rather than by directory folklore.
- Update `CLAUDE.md`, `AGENTS.md`, and `README.md` to reference the runner instead of raw cargo command soup.

## Non-Goals

- Rewriting all tests immediately.
- Solving every slow test in the same change.
- Replacing targeted raw `cargo test --lib <filter>` workflows for debugging.
- Building a custom CI platform.

## Proposed Approach

### 1. Add a Real Runner

Add a small checked-in Rust runner, exposed as `cargo xtask test ...`.

Canonical source of truth:

- Add a checked-in manifest at `xtask/test_tiers.toml`.
- This manifest is the only place where tier membership, bucket membership, expected duration, and timeout budgets are defined.
- The Rust `xtask` runner loads this file and executes it.
- `CLAUDE.md`, `AGENTS.md`, and `README.md` describe policy and supported commands, but do not duplicate bucket membership.

Responsibilities:

- Load tier and bucket definitions from `xtask/test_tiers.toml`.
- Run buckets sequentially.
- Print `START` and `END` markers for each bucket.
- Print elapsed time per bucket and total elapsed time.
- Fail with clear context when a bucket exits non-zero or exceeds its configured timeout.
- Support running one named bucket directly for local narrowing.

Rust `xtask` is preferred over shell glue because it is cross-platform, versioned in the repo, and can evolve without depending on local shell quirks.

### 2. Replace the Current Tier Model

Replace `fast = everything except search_quality` with explicit intent-based tiers.

Proposed tiers:

- `smoke`
  - Tiny sanity checks.
  - Used after very small changes or when validating that the repo is not obviously broken.
  - Hard rule: no bucket with expected runtime above 15 seconds belongs here.
  - Target budget: under 1 minute total.
- `dev`
  - Default day-to-day confidence tier.
  - Used after most normal code changes.
  - Hard rule: no bucket with expected runtime above 45 seconds belongs here.
  - Target budget: about 2 to 5 minutes max total.
- `system`
  - Slower integration-heavy Rust tests that are still relevant before landing a branch.
  - Used before merge and after broader internal refactors.
  - Hard rule: any bucket with expected runtime above 45 seconds and below dogfood scale lands here.
  - Hard rule: suites with heavy serial/global-environment behavior can also land here even if they are below 45 seconds.
- `dogfood`
  - Expensive realism tests such as `search_quality` and other large-fixture suites.
  - Used after search/scoring changes and before merging search-sensitive work.
  - Hard rule: any suite with expected runtime above 180 seconds lands here.
  - Hard rule: any suite that depends on large fixtures (roughly 50MB+), full reindexing/backfill, or corpus-quality assertions lands here.
- `full`
  - Composition of all required buckets, including dogfood.
  - Used before merging broad or risky branches.

### 3. Use Buckets as the Real Execution Unit

Each tier is made of named buckets. Buckets are the main operational unit because they give timing, progress, and failure locality.

Initial bucket structure:

- `smoke/cli`
- `smoke/core-database`
- `smoke/core-embeddings`
- `smoke/tools-get-context`
- `dev/tools-search`
- `dev/tools-workspace`
- `dev/tools-misc`
- `dev/core-fast`
- `system/workspace-init`
- `system/integration`
- `dogfood/search-quality`

Initial concrete mapping for known problematic groups:

- `tests::core` is no longer a runnable bucket as a whole.
- `tests::tools` is no longer a runnable bucket as a whole.
- `tests::core::workspace_init` becomes its own `system/workspace-init` bucket.
  - Reason: it exceeded a 120 second direct probe by itself.
- `tests::integration` starts as a `system/integration` bucket.
  - Reason: observed around 29 seconds and integration-heavy by nature.
- `tests::tools::search` becomes `dev/tools-search`.
  - Reason: observed around 19.5 seconds; acceptable for day-to-day feedback.
- `tests::tools::workspace` becomes `dev/tools-workspace` initially.
  - Reason: observed around 36.6 seconds; still tolerable in `dev`, but visible as its own bucket.
- `tests::tools::get_context*` becomes `smoke/tools-get-context` initially.
  - Reason: observed around 3.1 seconds.
- `tests::core::*` except `workspace_init` becomes `core-fast` initially.
  - This includes things like `database`, `embedding_provider`, `embedding_metadata`, and related fast core modules.

Implementation may further split `core-fast` or `tools-misc` if measured runtime is still ugly, but these initial mappings must be the first pass.

### 4. Reclassify Tests by Reality, Not Folder Name

Some current module groups are misleading:

- `tests::core` sounds cheap but is not.
- `tests::core::workspace_init` is a pathological outlier and should not live in a day-to-day tier.
- `tests::tools::get_context` is reasonably sized and could stay in a common dev tier.
- `tests::integration` is not free, but it is still much more understandable as a named bucket than as part of one giant pseudo-fast tier.

Implementation should classify modules based on:

- observed runtime
- fixture size or setup cost
- serial-test constraints
- whether the suite mutates process-global state or shared environment

### 5. Make Timeouts Explicit

Each bucket should have a timeout budget in the runner.

Purpose:

- surface likely hangs or pathological slowdowns as a first-class failure mode
- stop users from staring at a blank terminal wondering if the test process is alive
- create a visible performance contract for each bucket

Timeout model:

- Each bucket entry in `xtask/test_tiers.toml` stores:
  - `expected_seconds`
  - `timeout_seconds`
- Initial rule for setting budgets:
  - `timeout_seconds = max(expected_seconds * 3, expected_seconds + 60)`
  - round up to a simple human-readable value
- The runner should allow a temporary override, e.g. `--timeout-multiplier 2`, for unusually slow machines.
- Timeout failures should say which bucket timed out, what the limit was, what the expected time was, and which command was running.
- If observed runtime drifts materially from `expected_seconds`, updating the manifest becomes part of the maintenance workflow.

### 6. Make Documentation Secondary to the Runner

Update docs so they describe policy but delegate execution details to the runner.

- `CLAUDE.md`
  - Detailed playbook.
  - Which tier to run after which class of change.
  - When `dogfood` is required.
  - How to drop to raw cargo filters when narrowing a failure.
- `AGENTS.md`
  - Shorter rule set.
  - Use the repo runner by default.
  - Do not improvise full-suite commands casually.
- `README.md`
  - User-facing summary of the tier model and the canonical commands.

All three should reference the same runner commands.

## Runner Interface

Final CLI contract:

```bash
cargo xtask test smoke
cargo xtask test dev
cargo xtask test system
cargo xtask test dogfood
cargo xtask test full
cargo xtask test list

# Optional narrowing
cargo xtask test bucket workspace-init
cargo xtask test bucket tools-get-context
```

Rules:

- `cargo xtask test <tier>` runs all buckets in that tier.
- `cargo xtask test bucket <name>` runs exactly one named bucket.
- `cargo xtask test list` prints tiers, buckets, commands, expected times, and timeout budgets.
- There is no second bucket syntax. The CLI should have one boring shape and stick to it.

Example output shape:

```text
[1/4] START dev/core-fast
[1/4] PASS  dev/core-fast (12.4s)
[2/4] START dev/tools-get-context
[2/4] PASS  dev/tools-get-context (3.1s)
[3/4] START dev/tools-search
...
SUMMARY: 4 buckets passed in 68.2s
```

This is deliberately boring. Boring is good. Boring means people trust it.

## Documentation Migration Plan

- Replace the current `fast`, `dogfood`, and `full` raw cargo examples with runner commands.
- Remove stale hardcoded timings unless they are generated or periodically revalidated.
- Keep one section showing how to drop to a raw cargo filter for debugging specific failures.
- Update future plan templates or agent guidance to use the runner in examples.

## Verification Strategy

The implementation should validate:

- the runner can execute each tier successfully
- bucket output makes progress visible
- timeout failures are readable
- documented commands in `CLAUDE.md`, `AGENTS.md`, and `README.md` match the runner
- the default `dev` tier stays within the intended budget on a normal development machine

## Risks

- Bucket definitions may still be wrong on the first pass; they need measured feedback, not guessing.
- Some slow modules may need to be split before the new tier model feels good.
- If the runner becomes too clever, it will be annoying to maintain; keep it simple.

## Recommendation

Implement the runner and tier manifest first, then move the most obviously bad outliers out of the default development tier, then update docs to make the runner canonical. That fixes the source-of-truth problem instead of merely renaming the old mess.
