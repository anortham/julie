# Test Velocity Rescue: Design

**Status:** Draft (awaiting user review)
**Date:** 2026-04-17
**Author:** Main session (Alan + Claude)
**Target release:** Pre-v6.10.0 or early v6.10.x

## Problem

Test iteration time has killed velocity in this repo. A typical agent session spends 2 hours wall-clock delivering ~10 minutes of real progress, with the remainder absorbed by compile, link, and full-suite test runs. Three distinct pain threads combine to produce this:

1. **Compile and link cost per iteration.** Default dev profile, monolithic lib test binary, default macOS linker, no build cache. Small source edits trigger full relink of one large binary.
2. **Test execution cost once compiled.** Serial buckets plus pathological fixtures: `workspace-init` at 360s, a 164s dogfood test hiding inside `tools-misc`, and a 100 MB SQLite fixture load in `search-quality`.
3. **Agent discipline.** Agents default to `cargo xtask test dev` for tiny edits because it's culturally the "safe" option, even though the newly added `cargo xtask test changed` command is cheaper for almost every real change.

## Goals

- Cut `cargo xtask test dev` warm-cache wall-clock to under 3 minutes.
- Cut `workspace-init` from 360s to under 60s.
- Make the `changed` selector the default agent workflow, with guardrails that block the worst habits.
- Preserve all existing test coverage and behavior. No tests skipped.

## Non-goals

- Rewriting the test runner from scratch.
- Replacing the bucket-based xtask system with a different abstraction.
- Migrating off SQLite or Tantivy.
- Parallelizing buckets across processes in xtask (nextest already parallelizes across tests inside a binary; cross-bucket parallelism is out of scope because of file-lock interactions).
- Adding more xtask tiers or reshuffling the existing tier names.

## Phase 1: Configuration wins (low risk, high leverage)

### 1.1 Install and adopt `cargo-nextest`

**Change:** Install `cargo-nextest` on dev machines (`cargo install cargo-nextest --locked`). Swap every `cargo test --lib ...` inside `xtask/test_tiers.toml` to `cargo nextest run --lib ...`. Keep the `-- --skip search_quality` filter as-is (nextest passes unrecognized args through to the test binary).

**Acceptance criteria:**
- [ ] `cargo nextest run --lib tests::cli_tests` passes locally with the same tests as the current cargo-test invocation.
- [ ] `xtask/test_tiers.toml` uses `cargo nextest run` across all buckets.
- [ ] Nextest output on success is the summary line only; failures show the same detail as today.
- [ ] Existing xtask regression tests still pass (`cargo test -p xtask`).

**Risks:**
- Nextest does not support doc tests. Confirm the codebase has no doc tests by grepping for `/// ```` patterns. If any exist, preserve them via a separate `cargo test --doc` invocation in the appropriate bucket.
- Some tests may rely on in-process single-threaded state. If any test relies on ambient process state that nextest's process-per-test model breaks, fix the test (use `serial_test` or per-test state) instead of abandoning nextest.

### 1.2 Fast linker on macOS

**Change:** Add to `.cargo/config.toml`:

```toml
[target.'cfg(target_os = "macos")']
rustflags = ["-C", "link-arg=-fuse-ld=lld"]
```

Document the one-time setup requirement in `docs/DEVELOPMENT.md`: `brew install llvm` and ensure `/opt/homebrew/opt/llvm/bin` is on PATH, or install `lld` via any equivalent path.

**Acceptance criteria:**
- [ ] `cargo build --release` still produces a working `julie-server` binary after the change.
- [ ] Link-only phase of `cargo test --lib --no-run` drops by at least 30% in a before/after measurement.
- [ ] `docs/DEVELOPMENT.md` has a section describing the linker setup with the troubleshooting note.

**Risks:**
- `lld` on macOS has occasional quirks with debug info and Tantivy's bundled assets. If we hit breakage, fall back to the default linker (removing the `rustflags` block) and document it as a skipped optimization; do not block Phase 1 on it.

### 1.3 Pre-optimize dev deps

**Change:** Append to `Cargo.toml`:

```toml
[profile.dev.package."*"]
opt-level = 2
```

Keep `[profile.dev]` at `opt-level = 0` for our own code. First build after this change will be slower; every subsequent build benefits from optimized deps and test runtime also drops.

**Acceptance criteria:**
- [ ] `cargo test --lib tests::cli_tests` warm-cache wall-clock after the first rebuild is measurably faster (target: 20%+ improvement).
- [ ] Debug builds of `julie-server` still launch and pass smoke tests.

**Risks:**
- One-time recompile cost to prime the dep cache. Accept.

### 1.4 Move the 164s dogfood test out of `tools-misc`

**Change:**
- Identify the specific dogfood test inside `src/tests/tools/get_symbols_target_filtering.rs` that indexes the full repo (~164s).
- Move it into its own bucket in `xtask/test_tiers.toml`, named `tools-dogfood-repo-index` or similar.
- Add the new bucket to the `dogfood` tier (not the `dev` tier).
- Update `xtask/src/changed.rs` so changes that hit the dogfood-repo-index path route to the new bucket, not `tools-misc`.
- Drop `tools-misc` `expected_seconds` from 200s to ~40s and `timeout_seconds` to ~120s.

**Acceptance criteria:**
- [ ] The old `tools-misc` bucket completes in under 45s locally.
- [ ] The extracted dogfood bucket runs the full-repo-index test and completes in under 200s.
- [ ] `cargo xtask test changed` against an edit that touches the relevant path selects the new bucket.
- [ ] Existing xtask regression tests pass, and a new regression test confirms the routing change.

**Risks:**
- Routing edge cases where the old test module covered other tests that should stay in `tools-misc`. Verify by reading `get_symbols_target_filtering.rs` before moving.

### 1.5 Trim `target/` and install `sccache`

**Change:**
- Run `cargo clean` to drop the 49 GB of accumulated debug artifacts.
- Install `sccache` (`cargo install sccache --locked`).
- Set `RUSTC_WRAPPER=sccache` in shell init (document in `docs/DEVELOPMENT.md`).
- Configure `SCCACHE_DIR` outside the repo tree, default `~/.cache/sccache`.

**Acceptance criteria:**
- [ ] `target/` size after a fresh `cargo build --release && cargo test --lib --no-run` is under 20 GB.
- [ ] Sccache hit rate on a warm cache (second build) is above 70%, measured via `sccache --show-stats`.
- [ ] `docs/DEVELOPMENT.md` describes the setup.

**Risks:**
- Sccache can interact poorly with incremental compilation. Disable incremental compilation under sccache (document in setup): `CARGO_INCREMENTAL=0`. That loses incremental but gains cross-branch cache, which is the bigger win for this repo.

## Phase 2: Runner and pathology fixes (meaningful work, measurable wins)

### 2.1 Compile-once-run-many in xtask

**Change:**
- Modify `xtask/src/runner.rs` so that before running a bucket sequence, it invokes `cargo nextest run --no-run --lib` once, then each bucket command is rewritten to `cargo nextest run --lib --no-fail-fast <filter>` (which reuses the compiled test binary from step one).
- The `run_named_buckets` function grows a "build phase" that runs before any bucket.
- If the build fails, report it cleanly and abort without running any buckets.

**Acceptance criteria:**
- [ ] Running `cargo xtask test dev` invokes a single compile and then runs buckets without triggering recompilation between them.
- [ ] Bucket invocation overhead (time between `END bucket-a` and `START bucket-b`) drops below 1s.
- [ ] `cargo test -p xtask` passes; an added test confirms the build-phase behavior.

**Risks:**
- If any bucket uses features or filters that nextest cannot resolve with a shared build, the shared compile falls apart. Verify all bucket commands use the same feature set first.

### 2.2 Diagnose and fix `workspace-init` pathology

**Change:**
1. Profile the 10 slowest tests in `src/tests/core/workspace_init.rs` using nextest's `--message-format libtest-json` or similar.
2. For each slow test, inventory what it does: full index rebuild, sidecar spawn, fixture setup, filesystem I/O.
3. Triage fixes:
   - Shared fixture via `OnceCell` where tests only need read access.
   - In-memory SQLite where the test does not exercise disk paths.
   - Skip sidecar where the test does not require embeddings.
   - Parallel-friendly setup where `serial_test` is used defensively but not required.
4. Apply fixes one test at a time and re-measure.

**Acceptance criteria:**
- [ ] `workspace-init` bucket completes in under 60s.
- [ ] Every test that was modified keeps its original assertions and semantic coverage.
- [ ] `expected_seconds` and `timeout_seconds` in `xtask/test_tiers.toml` are reset to match the new reality.

**Risks:**
- Some workspace-init tests may genuinely need full initialization. If after triage the bucket lands at 90s instead of 60s, accept that and move on; document what was investigated in a checkpoint.

### 2.3 Share `search-quality` 100 MB fixture

**Change:**
- Audit the fixture load in `src/tests/tools/search/search_quality/`.
- Wrap the SQLite fixture open in a module-scoped `OnceCell<Arc<SymbolDatabase>>` or equivalent shared handle.
- Ensure tests that mutate index state either clone into an isolated workspace or snapshot/restore around the shared fixture.

**Acceptance criteria:**
- [ ] `search-quality` bucket completes in under 200s (down from 390s).
- [ ] All existing `search_quality` tests pass.

**Risks:**
- A shared fixture that is accidentally mutated becomes a source of flakiness. Review every test in the bucket for write paths and gate them with isolation.

## Phase 3: Agent guardrails (short, lasting effect)

### 3.1 Tighten CLAUDE.md and AGENTS.md guidance

**Change:**
- Replace the "Default Workflow" section with a tighter version that puts `cargo test --lib <exact_name>` first, `cargo xtask test changed` second, and `cargo xtask test dev` third (only for completed batches or before handoff).
- Add a concrete example at the top of the testing section: "You edited one function. The right command is `cargo test --lib <the_test_name>`. Not `dev`. Not `changed`. The narrowest test you have."
- Remove any language that frames `dev` as the safe default.

**Acceptance criteria:**
- [ ] CLAUDE.md and AGENTS.md diff shows the change; the pre-commit hook keeps them in sync.
- [ ] Reading the first 50 lines of the testing section gives an agent the correct command hierarchy without scrolling.

**Risks:** None substantive.

### 3.2 PreToolUse hook that catches broad test runs

**Change:**
- Add a PreToolUse hook in `.claude/hooks/` (and mirror it into the plugin's `hooks/` for distribution) that fires on Bash commands matching `cargo xtask test dev`, `cargo xtask test full`, or bare `cargo test --lib` with no filter.
- The hook blocks the call and prints a message pointing to `cargo xtask test changed` and the narrow-filter alternatives.
- Include a bypass: if the environment variable `CLAUDE_ALLOW_BROAD_TESTS=1` is set, the hook allows the call. The orchestrator flips that when a batch-level run is genuinely needed.

**Acceptance criteria:**
- [ ] An agent session that tries `cargo xtask test dev` is blocked with a clear message.
- [ ] Setting `CLAUDE_ALLOW_BROAD_TESTS=1` allows the call.
- [ ] Narrow commands (`cargo test --lib <name>`, `cargo xtask test changed`) pass through the hook without interference.

**Risks:**
- If the hook is too aggressive it will frustrate the human, not only agents. Confirm the regex with Alan before committing.

### 3.3 SessionStart reminder

**Change:**
- Extend the existing SessionStart hook to include a one-liner: "Tests: prefer `cargo test --lib <name>` or `cargo xtask test changed`. `dev` is a batch gate, not an edit-loop tool."

**Acceptance criteria:**
- [ ] Every new session prints the reminder once at startup.
- [ ] The reminder stays under 2 lines.

**Risks:** None.

## Sequencing rationale

- Phase 1 items are independent and all provide immediate, measurable wins. Do them first so Phase 2 has a fast baseline to measure against.
- Phase 2 is where the biggest single improvements land (`workspace-init`, shared fixture). They need Phase 1's faster runner to keep the investigation loop tolerable.
- Phase 3 lands last because guardrails that point agents at "the fast path" rely on the fast path existing.

## Acceptance markers (end-to-end)

- [ ] After Phase 1: `cargo xtask test dev` clean-cache wall-clock drops by at least 30%; `tools-misc` drops to under 45s.
- [ ] After Phase 2: `cargo xtask test dev` warm-cache completes under 3 minutes; `workspace-init` bucket under 60s.
- [ ] After Phase 3: new agent sessions default to narrow tests on trivial edits (verified by reading transcripts of the next 2-3 sessions).

## Rollback plan

- Phase 1: revert the commits individually. Each is a self-contained configuration change.
- Phase 2: each sub-item is its own commit; revert affected commits and the old behavior returns.
- Phase 3: deactivate the PreToolUse hook by removing its entry from `.claude/hooks/hooks.json`; CLAUDE.md edits are plain reverts.

## Open questions

- Does Alan want `lld` on macOS or should we accept the default linker and skip 1.2? (Decision: try `lld`; fall back if it breaks.)
- Should the PreToolUse hook live in the julie repo's `.claude/hooks/` (dev-only), the plugin repo's `hooks/` (distributed to all users), or both? (Recommendation: both. Dev-only copy in julie for repo-local guardrails; distributed copy in plugin for consistency across users.)
- Are there known-slow tests beyond `workspace-init` that should also get the `OnceCell` treatment in Phase 2? (Investigate during 2.2.)
