# Test Velocity Rescue Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use razorback:executing-plans to implement this plan task-by-task, or razorback:team-driven-development to dispatch a team.

**Goal:** Cut `cargo xtask test dev` warm-cache wall-clock from ~6+ minutes to under 3 minutes and make narrow tests the cultural default for agents.

**Architecture:** Three phases. Phase 1 lands configuration wins (nextest, faster linker, dev-profile dep opt, dogfood extraction, sccache). Phase 2 fixes the xtask runner and two pathological test paths. Phase 3 adds agent guardrails so the fast path we built is the one agents take.

**Tech Stack:** Rust, cargo-nextest, lld, sccache, xtask, Claude Code hooks.

**Design doc:** `docs/plans/2026-04-17-test-velocity-design.md`

**Execution notes:**
- Plan depth: **light** (same-session team execution via `razorback:team-driven-development`).
- Worktree: none. User prefers visible team collaboration on main.
- TDD expectation: every task follows RED → GREEN → commit. Subagents run only the narrow test for their change; the lead handles batch-level regression with `cargo xtask test changed` after each batch.

---

## Batch A: Phase 1 configuration wins

### Task A1: Adopt cargo-nextest in xtask buckets

**Files:**
- Install: `cargo install cargo-nextest --locked` (human runs; document in task report)
- Modify: `xtask/test_tiers.toml` (every `cargo test --lib ...` becomes `cargo nextest run --lib ...`)
- Modify: `xtask/src/runner.rs:93-103` (SPECIAL_BUCKETS entry for `system-health`)
- Modify: `xtask/src/changed.rs` (no command rewrite needed; only if any inline command strings exist; grep to confirm)
- Test: `xtask/tests/runner_tests.rs` and `xtask/tests/changed_tests.rs` (update any hardcoded expectations)

**What to build:** Replace `cargo test` with `cargo nextest run` across all bucket command strings. Nextest parallelizes tests across processes, so execution gets faster with zero code changes. The `-- --skip search_quality` tail still works; nextest passes unknown args through.

**Approach:**
- Grep the repo for `cargo test --lib` in `xtask/` and swap each to `cargo nextest run --lib`. Leave `cargo test -p xtask` as-is because nextest's xtask test discovery behavior is fine, but confirm by running once.
- Preserve every `-- --skip ...` filter.
- The `system-health` special bucket in `runner.rs:93-103` also needs updating.
- For any doc tests (if grep finds `/// \`\`\``, i.e. rustdoc code samples that run as tests), add a companion `cargo test --doc` command since nextest does not run doc tests.
- Confirm xtask regression tests still pass.

**Acceptance criteria:**
- [ ] `cargo nextest --version` returns a version (documented as a prerequisite).
- [ ] Every bucket command string in `xtask/test_tiers.toml` uses `cargo nextest run`.
- [ ] `cargo test -p xtask` passes.
- [ ] `cargo xtask test cli` executes and completes faster than the pre-change baseline (measure and report).
- [ ] Committed as a single commit: `refactor(xtask): adopt cargo-nextest across buckets`.

### Task A2: Fast linker on macOS

**Files:**
- Modify: `.cargo/config.toml`
- Modify: `docs/DEVELOPMENT.md` (add linker setup section)

**What to build:** Configure `lld` as the linker on macOS via `.cargo/config.toml`. Cuts link time on the monolithic test binary by a large factor.

**Approach:**
- Append to `.cargo/config.toml`:
  ```toml
  [target.'cfg(target_os = "macos")']
  rustflags = ["-C", "link-arg=-fuse-ld=lld"]
  ```
- Add a "Fast linker setup" section to `docs/DEVELOPMENT.md` describing: `brew install llvm` and ensuring the Homebrew llvm bin is on PATH, with a fallback note that removing the `rustflags` block restores the default linker if `lld` misbehaves on their machine.
- Verify a release build still succeeds: `cargo build --release` (user runs; subagent reports).

**Acceptance criteria:**
- [ ] `.cargo/config.toml` contains the macOS rustflags block.
- [ ] `docs/DEVELOPMENT.md` has a "Fast linker setup" section.
- [ ] `cargo build --release` succeeds on macOS.
- [ ] Test-binary link time drops by at least 30% compared to pre-change baseline (measure with `time cargo test --lib --no-run` before and after).
- [ ] Committed as: `build: enable lld linker on macOS`.

### Task A3: Pre-optimize dev profile dependencies

**Files:**
- Modify: `Cargo.toml` (add `[profile.dev.package."*"]` section after existing `[profile.dev]`)

**What to build:** Heavy deps (tantivy, tree-sitter, tokio, rmcp, rusqlite) compile optimized in dev builds so test runtime drops and incremental rebuilds stay fast. Our own code stays at opt-0 so edit compiles are fast.

**Approach:**
- Append to `Cargo.toml`:
  ```toml
  [profile.dev.package."*"]
  opt-level = 2
  ```
- Do NOT change `[profile.dev]` itself; the wildcard applies only to dependencies.
- Accept the one-time recompile cost to prime the cache.

**Acceptance criteria:**
- [ ] `Cargo.toml` has the `[profile.dev.package."*"] opt-level = 2` stanza.
- [ ] `cargo build --lib` succeeds after a `cargo clean`.
- [ ] Smoke test: a narrow test (`cargo nextest run --lib tests::cli_tests`) runs at least as fast as before on warm cache.
- [ ] Committed as: `build: optimize dev-profile dependencies`.

### Task A4: Extract the dogfood outlier from tools-misc

**Files:**
- Modify: `src/tests/tools/get_symbols_target_filtering.rs` (extract the single non-ignored test)
- Create: `src/tests/tools/get_symbols_target_filtering_dogfood.rs` (new home for the extracted test)
- Modify: `src/tests/tools/mod.rs` (register the new test module)
- Modify: `xtask/test_tiers.toml` (remove `get_symbols_target_filtering` from `tools-misc` commands list, add a new `tools-dogfood-repo-index` bucket, add it to the `dogfood` tier, drop `tools-misc` expected/timeout seconds)
- Modify: `xtask/src/changed.rs` (route `src/tests/tools/get_symbols_target_filtering_dogfood.rs` to the new bucket)
- Modify: `xtask/tests/changed_tests.rs` (add regression test for the new routing)

**What to build:** The non-ignored test `test_target_minimal_mode_includes_body_for_child_symbols` in `get_symbols_target_filtering.rs` indexes the full julie repo (~164s). It's the sole reason the `tools-misc` bucket needs 200s budget. Move it into its own bucket outside the local-dev loop.

**Approach:**
- Copy `test_target_minimal_mode_includes_body_for_child_symbols` (with its imports and helper) from `get_symbols_target_filtering.rs` into a new file `get_symbols_target_filtering_dogfood.rs`. Delete it from the original.
- Register the new file in `src/tests/tools/mod.rs` (alphabetical ordering matches the existing pattern).
- Update `xtask/test_tiers.toml`:
  - Remove the `get_symbols_target_filtering` line from `tools-misc` commands.
  - Add new bucket `[buckets.tools-dogfood-repo-index]` with command `"cargo nextest run --lib tests::tools::get_symbols_target_filtering_dogfood"`, `expected_seconds = 200`, `timeout_seconds = 450`.
  - Add `"tools-dogfood-repo-index"` to the `[tiers] dogfood` list (next to `search-quality`).
  - Drop `tools-misc` `expected_seconds` to 40 and `timeout_seconds` to 120.
  - Remove the stale comment about the 164s dogfood test from the `tools-misc` bucket.
- Update `xtask/src/changed.rs`:
  - Add routing for `src/tests/tools/get_symbols_target_filtering_dogfood.rs` into the new `tools-dogfood-repo-index` bucket.
  - Keep `src/tests/tools/get_symbols_target_filtering.rs` routing as-is (still goes to `tools-misc` since the other 4 ignored tests live there).
- Add a new xtask test that asserts the routing.

**Acceptance criteria:**
- [ ] `cargo nextest run --lib tests::tools::get_symbols_target_filtering` completes in under 10s (only the ignored tests, which are skipped, and nothing else left).
- [ ] `cargo nextest run --lib tests::tools::get_symbols_target_filtering_dogfood` runs the extracted test and passes.
- [ ] `cargo xtask test dogfood` includes the new bucket.
- [ ] `cargo xtask test dev` sums to a lower total expected-seconds than before.
- [ ] `cargo test -p xtask` passes with the new routing test.
- [ ] Committed as: `test(xtask): extract repo-index dogfood test from tools-misc`.

**Dependency:** Sequence this AFTER Task A1 (both modify `xtask/test_tiers.toml`).

### Task A5: sccache + target cleanup documentation

**Files:**
- Modify: `docs/DEVELOPMENT.md` (sccache setup section)

**What to build:** Installing sccache across the team gives cross-branch build caching. Document the setup; the human runs `cargo clean` and the install locally.

**Approach:**
- Add a "Build cache (sccache)" section to `docs/DEVELOPMENT.md`:
  - `cargo install sccache --locked`
  - `export RUSTC_WRAPPER=sccache` in shell init
  - `export SCCACHE_DIR=$HOME/.cache/sccache` (default)
  - `export CARGO_INCREMENTAL=0` (required with sccache; lose incremental, gain cross-branch cache)
  - `sccache --show-stats` to verify cache hits
- Include a one-liner telling the user to run `cargo clean` now to reclaim the 49 GB of accumulated debug artifacts.
- This is a documentation-only task; no code change.

**Acceptance criteria:**
- [ ] `docs/DEVELOPMENT.md` has a "Build cache (sccache)" section with all four env vars described.
- [ ] The section explicitly calls out the incremental trade-off.
- [ ] Committed as: `docs: document sccache setup for cross-branch build cache`.

---

## Batch B: Phase 2 runner and pathology fixes

### Task B1: compile-once-run-many in xtask runner

**Files:**
- Modify: `xtask/src/runner.rs:287-345` (`run_named_buckets` function gains a prebuild phase)
- Modify: `xtask/src/cli.rs` (if a new flag is needed to opt out; otherwise no change)
- Modify: `xtask/tests/runner_tests.rs` (add prebuild-phase test)
- Reference: `xtask/test_tiers.toml` (no change; confirm all bucket commands share the same feature set)

**What to build:** Before running the bucket sequence, invoke `cargo nextest run --no-run --lib` once. Each bucket then reuses the cached compiled artifact, saving cargo wrapper overhead per bucket.

**Approach:**
- Add a `prebuild_tests` function to `runner.rs` that calls `cargo nextest run --no-run --lib` and returns its outcome.
- In `run_named_buckets`, call `prebuild_tests` first. If prebuild fails, abort with a clear error before any bucket runs.
- Do not change bucket command strings; nextest is idempotent about the `--no-run` cache. The bucket-level invocations reuse the compiled binary.
- Add a new `FakeExecutor`-based test to `runner_tests.rs` that asserts prebuild is invoked exactly once before any bucket.

**Acceptance criteria:**
- [ ] `run_named_buckets` calls a prebuild step before looping through buckets.
- [ ] A test in `runner_tests.rs` asserts prebuild happens once.
- [ ] `cargo xtask test dev` output shows only one compile phase.
- [ ] Between `END bucket-a` and `START bucket-b`, wall-clock gap drops below 1s on warm cache.
- [ ] Committed as: `refactor(xtask): compile test binary once per run`.

**Dependency:** AFTER Task A1 (nextest must be in use).

### Task B2: Investigate and fix workspace-init pathology

**Files:**
- Profile first, then modify: `src/tests/core/workspace_init.rs` (1150 lines; contains the slow tests)
- Possibly modify: `src/tests/helpers/workspace.rs` (shared fixture helpers)
- Modify: `xtask/test_tiers.toml` (update `expected_seconds` / `timeout_seconds` for the `workspace-init` bucket once the real number is known)

**What to build:** Triage the 360s pathology in `workspace-init`. Cut the total to under 60s without dropping coverage.

**Approach:**
1. Run `cargo nextest run --lib tests::core::workspace_init --message-format libtest-json` and identify the 10 slowest tests.
2. For each slow test, inspect it with `deep_dive` and identify the cost driver. Candidate fixes in priority order:
   - Shared `OnceCell<Arc<...>>` fixture for read-only tests that spin up workspaces.
   - In-memory SQLite where on-disk behavior is not what's under test.
   - Skip sidecar spawn where the test does not exercise embeddings.
   - Remove unnecessary `serial_test` annotations if the test does not need exclusivity.
3. Apply fixes one test at a time and re-measure.
4. Update the manifest's bucket budget to the new reality.

**Acceptance criteria:**
- [ ] Every test in `tests::core::workspace_init` still passes individually.
- [ ] `cargo nextest run --lib tests::core::workspace_init` completes in under 60s on a warm-cache run.
- [ ] `xtask/test_tiers.toml` reflects the new `expected_seconds` and `timeout_seconds` for the `workspace-init` bucket.
- [ ] A checkpoint documents which tests were touched and what was changed.
- [ ] Committed as: `perf(tests): triage workspace-init pathology`.

### Task B3: Share the search_quality fixture via OnceCell

**Files:**
- Read first: `src/tests/mod.rs:81` (module declaration)
- Probable modification target: files under `src/tests/tools/search/` that set up the 100 MB SQLite fixture (the load pattern matches by convention; inspect `quality.rs` and related files first with `deep_dive` / `get_symbols`)
- Possibly modify: `src/tests/fixtures/julie_db.rs`

**What to build:** The 100 MB fixture load should happen once per process, not once per test. Wrap the load in a module-scoped `OnceCell<Arc<SymbolDatabase>>` (or equivalent) and adjust the tests to share it, isolating mutations where needed.

**Approach:**
- Identify where the fixture is opened by inspecting `src/tests/fixtures/julie_db.rs` and the search-quality test files.
- Replace the per-test open with a shared `OnceCell`.
- For any test that mutates fixture state, switch to a cloned or temporary workspace instead of touching the shared handle.
- Verify no test is secretly flaky-on-shared-state by running `search-quality` twice in a row.

**Acceptance criteria:**
- [ ] The 100 MB fixture is loaded at most once per test binary invocation.
- [ ] Every `search_quality` test still passes.
- [ ] `cargo nextest run --lib search_quality` completes in under 200s (down from 390s).
- [ ] Committed as: `perf(tests): share search_quality fixture via OnceCell`.

---

## Batch C: Phase 3 agent guardrails

### Task C1: Tighten CLAUDE.md and AGENTS.md

**Files:**
- Modify: `CLAUDE.md` (the "Default Workflow" section around the test rules)
- Modify: `AGENTS.md` (pre-commit hook keeps the two in sync; still edit both explicitly to be safe)

**What to build:** The testing section currently frames `dev` as the default batch command. Rewrite the workflow hierarchy so narrow tests are the first choice, `changed` is the second, and `dev` is the last.

**Approach:**
- Find the "Default Workflow" list under the test section.
- Rewrite the list as:
  1. Narrow test by name first: `cargo test --lib <exact_test>` (or `cargo nextest run --lib <exact_test>` now that we've adopted nextest).
  2. `cargo xtask test changed` for wider coverage after a batch.
  3. `cargo xtask test dev` only before handoff, after multiple batches.
- Add a concrete "one-function edit" example at the top of the testing section showing the narrow-test command.
- Remove any language framing `dev` as the safe default.
- Update references to `cargo test` in the subagent rules section to `cargo nextest run` where appropriate.

**Acceptance criteria:**
- [ ] `CLAUDE.md` and `AGENTS.md` both show the new hierarchy.
- [ ] The pre-commit hook (`hooks/pre-commit`) reports no drift between the two.
- [ ] First 50 lines of the testing section convey the correct hierarchy without scrolling.
- [ ] Committed as: `docs: make narrow tests the default agent workflow`.

### Task C2: PreToolUse hook that blocks broad test runs

**Files:**
- Create: `.claude/hooks/pretool-broad-tests.cjs` (new hook script)
- Modify: `.claude/hooks/hooks.json` (register the new hook under PreToolUse for Bash)
- Reference for patterns: `.claude/hooks/pretool-edit.cjs` and `.claude/hooks/pretool-agent.cjs`
- Mirror (documented separately in Task C2b): the plugin repo `~/source/julie-plugin/hooks/` after this lands. A followup PR in julie-plugin; out of scope for this plan.

**What to build:** A PreToolUse hook that matches Bash commands containing `cargo xtask test dev`, `cargo xtask test full`, or `cargo test --lib` (or `cargo nextest run --lib`) without a test filter. The hook blocks the call and prints a message pointing to narrower alternatives. A `CLAUDE_ALLOW_BROAD_TESTS=1` environment variable bypasses it.

**Approach:**
- Follow the pattern in `.claude/hooks/pretool-edit.cjs` (read stdin JSON, inspect `tool_input.command` when `tool_name === "Bash"`, return `{ "continue": false, "stopReason": "..." }` to block or silent success to pass through).
- Regex matches:
  - `/\bcargo\s+xtask\s+test\s+(dev|full)\b/` (explicit broad tier)
  - `/\bcargo\s+(nextest\s+run|test)\s+--lib\s*(\s--.*)?$/` (no test-name filter)
- Skip the block if `process.env.CLAUDE_ALLOW_BROAD_TESTS === "1"`.
- Error message points the agent to `cargo xtask test changed` and `cargo nextest run --lib <test_name>`.
- Register in `.claude/hooks/hooks.json` under `PreToolUse` matching `Bash`.
- Include a narrow unit test for the regex behavior if the existing hook infrastructure supports inline tests; otherwise document the matrix in a comment.

**Acceptance criteria:**
- [ ] A Bash tool call of `cargo xtask test dev` in a session triggers the hook and is blocked.
- [ ] Setting `CLAUDE_ALLOW_BROAD_TESTS=1` lets the same call through.
- [ ] Narrow commands like `cargo nextest run --lib tests::cli_tests` pass through.
- [ ] The hook is registered in `.claude/hooks/hooks.json`.
- [ ] Committed as: `feat(hooks): block broad test runs with PreToolUse guardrail`.

### Task C3: SessionStart reminder for test workflow

**Files:**
- Modify: existing session-start hook script in the plugin or in `.claude/hooks/` (discover via `grep -r SessionStart .claude hooks` inside the session)

**What to build:** Extend the existing SessionStart hook (if present) or add a minimal one that prints a one-liner reminder at session start: "Tests: prefer `cargo nextest run --lib <name>` or `cargo xtask test changed`. `dev` is a batch gate, not an edit-loop tool."

**Approach:**
- Locate the existing SessionStart entry points by grepping `hooks.json` for `SessionStart`.
- If one exists, add the test reminder to its output.
- If none exists, create a minimal `.claude/hooks/session-start-tests.cjs` that prints the reminder on `SessionStart`.
- Keep output to 2 lines maximum so it does not crowd the start of every session.

**Acceptance criteria:**
- [ ] A new session shows the test-workflow reminder.
- [ ] Reminder is at most 2 lines.
- [ ] Committed as: `feat(hooks): remind agents about narrow test workflow on session start`.

---

## Batch ordering and parallelization

- **Batch A** (parallel, respect A1 → A4 sequencing on `test_tiers.toml`): A1 first, then A2/A3/A5 can run in parallel with each other, then A4 after A1.
- **Batch B** (parallel except B1 depends on A1): B1 after A1; B2 and B3 are independent and can run in parallel with B1 and with each other.
- **Batch C** (parallel, then sequence C2 and C3 because both touch `.claude/hooks/hooks.json`): C1 anywhere; C2 before C3; all three can happen in parallel with Batch B.

The critical path is A1 → A4 → A5 (small) and A1 → B1. Everything else fans out.

## Closing

After all tasks land, the lead runs `cargo xtask test dev` once to validate the warm-cache sub-3-minute target and `cargo xtask test full` once to validate no regression. Report the before/after numbers in a final checkpoint.
