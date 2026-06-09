# Daemon Orphaned Complexity Cleanup Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Remove dead daemon-complexity code left after Phase 3d.2 (HTTP daemon teardown). Clean up orphaned path methods, unused dashboard fields, dead module names, and stranded test files.

**Architecture:** Straightforward deletion and renaming. Rename `pub struct DaemonPaths` → `pub struct RegistryPaths`. Rename `mod daemon` → `mod registry`. No behavior changes.

**Tech Stack:** Rust

**No Architecture Impact**

---

## Validation Notes (2026-06-08)

**Status:** Cleanup target is valid, but this plan is not implementation-ready as written.

Validated against the current tree with Julie/Miller symbol search, exact `rg`
checks, `cargo build --bins`, and `cargo xtask test list`.

**Confirmed valid findings:**
- `DaemonPaths` is still the public path container name; a rename to
  `RegistryPaths` is mechanically valid but touches more than the file impact
  map lists (29 live code/test files in the current tree).
- `src/daemon/` and `src/tests/daemon/` are still live module names, and
  `src/lib.rs` / `src/tests/mod.rs` still declare `daemon`.
- The legacy path helpers `daemon_pid`, `daemon_lock`, `daemon_log`,
  `daemon_port`, `daemon_mcp_transport`, `daemon_mcp_token`,
  `migration_state`, and `daemon_shutdown_event` have no production call sites
  found in the current tree. Their remaining direct calls are test-only.
- `daemon_startup_lock`, `daemon_singleton_lock`, `discovery_file`,
  `token_file`, and the path-level `daemon_state` helper have no direct call
  sites found in the current tree.
- `workspace_pool_connected` is hard-coded to `false` in
  `DashboardState::health_snapshot()`.
- `src/tests/daemon/shutdown_event.rs` is a stale Windows-only test surface:
  it imports `crate::daemon::shutdown_event`, while
  `src/daemon/shutdown_event.rs` is already deleted.
- The xtask `daemon` bucket still exists (`cargo xtask test list` reports it).

**Corrections required before implementation (Codex review):**
- Do not delete `julie_home_hash()` as written. It is still used by the
  Windows `embedding_host_pipe_name()` helper. If `daemon_shutdown_event()` is
  removed, keep or rename the hash helper for embedding-host pipe naming.
- Do not delete `DashboardState::daemon_db` / `daemon_db()` as dead state.
  Current dashboard routes still read it in:
  `routes/search.rs`, `routes/projects.rs`, `routes/search_session.rs`,
  `routes/metrics.rs`, `routes/search_analysis.rs`, and
  `routes/intelligence.rs`, plus metrics dashboard tests.
- Do not describe `daemon_db_connected` as always false. It is computed from
  whether the registry DB is attached and whether `list_workspaces()` succeeds,
  and it drives both control-plane and data-plane readiness.
- Do not delete `daemon_phase` from `DashboardState`. It is read by
  `daemon_phase_kind()` and `health_snapshot()`.
- Do not delete `JulieServerHandler.daemon_db` as an orphaned field. It is
  still used by workspace resolution, workspace index/register/refresh code,
  embedding finalization/sync, tool context, metrics, and dashboard wiring.
- `JulieServerHandler.restart_pending` has only one non-test live read
  (`src/health/checker.rs`), but it is still threaded through multiple
  constructors and many tests. Removing it is plausible but must update the
  health checker, constructor signatures, dashboard/status surfaces, templates,
  and tests together.
- The xtask rename scope is broader than listed. In addition to
  `xtask/test_tiers.toml` and `xtask/src/changed.rs`, update xtask tests and
  contract fixtures such as `xtask/tests/changed_tests.rs`,
  `xtask/tests/support/manifest_contract_expected.rs`, and
  `xtask/tests/search_matrix_contract_tests.rs`.

**Baseline verification run during validation:**
- `cargo build --bins` passed.
- `cargo xtask test list` confirmed the current `daemon` bucket is still in
  `dev`, `full`, and `reliability`.

---

## Verification Strategy

**Project source of truth:** `CLAUDE.md` — xtask test tiers with canonical commands.

**Worker red/green scope:** `cargo nextest run --lib <test_name>` per changed surface (one test per task). Workers run their narrow test only.

**Worker ceiling:** One narrow test per task.

**Branch gate:** `cargo xtask test dev` — fast regression tier before handoff.

**Escalation triggers:** If `cargo build` fails across the workspace, escalate immediately.

---

## File Impact Map

| File | Action |
|------|--------|
| `crates/julie-core/src/paths.rs` | Delete orphaned path methods (keep `julie_home_hash`), rename struct |
| `crates/julie-core/src/tests/paths.rs` | Delete/update tests for deleted path helpers |
| `src/health/checker.rs` | Delete `restart_pending` reads |
| `src/dashboard/routes/status.rs` | Delete `restart_pending` reads |
| `src/daemon/` | Rename directory → `src/registry/` |
| `src/daemon/mod.rs` | Rename → `src/registry/mod.rs`, update doc comment |
| `src/tests/daemon/` | Rename directory → `src/tests/registry/`, update mod.rs |
| `src/tests/registry/paths.rs` | Update `DaemonPaths` → `RegistryPaths`, remove deleted helper tests |
| `src/tests/integration/in_process_boundary.rs` | Update `src/daemon/` path literals → `src/registry/` |
| `crates/julie-index/tests/no_upward_deps.rs` | Update `crate::daemon` guards → `crate::registry` |
| `crates/julie-core/tests/no_upward_deps.rs` | Update `crate::daemon` guards → `crate::registry` |
| `crates/julie-pipeline/tests/no_upward_deps.rs` | Update `crate::daemon` guards → `crate::registry` |
| `crates/julie-runtime/tests/no_upward_deps.rs` | Update `crate::daemon` guards → `crate::registry` |
| `crates/julie-context/tests/no_upward_deps.rs` | Update `crate::daemon` guards → `crate::registry` |
| `crates/julie-tools/tests/no_upward_deps.rs` | Update `crate::daemon` guards → `crate::registry` |
| `xtask/test_tiers.toml` | Update `daemon` bucket references |
| `xtask/tests/changed_tests.rs` | Update `daemon` → `registry` references |
| `xtask/tests/support/manifest_contract_expected.rs` | Update `daemon` → `registry` references |
| `xtask/tests/search_matrix_contract_tests.rs` | Update `daemon` → `registry` references |
| All callers of `crate::daemon::` | Update to `crate::registry::` |
| All callers of `DaemonPaths` | Update to `RegistryPaths` |

**NOT changing (Codex correction — these are still live):**
- `DashboardState::daemon_db` / `daemon_db()` — still read by dashboard routes
- `DashboardState::daemon_phase` — still read by `daemon_phase_kind()` and `health_snapshot()`
- `JulieServerHandler::daemon_db` — still used by workspace resolution, indexing, embedding, metrics
- `daemon_db_connected` — computed dynamically, not always false
- `workspace_pool_connected` — hard-coded false in `health_snapshot()`, but removing requires verifying all callers
- `julie_home_hash()` — still used by `embedding_host_pipe_name()`

---

## Task 1: Delete Orphaned DaemonPaths Methods

**Files:**
- Modify: `crates/julie-core/src/paths.rs`
- Modify: `crates/julie-core/src/tests/paths.rs` — remove test calls to deleted helpers

**What to build:** Delete the following methods from `DaemonPaths` (all have zero live callers; confirmed by `fast_refs`):

```
daemon_pid()             — orphaned (no callers)
daemon_lock()            — orphaned (no callers)
daemon_startup_lock()    — orphaned (no callers)
daemon_singleton_lock() — orphaned (no callers)
daemon_log()             — orphaned (no callers)
daemon_port()            — orphaned (no callers)
daemon_mcp_transport()  — orphaned (no callers)
daemon_mcp_token()      — orphaned (no callers)
discovery_file()          — orphaned (no callers)
token_file()            — orphaned (no callers)
daemon_state()           — orphaned (no callers)
daemon_shutdown_event()  — orphaned (no callers); keep julie_home_hash()
migration_state()        — orphaned (no callers)
daemon_db()              — deprecated shim (lines 465-468)
```

**Approach:** Delete the orphaned methods. **KEEP `julie_home_hash()`** — it is still used by `embedding_host_pipe_name()`. Do NOT delete `registry_db()`, `embedding_host_socket()`, `embedding_host_lock()`, `embedding_host_pipe_name()`, `project_log_dir()`, `ensure_dirs()`, or the VCS root markers.

**Acceptance criteria:**
- [ ] Deleted methods confirmed gone: `cargo build --bins` succeeds
- [ ] `embedding_host_pipe_name()` still works (uses `julie_home_hash()`)

---

## Task 2: Rename DaemonPaths → RegistryPaths

**Files:**
- Modify: `crates/julie-core/src/paths.rs:243` (struct definition)
- Modify: ALL callers of `DaemonPaths` across the workspace

**What to build:** Rename the struct and update every call site.

**Approach:** Use Julie's `rename_symbol(dry_run=true)` to preview, then `dry_run=false` to apply. Alternatively, use `Edit` with `replace_all=true`. Update the doc comment on the struct from "Daemon paths" to "Registry and runtime paths".

**Acceptance criteria:**
- [ ] `cargo build --bins` succeeds
- [ ] All `DaemonPaths` references updated to `RegistryPaths`

---

## Task 3: Delete Orphaned `restart_pending` from Handler and Health

**Files:**
- Modify: `src/handler.rs` — remove `restart_pending` field and constructor params
- Modify: `src/health/checker.rs` — remove `restart_pending` reads
- Modify: `src/dashboard/routes/status.rs` — remove `restart_pending` reads
- Modify: `src/dashboard/state.rs` — remove `restart_pending` field and accessor
- Modify: All test files that construct handler/state with this field

**What to build:** `JulieServerHandler.restart_pending` has only one non-test live read (`src/health/checker.rs`). It is threaded through constructors and many tests. Removing it requires updating the health checker, constructor signatures, dashboard/status surfaces, and tests together.

`DashboardState.restart_pending` is written by handlers but only read by `is_restart_pending()` which feeds the health snapshot.

**Approach:**
1. `fast_refs` on `restart_pending` to confirm the caller set
2. Remove the field from `JulieServerHandler` and all its constructors
3. Remove the read from `health/checker.rs`
4. Remove `restart_pending` from `DashboardState` and its `is_restart_pending()` accessor
5. Update dashboard routes that call `is_restart_pending()`
6. Update all test files that construct handler/state with this field

**Acceptance criteria:**
- [ ] `cargo build --bins` succeeds
- [ ] Health checker no longer reads `restart_pending`
- [ ] `cargo nextest run --lib handler_construction_uses_startup_hint_for_current_root` passes

---

## Task 4: Rename `mod daemon` → `mod registry`

**Files:**
- Rename: `src/daemon/` → `src/registry/`
- Modify: `src/lib.rs` — update `pub mod daemon` → `pub mod registry`
- Modify: All imports of `crate::daemon::` → `crate::registry::` across the workspace

**What to build:** Rename the module directory and update all import paths. This includes test files and xtask crates that reference `crate::daemon::`.

**Approach:**
1. `mv src/daemon src/registry`
2. `mv src/tests/daemon src/tests/registry`
3. Update `src/lib.rs` and `src/tests/mod.rs` mod declarations
4. Use `Edit` with `replace_all=true` on `use crate::daemon::` → `use crate::registry::`
5. Use `Edit` with `replace_all=true` on `pub mod daemon` → `pub mod registry`
6. Update `src/registry/mod.rs` doc comment from "Julie daemon" to "Julie registry"

**Acceptance criteria:**
- [ ] `cargo build --bins` succeeds
- [ ] All `crate::daemon::` references updated to `crate::registry::`

---

## Task 5: Delete Orphaned Test Files and Update Path Tests

**Files to delete:**
- `src/tests/registry/shutdown_event.rs` — `#[cfg(windows)]`, tests `daemon_shutdown_event` which is deleted in Task 1

**Files to update:**
- `src/tests/registry/paths.rs` — update `DaemonPaths` → `RegistryPaths`, remove deleted helper test functions
- `crates/julie-core/src/tests/paths.rs` — remove tests calling deleted helpers
- `src/tests/integration/in_process_boundary.rs` — update `src/daemon/` path literals to `src/registry/`

**What to build:** The orphaned `shutdown_event.rs` test file imports `crate::daemon::shutdown_event`, but `src/daemon/shutdown_event.rs` was deleted in Phase 3d.2. The core paths test file also has tests for deleted helpers that must be removed. The integration boundary tripwire reads `src/daemon` path strings that must be updated.

**Approach:**
1. Confirm `src/daemon/shutdown_event.rs` (the production file) does not exist
2. Confirm `src/tests/registry/shutdown_event.rs` still imports it
3. Delete the test file
4. Update `src/tests/registry/paths.rs` to use `RegistryPaths` and remove deleted helper tests
5. Update `crates/julie-core/src/tests/paths.rs` to remove tests for deleted helpers
6. Update `src/tests/integration/in_process_boundary.rs` path literals from `src/daemon/` to `src/registry/`

**Acceptance criteria:**
- [ ] `cargo build --bins` succeeds
- [ ] `cargo nextest run --lib test_daemon_lock_file_persists_after_release` (renamed) passes

---

## Task 6: Update xtask Test Tiers and Fixtures

**Files:**
- Modify: `xtask/test_tiers.toml` — update `daemon` bucket → `registry`
- Modify: `xtask/src/changed.rs` — update any `daemon` → `registry` path mappings
- Modify: `xtask/tests/changed_tests.rs` — update `tests::daemon::` → `tests::registry::`
- Modify: `xtask/tests/support/manifest_contract_expected.rs` — update `daemon` → `registry` references
- Modify: `xtask/tests/search_matrix_contract_tests.rs` — update `crate::daemon::` → `crate::registry::`

**What to build:** Reflect the `daemon` → `registry` module rename in xtask configuration and test fixtures.

**Approach:** `grep` for `daemon` in `xtask/` and update references. Update the `daemon` test bucket name to `registry` and update all path references to `tests::daemon::` → `tests::registry::`.

**Acceptance criteria:**
- [ ] `cargo xtask test list` shows `registry` bucket instead of `daemon`
- [ ] `cargo xtask test dev` passes

---

## Task 7: Update no_upward_deps Tripwires

**Files:**
- `crates/julie-index/tests/no_upward_deps.rs`
- `crates/julie-core/tests/no_upward_deps.rs`
- `crates/julie-pipeline/tests/no_upward_deps.rs`
- `crates/julie-runtime/tests/no_upward_deps.rs`
- `crates/julie-context/tests/no_upward_deps.rs`
- `crates/julie-tools/tests/no_upward_deps.rs`

**What to build:** The `no_upward_deps` tests in each split crate use string tripwires to verify that `crate::daemon` is not referenced from that crate. After the rename, these tripwires must be updated to `crate::registry` so the guards continue to work.

**Approach:** In each `no_upward_deps.rs` file, update the string literals that match `crate::daemon` to match `crate::registry` instead. Verify the guards still function by running `cargo nextest run -p <crate> --lib no_upward_deps`.

**Acceptance criteria:**
- [ ] All `no_upward_deps` tests pass with updated guards
- [ ] `cargo build --bins` succeeds

---

## Verification

After all tasks complete:

```bash
cargo build --bins 2>&1 | tail -5
cargo xtask test dev  # branch gate
```

Expected: build succeeds, `dev` tier green.

---

## Rollback Plan

If any task breaks the build:
1. `git stash` uncommitted changes
2. Identify the breaking task
3. Revert that task's changes
4. `git stash pop`
5. Investigate and fix
