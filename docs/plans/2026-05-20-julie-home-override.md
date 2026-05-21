# JULIE_HOME Override Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Let operators move Julie daemon state and workspace indexes off `~/.julie` by setting `JULIE_HOME=/path/to/julie-home`.

**Architecture:** Keep `DaemonPaths` as the single source of truth for daemon storage *and* for the "is this `.julie` directory the global config dir?" check. `JULIE_HOME` should override the daemon home directory itself, not its parent, so indexes live at `$JULIE_HOME/indexes`. Most production adapter, daemon, CLI, dashboard, status, stop, and workspace registry paths already flow through `DaemonPaths::new()` or `DaemonPaths::try_new()`; the implementation should change the resolver and delete duplicate global-home detectors rather than wiring each call site.

**Tech Stack:** Rust, std env/path APIs, existing Julie daemon path abstractions, existing xtask verification tiers.

**Architecture Quality:** Low-risk central configuration change *only if* every `.julie` global-home resolver is deleted in favor of `DaemonPaths`. Two duplicate resolvers exist today and must both go:
1. `src/tools/workspace/paths.rs:1-7` — `julie_home()` helper plus `is_global_julie_dir()` comparison.
2. `src/workspace/mod.rs:375-396` — inline `HOME`/`USERPROFILE` lookup plus a near-identical canonicalize-and-compare block, *but* with an extra macOS case-insensitive comparison that the tools version is missing.

The case-insensitivity divergence between those two implementations is a latent bug today: one walker treats `/Users/foo/.julie` and `/users/foo/.julie` as the same dir, the other does not. Consolidating both call sites onto a new `DaemonPaths::is_julie_home(&Path)` method fixes this drift by construction.

---

## Current Verified State

- `src/paths.rs:12-22` defines `DaemonPaths::try_new()` and currently resolves `dirs::home_dir().join(".julie")`.
- `src/paths.rs:30-32` already has `DaemonPaths::with_home(...)`, but it is only wired for tests/internal harnesses.
- `src/main.rs:30-138` uses `DaemonPaths::new()` for `julie-server daemon`, `dashboard`, `stop`, `status`, `restart`, and adapter mode logging.
- `src/daemon/cli.rs:61-75` uses `DaemonPaths::new()` for the `julie-daemon` lifecycle binary.
- `src/adapter/mod.rs:35-39` uses `DaemonPaths::new()` for stdio adapter daemon launch/discovery.
- `src/bin/julie-adapter.rs` uses `DaemonPaths::new()` for adapter-side logging.
- `src/cli_tools/daemon.rs:54-70`, `165-182` use `DaemonPaths::new()` for CLI daemon discovery, readiness, and launch.
- `src/tools/workspace/paths.rs:1-7` has a separate `julie_home()` helper that hardcodes `$HOME/.julie`; `src/tools/workspace/paths.rs:69-76` has its own `is_global_julie_dir()` comparison. Both must be deleted, not wrapped.
- `src/workspace/mod.rs:375-396` has a *second* duplicate: inline `env::var("HOME").or_else(|_| env::var("USERPROFILE"))...join(".julie")` plus a canonicalize-and-compare block that includes macOS case-insensitive handling. Must also be deleted.
- `src/tests/daemon/paths.rs` contains existing path tests and is the right home for resolver tests.
- Existing env-test patterns to reuse: `src/tests/daemon/drain_timeout.rs:18-46` defines a clean `EnvGuard` (save/restore on drop) and `src/tests/cli_execution_tests.rs:441` shows `#[serial_test::serial(home_env)]` usage. New tests should use both: an `EnvGuard` for `JULIE_HOME` and `#[serial_test::serial(julie_home_env)]` to avoid racing the tempdir-based `JULIE_HOME` already set by `src/tests/harness/in_process.rs`.

## File Structure

- Modify `src/paths.rs`
  - Add production `JULIE_HOME` override handling in `DaemonPaths::try_new()`.
  - Add `is_julie_home(&Path) -> bool` method that canonicalizes both sides and applies macOS case-insensitive comparison (port the behavior from `src/workspace/mod.rs:384-395`).
  - Keep all derived paths unchanged.
  - Keep `with_home()` as the explicit test/internal constructor.

- Modify `src/tools/workspace/paths.rs`
  - **Delete** the local `julie_home()` helper at lines 1-7.
  - **Delete** `ManageWorkspaceTool::is_global_julie_dir()` at lines 69-76.
  - Update `find_workspace_root()` (line 79+) to call `DaemonPaths::try_new().ok().map(|p| p.is_julie_home(&julie_dir)).unwrap_or(false)` in place of the deleted helper.
  - No wrapper. The call site uses `DaemonPaths` directly.

- Modify `src/workspace/mod.rs`
  - **Delete** the inline `global_julie_home` lookup at lines 375-378.
  - **Delete** the canonicalize-and-compare block at lines 384-395.
  - Replace both with `DaemonPaths::try_new().ok().map(|p| p.is_julie_home(&julie_dir)).unwrap_or(false)`.
  - Net result: the macOS case-insensitive behavior, previously only present here, now lives on `DaemonPaths::is_julie_home()` and applies to both walkers.

- Modify `src/tests/daemon/paths.rs`
  - Add env-override tests for `DaemonPaths::try_new()`.
  - Add fallback/default behavior test when `JULIE_HOME` is absent.
  - Add empty-env validation test.
  - Add `is_julie_home()` tests: positive match, negative match, and (on macOS) case-insensitive match.

- Modify `src/tests/tools/workspace/...` only if needed
  - Prefer no new test here unless `changed` reveals a gap. The primary behavior is covered by `DaemonPaths`; the workspace call sites are now pure delegates.

- Modify documentation
  - `docs/WORKSPACE_ARCHITECTURE.md`: daemon storage table and operational notes should say `$JULIE_HOME` defaults to `~/.julie`.
  - `docs/OPERATIONS.md`: note how to move daemon state/indexes and the required stop/move/start sequence.
  - Any user-facing install/config docs found by search that state hardcoded `~/.julie`.

## Behavior Contract

- If `JULIE_HOME` is unset:
  - Behavior remains unchanged: daemon home is `dirs::home_dir()/.julie`.

- If `JULIE_HOME` is set to a non-empty path:
  - `DaemonPaths::try_new().julie_home()` returns that exact path as a `PathBuf`.
  - `indexes_dir()` is `$JULIE_HOME/indexes`.
  - `daemon.db`, pid/lock/port/token/discovery files, adapter/daemon logs, and migration state all live directly under `$JULIE_HOME`.
  - Do not append `.julie` to the override.
  - Do not canonicalize the override. The directory may not exist yet; `ensure_dirs()` creates it.
  - Use `std::env::var_os("JULIE_HOME")`, not `std::env::var`, so non-UTF-8 paths remain representable on Unix.

- If `JULIE_HOME` is set but empty:
  - `DaemonPaths::try_new()` returns `Err(std::io::ErrorKind::InvalidInput)` with a message naming `JULIE_HOME`.
  - Rationale: silently falling back would make an operator believe indexes moved when they did not.

- `DaemonPaths::is_julie_home(candidate: &Path) -> bool`:
  - Returns `true` when `candidate` (after best-effort `canonicalize()`) matches `self.julie_home()` (after best-effort `canonicalize()`).
  - On macOS (`#[cfg(target_os = "macos")]`), the comparison is case-insensitive on the path's string representation, matching the existing behavior at `src/workspace/mod.rs:384-395`. Other platforms remain case-sensitive.
  - Never panics; canonicalization failures fall back to the raw `PathBuf`.
  - Pure function over `self.julie_home`; no env reads, no I/O beyond `canonicalize()`.

- Scope explicitly excludes:
  - Per-workspace project logs under `<project>/.julie/logs`; those stay project-local via `DaemonPaths::project_log_dir(project_root)`.
  - External extract lock files and caller-owned DB paths.
  - `JULIE_WORKSPACE`, which remains workspace-root selection only.

## Verification Strategy

**Project source of truth:** `AGENTS.md` test workflow and commands.

**Worker red/green scope:** exact tests:
- `cargo nextest run --lib test_julie_home_env_override`
- `cargo nextest run --lib test_julie_home_env_empty_is_rejected`
- `cargo nextest run --lib test_julie_home_uses_home_dir`
- `cargo nextest run --lib test_is_julie_home_matches_canonicalized_path`
- `cargo nextest run --lib test_is_julie_home_rejects_unrelated_path`
- `cargo nextest run --lib test_is_julie_home_case_insensitive_on_macos` (gated `#[cfg(target_os = "macos")]`)

**Worker ceiling:** exact tests only during RED/GREEN. Do not run `cargo xtask test changed` or `cargo xtask test dev` from worker agents.

**Worker gate invariant:** exact tests prove `JULIE_HOME` override, empty override rejection, and default fallback behavior.

**Lead affected-change scope:** `cargo xtask test changed` after the coherent implementation/doc batch.

**Branch gate:** `cargo xtask test dev` before handoff/commit.

**Replay/metric evidence:** no replay metrics needed. This is path configuration behavior.

**Escalation triggers:**
- If daemon lifecycle, adapter launch, or workspace registry tests fail, add `cargo xtask test system`.
- If Windows-specific path handling is changed beyond `var_os`, add Windows-oriented unit coverage where possible and note that CI must confirm.

**Assigned verification failure:** Workers stop and report when assigned verification fails, unless this plan explicitly says to update that gate.

**Verification ledger:** Record invariant, command, scope label, commit SHA, result, and timestamp. Reuse evidence only for the same HEAD and same scope.

## Model Routing

**Project source of truth:** no repo-local `RAZORBACK.md` found during planning; use inherited harness defaults.

**Strategy tier:** planning, architecture, decomposition, lead review, finding triage
- Harness mapping: inherit

**Implementation tier:** bounded worker tasks from this plan
- Harness mapping: inherit

**Mechanical tier:** docs updates and rote call-site checks with no gate ownership
- Harness mapping: inherit

**Gate-interpretation reviewer:** read failing test output and diff when path/env behavior is ambiguous
- Harness mapping: inherit

**Escalation tier:** daemon lifecycle regressions, env-var race issues in tests, Windows path uncertainty
- Harness mapping: inherit

**Worker eligibility:** suitable for one implementation worker if subagents are available; write scope is `src/paths.rs`, `src/tools/workspace/paths.rs`, `src/workspace/mod.rs`, `src/tests/daemon/paths.rs`, and docs only.

**Escalation triggers:** any change requiring redesign of daemon lifecycle, adapter launch, or workspace registry storage layout.

**Mechanical exclusion:** docs-only workers cannot own test failures or decide path semantics.

**Unsupported harness behavior:** if per-agent routing is unavailable, inherit the current model and continue.

## Tasks

### Task 1: Add Failing Path Resolver Tests

**Files:**
- Modify: `src/tests/daemon/paths.rs`

**What to build:** Add focused tests that define the `JULIE_HOME` env contract AND the new `is_julie_home()` method before implementation.

**Approach:**
- Reuse the existing `EnvGuard` pattern from `src/tests/daemon/drain_timeout.rs:18-46` (save the previous `JULIE_HOME` value, set/remove inside the test, restore on drop) — do not invent a new pattern.
- Mark every env-mutating test with `#[serial_test::serial(julie_home_env)]`. This is mandatory: `src/tests/harness/in_process.rs` already sets `JULIE_HOME` to a tempdir for in-process daemon tests, and after Task 2 those tests will start *honoring* the env var. Parallel execution would race that state.
- Use `DaemonPaths::try_new()` for fallible cases; avoid `DaemonPaths::new()` in tests that expect error behavior.

**Test cases (env-gated, `serial(julie_home_env)`):**
- `test_julie_home_env_override`
  - Set `JULIE_HOME` to a temp directory path like `<tmp>/external-julie-home`.
  - Assert `DaemonPaths::try_new()?.julie_home()` is exactly that path.
  - Assert `indexes_dir()` is `<tmp>/external-julie-home/indexes`.
  - Assert `daemon_db()` is `<tmp>/external-julie-home/daemon.db`.

- `test_julie_home_env_empty_is_rejected`
  - Set `JULIE_HOME` to an empty value.
  - Assert `DaemonPaths::try_new()` returns `Err`.
  - Assert error kind is `InvalidInput`.
  - Assert error string mentions `JULIE_HOME`.

- Update `test_julie_home_uses_home_dir` to explicitly clear `JULIE_HOME` via `EnvGuard` and mark `serial(julie_home_env)`. Without this, the test will pass or fail based on whatever the surrounding test order leaves in the env.

**Test cases (no env mutation, no serial required):**
- `test_is_julie_home_matches_canonicalized_path`
  - Construct `DaemonPaths::with_home(<tmp>/home)` and create that directory.
  - Assert `paths.is_julie_home(&<tmp>/home)` returns `true`.

- `test_is_julie_home_rejects_unrelated_path`
  - Same setup.
  - Assert `paths.is_julie_home(&<tmp>/other)` returns `false`.

- `test_is_julie_home_case_insensitive_on_macos` (`#[cfg(target_os = "macos")]`)
  - Construct `DaemonPaths::with_home(<tmp>/Home)`, create the directory.
  - Assert `paths.is_julie_home(&<tmp>/home)` returns `true`.
  - This locks in the behavior currently only present in `src/workspace/mod.rs`.

**Acceptance criteria:**
- [ ] All env-override tests fail before implementation because `DaemonPaths::try_new()` still returns `~/.julie` and ignores env.
- [ ] `is_julie_home()` tests fail before implementation because the method does not exist (compile error is acceptable for RED).
- [ ] Existing default-home behavior remains covered.

### Task 2: Implement `JULIE_HOME` and `is_julie_home()` in `DaemonPaths`

**Files:**
- Modify: `src/paths.rs`

**What to build:**
1. Make `DaemonPaths::try_new()` check `JULIE_HOME` before falling back to `dirs::home_dir().join(".julie")`.
2. Add `DaemonPaths::is_julie_home(&Path) -> bool` so both workspace walkers can stop computing the global-home path on their own.

**Approach (env override):**
- Use `std::env::var_os("JULIE_HOME")`.
- If present and empty, return `std::io::ErrorKind::InvalidInput` with a message naming `JULIE_HOME`.
- If present and non-empty, return `DaemonPaths { julie_home: PathBuf::from(value) }`.
- If absent, keep the existing `dirs::home_dir()` fallback.
- Update the doc comments on `DaemonPaths` and `try_new()` to say default is `~/.julie`, overridden by `JULIE_HOME`.
- Do not canonicalize or create directories in `try_new()`.

**Approach (`is_julie_home`):**
- Take `&self` and `candidate: &Path`. Best-effort `canonicalize()` both `candidate` and `self.julie_home`, falling back to the raw `PathBuf` on error (matching the existing patterns at `src/tools/workspace/paths.rs:72-75` and `src/workspace/mod.rs:386-388`).
- On `#[cfg(target_os = "macos")]`, compare via `to_string_lossy().to_lowercase()` to preserve the case-insensitive behavior that currently lives only in `src/workspace/mod.rs`.
- On other platforms, compare the canonicalized `PathBuf`s directly.
- Never panics; no env reads inside the method (caller is expected to have constructed `DaemonPaths` via `try_new()` if env override matters).

**Acceptance criteria:**
- [ ] `test_julie_home_env_override` passes.
- [ ] `test_julie_home_env_empty_is_rejected` passes.
- [ ] `test_is_julie_home_matches_canonicalized_path` passes.
- [ ] `test_is_julie_home_rejects_unrelated_path` passes.
- [ ] `test_is_julie_home_case_insensitive_on_macos` passes (on macOS).
- [ ] Existing path derivation tests still pass.
- [ ] `cargo nextest run --lib test_julie_home_uses_home_dir` still passes with `JULIE_HOME` absent.

### Task 3: Delete Duplicate Global-Home Resolvers

**Files:**
- Modify: `src/tools/workspace/paths.rs`
- Modify: `src/workspace/mod.rs`

**What to build:** Both workspace walkers stop computing the global Julie home themselves and call `DaemonPaths` instead. One function returns the home path; one method answers the identity check; the call sites have neither.

**Approach for `src/tools/workspace/paths.rs`:**
- **Delete** the `julie_home()` helper at lines 1-7 entirely.
- **Delete** `ManageWorkspaceTool::is_global_julie_dir()` at lines 69-76 entirely.
- In `find_workspace_root()` (line 79+), replace:
  ```rust
  let global_julie_home = julie_home().ok();
  // ...
  if Self::is_global_julie_dir(&julie_dir, &global_julie_home) {
  ```
  with:
  ```rust
  let daemon_paths = crate::paths::DaemonPaths::try_new().ok();
  // ...
  if daemon_paths.as_ref().map_or(false, |p| p.is_julie_home(&julie_dir)) {
  ```
- Remove the now-unused `&Option<PathBuf>` parameter plumbing.

**Approach for `src/workspace/mod.rs`:**
- **Delete** the inline `global_julie_home` block at lines 375-378 entirely.
- **Delete** the canonicalize-and-compare block at lines 384-395 entirely (including the macOS case-insensitive branch — that behavior now lives on `DaemonPaths::is_julie_home`).
- Replace with a single `DaemonPaths::try_new().ok()` at the top of the function, then `daemon_paths.as_ref().map_or(false, |p| p.is_julie_home(&julie_dir))` at the comparison site.
- Preserve the surrounding loop, debug logging, and walk-up termination unchanged.

**Acceptance criteria:**
- [ ] No `julie_home()` function in `src/tools/workspace/paths.rs`.
- [ ] No `is_global_julie_dir()` function in `src/tools/workspace/paths.rs`.
- [ ] No `env::var("HOME")` or `env::var("USERPROFILE")` inside `src/workspace/mod.rs::find_workspace_root`.
- [ ] No `canonicalize()` / case-insensitive path comparison inline in `src/workspace/mod.rs::find_workspace_root` (it's all behind `is_julie_home`).
- [ ] Both call sites resolve global home via `DaemonPaths::try_new()`, so `$JULIE_HOME` override is honored by both walkers automatically.
- [ ] Existing workspace-root-detection tests still pass (`cargo nextest run --lib find_workspace_root` and `cargo nextest run --lib is_global_julie_dir` are starting points; the latter test name disappears, but its behavioral coverage must remain via `is_julie_home` tests + an integration check that the walker skips the global dir).
- [ ] No behavior change for unset `JULIE_HOME`.

### Task 4: Audit for Remaining Global-Home Resolvers

**Files:**
- Inspect only unless tests reveal drift:
  - `src/main.rs:30-138`
  - `src/daemon/cli.rs:61-75`
  - `src/adapter/mod.rs:35-39`
  - `src/bin/julie-adapter.rs`
  - `src/cli_tools/daemon.rs:54-70`, `165-182`

**What to build:** Confirm no additional production code computes the global `~/.julie` path on its own. (Task 3 covered the two known duplicates; this task catches anything missed.)

**Approach:**
- Run a narrow audit focused on *global-home* resolvers, not project-local `.julie` paths:
  ```
  rg -n 'env::var.*"HOME"|env::var.*"USERPROFILE"|env::var_os.*"HOME"|env::var_os.*"USERPROFILE"|dirs::home_dir' src
  ```
- Skim each hit and classify:
  - **Allowed:** consumers that need `$HOME` for non-Julie reasons (e.g. `src/embeddings/sidecar_supervisor.rs` for `XDG_CACHE_HOME` fallback, `xtask/` tooling outside production scope, test harnesses).
  - **Must delegate:** any production code that joins `HOME`/`USERPROFILE`/`home_dir()` with `.julie`. Must call `DaemonPaths::try_new()` instead.
- Do *not* use `rg '\.join\("\.julie"\)'` as the audit — it produces 25+ legitimate project-local hits (`handler.rs`, `migration.rs`, `daemon/workspace_pool.rs`, `daemon/project_log.rs`, etc.) that are intentionally per-workspace and out of scope.

**Acceptance criteria:**
- [ ] Adapter, daemon, CLI daemon client, status/stop/dashboard all flow through `DaemonPaths`.
- [ ] No production code joins `HOME`/`USERPROFILE`/`home_dir()` with `.julie` outside `DaemonPaths::try_new()`.
- [ ] Project-local `.julie/logs` and per-workspace `.julie` paths are unchanged.
- [ ] Any non-Julie `$HOME` consumer found during the audit is documented in the verification ledger as "intentional, out of scope".

### Task 5: Document Operator Usage

**Files:**
- Modify: `docs/WORKSPACE_ARCHITECTURE.md`
- Modify: `docs/OPERATIONS.md`
- Modify other docs only if `rg "~/.julie|\\.julie/indexes|JULIE_HOME" docs` finds user-facing hardcoded storage statements that would mislead operators.

**What to build:** Document the override and migration workflow.

**Approach:**
- In architecture docs, replace daemon storage wording from `~/.julie/indexes/<workspace_id>` to `$JULIE_HOME/indexes/<workspace_id>` with default `$JULIE_HOME=~/.julie`.
- In operations docs, add a short section:
  - Stop daemon.
  - Move existing `~/.julie` to the target drive/path.
  - Set `JULIE_HOME` in the MCP client environment and any shell used for `julie-daemon`, `julie-server`, or CLI tool calls.
  - Start/restart Julie.
  - Verify with `julie-server status` or by checking `$JULIE_HOME/daemon.db` and `$JULIE_HOME/indexes/`.
- Mention that changing `JULIE_HOME` creates an independent daemon identity and index registry. Existing `~/.julie` data is not auto-migrated unless the operator moves it.

**Acceptance criteria:**
- [ ] Docs state the default and override path clearly.
- [ ] Docs do not imply `JULIE_WORKSPACE` moves indexes.
- [ ] Docs explain that all Julie processes must see the same `JULIE_HOME`.

### Task 6: Verification and Commit

**Files:**
- All touched files.

**What to build:** Final verification and source-control checkpoint.

**Approach:**
- Run narrow tests first:
  - `cargo nextest run --lib test_julie_home_env_override`
  - `cargo nextest run --lib test_julie_home_env_empty_is_rejected`
  - `cargo nextest run --lib test_julie_home_uses_home_dir`
- Run `cargo check`.
- Run `cargo xtask test changed`.
- Run `cargo xtask test dev`.
- If daemon lifecycle or adapter buckets fail or were heavily touched, add `cargo xtask test system`.
- Run `cargo fmt --check` and `git diff --check`.
- Save a Goldfish checkpoint before committing.
- Commit with `fix(daemon): honor JULIE_HOME for daemon storage` or `feat(daemon): support JULIE_HOME storage override`.

**Acceptance criteria:**
- [ ] Exact tests pass.
- [ ] `cargo check` passes.
- [ ] `cargo xtask test changed` passes.
- [ ] `cargo xtask test dev` passes.
- [ ] Worktree is clean after commit.

