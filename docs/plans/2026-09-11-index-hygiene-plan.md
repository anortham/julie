# Index and Registry Hygiene Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development whenever delegation is available and permitted, including for one task; serialize dependent tasks. Use razorback:executing-plans only when delegation is unavailable or the user/session explicitly selected single-agent execution.

**Goal:** Stop the service from deleting live indexes on transient failures and restarts, stop tests from touching the developer's live Julie home, give the service a real log file, and prune dead registry rows at service start.

**Architecture:** One classified decision replaces four delete layers: a store is deleted only when `CheckoutStore::open` returns the typed `VersionMismatch`. `/status` reads through the non-destructive opener. Restart waits on the record's pid; only the owning pid removes the record. The service log lands under `$JULIE_HOME/logs/`. Tests get `JULIE_HOME` from `.cargo/config.toml`, and a registry store derives its indexes directory from the database it holds, so a temp registry can never sweep a foreign indexes tree. Service start spawns the existing cleanup sweep once.

**Tech Stack:** Rust (`julie`, `julie-runtime`, `julie-index`, `julie-core`), cargo config.

**Architecture Quality:** Approved shape in `docs/plans/2026-09-11-index-hygiene-design.md`. Net lines negative or flat: three delete layers removed. Risk: a `VersionMismatch` that does not downcast (wrapped twice) would stop deleting on real version bumps; Task 1's mismatch test guards that.

## Global Constraints

- Design: `docs/plans/2026-09-11-index-hygiene-design.md`. Evidence: checkpoint `6bbb54be` and the two investigator reports summarized in the design.
- Design rule: no lock file, no lease, no pid file beyond the `pid` already in `service.json`.
- Delete an index directory in exactly one place on the open path: the two `open_or_recreate` functions, and only on `VersionMismatch`. Explicit `force` reindex, `rebuild`, workspace `remove`, and the cleanup sweep keep their deletes.
- `.cargo/config.toml` `[env]` gains `JULIE_HOME = { value = "target/test-julie-home", relative = true }`. No `force`.
- Log file name: `$JULIE_HOME/logs/julie-service.log.<YYYY-MM-DD>` using the existing `ProjectLog` rotation.
- Commit messages: conventional commits, one commit per task, on branch `index-hygiene` in `/home/murphy/source/julie/.worktrees/index-hygiene`.
- No pushes, no releases, no edits outside this worktree.
- Test rules from `CLAUDE.md`: workers run exact tests only, at most two runs per change. The lead runs `cargo xtask test changed` and the tiers.
- Tests get zero comments. No narration comments in code.

---

## Verification Strategy

**Project source of truth:** `CLAUDE.md` sections "RUNNING TESTS" and "Canonical Test Tiers"; `xtask/test_tiers.toml`.

**Worker red/green scope:** `cargo nextest run --lib <exact_test_name>`.

**Worker ceiling:** the exact test names written in each task. Workers never run `cargo xtask test …` or an unfiltered `cargo nextest run`.

**Worker gate invariant:** per task, listed under acceptance criteria.

**Lead affected-change scope:** `cargo xtask test changed` after each task; `cargo xtask test bucket service` after Tasks 3, 4, 6; `cargo xtask test bucket tools-workspace-targeting` after Task 5.

**Branch gate:** `cargo fmt --check`, `cargo clippy --workspace --all-targets`, `cargo xtask test system`, `cargo xtask test dev`, `NEXTEST_TEST_THREADS=6 cargo xtask test full`. Then the live check in Task 7.

**Security scope:** none declared.

**Replay/metric evidence:** hard gates: every test named in this plan; `dev`, `system`, `full` pass; after `dev` and `full`, `~/.julie/registry.db` row count unchanged and no file under `~/.julie/indexes/` has a new birth time; two consecutive `service restart` calls on the ten open corpus repos change no `facts.sqlite` birth time and drop no vector count. Report-only: sweep counts logged at start, RSS after restart.

**Escalation triggers:** any change to `src/service/` or `src/request_engine/` runs `cargo xtask test system`.

**Assigned verification failure:** Workers stop and report when assigned verification fails, unless this plan explicitly says to update that gate.

**Verification ledger:** `docs/plans/2026-09-11-index-hygiene-ledger.md` using `docs/plans/verification-ledger-template.md`.

## Parallel Execution Contract

All tasks run serially in this worktree; several touch `src/service/mod.rs` or `src/request_engine/runtime_factory.rs`, and serial keeps the git index simple.

| Task | Parallel batch | File ownership | Serialization required | Dependency reason |
|---|---|---|---|---|
| Task 1: Delete only on version mismatch | None - serial | `src/tools/workspace/indexing/store_open.rs`, `crates/julie-runtime/src/workspace/mod.rs`, `src/startup_repair_plan.rs`, `src/request_engine/runtime_factory.rs`, new `src/tests/tools/workspace/store_open.rs`, `src/tests/tools/workspace/mod.rs` | Yes | First; others assume the single delete site. |
| Task 2: Status reads through the non-destructive opener | None - serial | `src/tools/workspace/commands/registry/status.rs`, new test in `src/tests/tools/workspace/store_open.rs` | Yes | Uses Task 1's test fixture. |
| Task 3: Restart waits on the pid; only the owner removes the record | None - serial | `src/main.rs`, `src/service/client.rs`, `src/service/mod.rs`, `src/service/discovery.rs`, `src/tests/service/client.rs`, `src/tests/service/control.rs` | Yes | Touches `service/mod.rs`, shared with Task 6. |
| Task 4: Service log under the Julie home | None - serial | `crates/julie-core/src/paths.rs`, `src/registry/project_log.rs`, `src/request_engine/runtime_factory.rs`, `src/service/http.rs`, `src/tests/service/http_api.rs` | Yes | Touches `runtime_factory.rs`, shared with Task 1. |
| Task 5: Hermetic tests and database-derived indexes dir | None - serial | `.cargo/config.toml`, `src/registry/database.rs`, `src/tools/workspace/commands/registry/mod.rs`, `src/tests/registry/database.rs`, `src/tests/tools/workspace/global_targeting/list.rs` | Yes | Task 6 uses the fixed store helper. |
| Task 6: Cleanup sweep at service start | None - serial | `src/service/mod.rs`, `src/tests/service/http_api.rs` | Yes | Needs Tasks 3 and 5. |
| Task 7: Gates, live restart check, finding, ledger | None - serial | `docs/findings/2026-09-1X-index-hygiene.md`, `docs/plans/2026-09-11-index-hygiene-ledger.md` | Yes | Lead-run. |

Commit mode for every task: `serial-worker-commit`.

---

## Task 1: Delete only on version mismatch

**Files:**
- Modify: `src/tools/workspace/indexing/store_open.rs:19-33` (`open_or_recreate`), `:56-92` (`store_for_workspace`, drop the second delete branch at 80-91); `crates/julie-runtime/src/workspace/mod.rs:101-116` (`open_or_recreate_store`); `src/startup_repair_plan.rs:47-67` (drop both `delete_store_dir` calls; a failed open or facts read returns the error, a version mismatch is already caught at `:41` by `store_engine_mismatch`); `src/request_engine/runtime_factory.rs:283-300` (`initialize_recovering_store` becomes: call `initialize_workspace_with_force(None, false)` once, map the error, and if the workspace is still `None` return `RequestFailure::internal` without deleting).
- Create: `src/tests/tools/workspace/store_open.rs`; register in `src/tests/tools/workspace/mod.rs`.

**Interfaces:**
- Consumes: `VersionMismatch` (`crates/julie-index/src/checkout_store/mod.rs:36-41`), boxed by `open_facts` at `:84-89`; `CheckoutStore::open` (`:96-104`); `FactsStore::open` and `Opened::VersionMismatch` in `crates/julie-facts/src/store.rs`.
- Produces: `open_or_recreate` and `open_or_recreate_store` return `Err` for every non-mismatch failure with the directory intact.

**Contract inputs:** design "Decision" item 1.
**File ownership:** as listed.
**Serialization required:** Yes.
**Dependency reason:** First task.

**What to build:** In both openers, match `Err(err) if err.downcast_ref::<VersionMismatch>().is_some()` (confirm with `deep_dive VersionMismatch` that the type is public to `julie-runtime`; if not, expose it from `julie_index::checkout_store`) to delete and reopen; every other `Err` propagates with context. Remove the three duplicate delete layers. Keep `warn!` on the mismatch path with the store dir.

**Tests** (`src/tests/tools/workspace/store_open.rs`):
- `locked_store_is_not_deleted_on_open_failure`: build a real store in a temp dir with one Rust file (model on `src/tests/service/durable_roots.rs:30-40`), then hold an exclusive lock: open a second `rusqlite::Connection` to `facts.sqlite` and run `BEGIN EXCLUSIVE`. Call `open_or_recreate(store_dir, root)` on a `spawn_blocking` thread. Assert `Err`, `facts.sqlite` still exists, and its `count(*) from symbols` is unchanged after releasing the lock. If the 5 s busy timeout makes the test slow, set the lock, call, and assert within one test; do not lower the timeout.
- `version_mismatch_store_is_deleted_and_rebuilt`: build a store, then `UPDATE meta SET value='stale' WHERE key='engine_version'` through rusqlite. Call `open_or_recreate`. Assert `Ok`, and `facts.sqlite` has a new birth time or zero symbols.
- `initialize_recovering_store_returns_error_without_deleting`: a handler bound to a temp index root whose `facts.sqlite` is locked exclusively; assert the initialize error and the file's survival. If the private function cannot be reached from `src/tests`, expose it `pub(crate)`.

**Acceptance criteria:**
- [x] Three tests pass by exact name.
- [x] `grep -n delete_store_dir src crates` shows only `store_open.rs` (the force, rebuild and sweep paths call `remove_dir_all` directly and are untouched).
- [x] `cargo check --workspace --all-targets` passes.
- [x] Commit: `fix(index): delete a checkout store only on a version mismatch` (bca54911).

---

## Task 2: Status reads through the non-destructive opener

**Files:**
- Modify: `src/tools/workspace/commands/registry/status.rs:111` (call `handler.checkout_store_for_workspace(&workspace_id, &root)`; the `Err(_) => (None, 0, None)` arm at `:122` stays), drop the now-unused `store_for_workspace` import at `:11`.
- Test: `src/tests/tools/workspace/store_open.rs` (extend).

**Interfaces:**
- Consumes: `JulieServerHandler::checkout_store_for_workspace` (`src/handler.rs:1366`).

**Contract inputs:** design "Decision" item 2.
**File ownership:** as listed.
**Serialization required:** Yes.
**Dependency reason:** Uses Task 1's fixture.

**What to build:** One call-site swap. Then one test: `status_over_a_locked_checkout_reports_not_loaded_and_deletes_nothing`: a handler with a temp registry (model on `src/tests/tools/workspace/global_targeting/list.rs:39` for the temp `daemon.db` setup) holding one row for a temp workspace whose `facts.sqlite` is locked exclusively; run `manage_workspace` `status` with no `workspace_id`; assert the row's status has `store_status: None` (or the field the `CheckoutStatus` struct uses; read it with `get_symbols CheckoutStatus`), and `facts.sqlite` survives.

**Acceptance criteria:**
- [x] Test passes by exact name.
- [x] `grep -n '\bstore_for_workspace' src/tools/workspace/commands/registry/status.rs` is empty (the bare grep matches `checkout_store_for_workspace`).
- [x] Commit: `fix(status): read checkouts through the non-destructive opener` (27bf69ee).

---

## Task 3: Restart waits on the pid; only the owner removes the record

**Files:**
- Modify: `src/service/discovery.rs` (add `pub fn pid_alive(pid: u32) -> bool`; on unix `libc::kill(pid, 0) == 0` or `Path::new("/proc/<pid>").exists()` when `cfg!(target_os = "linux")`, and `sysinfo`-free fallback `true` on other platforms only if no crate is already available; check `Cargo.toml` for `libc` or `nix` first and reuse), `src/main.rs:111-128` (restart: after `post_shutdown`, poll `pid_alive(record.pid)` every 100 ms up to 30 s; then `connect_or_start`), `src/service/client.rs:100-103` (`try_connect`: on `status()` failure, remove the record only when `!pid_alive(record.pid)`; when alive, return `Err(ConnectError::Unavailable("service <pid> is shutting down"))`), `src/service/mod.rs:114` (remove the record only when `read_record` returns a record whose `pid == std::process::id()`).
- Test: `src/tests/service/client.rs`, `src/tests/service/control.rs`.

**Interfaces:**
- Consumes: `ServiceRecord.pid` (`src/service/discovery.rs:10`), `read_record`, `remove_record`, `write_record`.
- Produces: `discovery::pid_alive`.

**Contract inputs:** design "Decision" item 3.
**File ownership:** as listed.
**Serialization required:** Yes.
**Dependency reason:** `service/mod.rs` shared with Task 6.

**Tests:**
- `try_connect_keeps_the_record_while_its_pid_is_alive` (`src/tests/service/client.rs`): write a record with `pid = std::process::id()` and a port nothing listens on; call `connect_or_start` with a spawn closure that panics if called; assert `Err`, record still on disk.
- `try_connect_removes_the_record_when_its_pid_is_dead`: same with a pid that cannot exist (spawn `true` via `std::process::Command`, wait, use its pid); the spawn closure runs; record gone. Keep the existing `stale_service_json_is_replaced_by_a_fresh_service` in `process.rs` green.
- `shutdown_removes_only_a_record_it_owns` (`src/tests/service/control.rs`, model on `:6`): start the in-process service, overwrite the record with a different pid, shut down, assert the foreign record survives.

**Acceptance criteria:**
- [x] Three tests pass by exact name; `service` and `service-process` buckets pass.
- [x] No new dependency (`libc` and `windows-sys` were already present).
- [x] Commit: `fix(service): restart waits for the previous pid and only the owner removes the record` (d1c39d99).

---

## Task 4: Service log under the Julie home

**Files:**
- Modify: `crates/julie-core/src/paths.rs:340-380` (add `pub fn logs_dir(&self) -> PathBuf { self.julie_home.join("logs") }`), `src/registry/project_log.rs:31-45` (add `pub fn in_dir(log_dir: PathBuf, file_prefix: &str) -> Self`; `new` delegates with `"julie.log"`; `open_for_date` uses the prefix), `src/request_engine/runtime_factory.rs:247-262` (`create_unbound_runtime`: after the handler is built, replace its `project_log` with `ProjectLog::in_dir(self.registry_paths.logs_dir(), "julie-service.log")`; find the field at `src/handler.rs:151`), `src/service/http.rs:166-186` (`/status` JSON gains `"log": "<logs_dir>/julie-service.log.<today>"`).
- Test: `src/tests/service/http_api.rs`.

**Interfaces:**
- Consumes: `ProjectLog` (`src/registry/project_log.rs`), `RegistryPaths`.
- Produces: `RegistryPaths::logs_dir`, `ProjectLog::in_dir`, `/status.log`.

**Contract inputs:** design "Decision" item 4.
**File ownership:** as listed.
**Serialization required:** Yes.
**Dependency reason:** `runtime_factory.rs` shared with Task 1.

**Test:** `status_reports_a_service_log_under_the_julie_home` (`src/tests/service/http_api.rs`, model on the existing status test there): start the in-process service on a temp `JULIE_HOME`, make one `manage_workspace` `list` call, `GET /status`, assert `log` starts with `<home>/logs/julie-service.log.` and the file exists and is non-empty.

**Acceptance criteria:**
- [x] Test passes by exact name.
- [x] Per-project `.julie/logs/julie.log.<date>` behavior for bound runtimes is unchanged (`registry` bucket green).
- [x] Commit: `feat(service): write the service log under the Julie home and report it in status` (a490baaa).

---

## Task 5: Hermetic tests and database-derived indexes dir

**Files:**
- Modify: `.cargo/config.toml` (add `JULIE_HOME = { value = "target/test-julie-home", relative = true }` under `[env]`), `src/registry/database.rs:11-46` (`DaemonDatabase` gains `path: PathBuf` set in `open`; add `pub fn path(&self) -> &Path` and `pub fn indexes_dir(&self) -> PathBuf { self.path.parent().map(|p| p.join("indexes")).unwrap_or_else(|| PathBuf::from("indexes")) }`), `src/tools/workspace/commands/registry/mod.rs:15-24` (`registry_store_for` uses `daemon_db.indexes_dir()`; drop the `RegistryPaths::try_new()` call).
- Test: `src/tests/registry/database.rs`, `src/tests/tools/workspace/global_targeting/list.rs`.

**Interfaces:**
- Consumes: `DaemonDatabase::open` (`src/registry/database.rs:19`), `WorkspaceRegistryStore::new`.
- Produces: `DaemonDatabase::path`, `DaemonDatabase::indexes_dir`.

**Contract inputs:** design "Decision" item 5.
**File ownership:** as listed.
**Serialization required:** Yes.
**Dependency reason:** Task 6 uses the fixed helper.

**Tests:**
- `registry_store_indexes_dir_sits_beside_its_database` (`src/tests/registry/database.rs`): open a `DaemonDatabase` at `<tmp>/registry.db`, call `registry_store_for`, assert `indexes_dir() == <tmp>/indexes`.
- `list_sweep_with_a_temp_registry_deletes_nothing_outside_its_home` (`src/tests/tools/workspace/global_targeting/list.rs`, next to the test at `:39`): create a decoy directory `<other>/indexes/decoy_deadbeef/facts.sqlite` and point `JULIE_HOME` at `<other>` with the existing `with_env` guard from `src/tests/registry/paths.rs` under `#[serial(home_env)]`; run `manage_workspace` `list` on a handler with a temp `daemon.db`; assert the decoy survives.
- `tests_never_see_the_real_julie_home` (`src/tests/registry/paths.rs`): assert `RegistryPaths::try_new()` resolves under `target/test-julie-home` when the var is untouched, and never under `dirs::home_dir()`. Skip with a clear message if `JULIE_HOME` was exported by the caller to something else.

**Acceptance criteria:**
- [x] Three tests pass by exact name; the two existing `~/.julie` default tests in `src/tests/registry/paths.rs` still pass.
- [x] `grep -rn 'RegistryPaths::try_new' src/tools/workspace/commands/registry/mod.rs` is empty.
- [x] Commit: `fix(tests): isolate the Julie home for every test and derive the indexes dir from the registry database` (2dd45611).

---

## Task 6: Cleanup sweep at service start

**Files:**
- Modify: `src/service/mod.rs` (`ServiceApp::run` or `run_service`, after the listener binds and before serving: `tokio::spawn` a task that opens `DaemonDatabase` at `registry_paths.registry_db()`, builds `registry_store_for`, builds `WorkspaceCleanupActivity::new(HashSet::new())`, calls `run_cleanup_sweep`, and logs `pruned_workspaces.len()`, `pruned_orphan_dirs.len()`, `blocked_workspaces.len()` at `info!`).
- Test: `src/tests/service/http_api.rs`.

**Interfaces:**
- Consumes: `run_cleanup_sweep` (`src/tools/workspace/commands/registry/cleanup.rs:342`), `CleanupSweepSummary`, `WorkspaceCleanupActivity::new` (`cleanup.rs`; read with `deep_dive`), `registry_store_for` (Task 5 shape).

**Contract inputs:** design "Decision" item 6.
**File ownership:** as listed.
**Serialization required:** Yes.
**Dependency reason:** Needs Tasks 3 and 5.

**Test:** `service_start_prunes_dead_registry_rows_in_the_background`: on a temp `JULIE_HOME`, insert a workspace row whose path is `<tmp>/gone` (never created) and one live row for a real temp dir, plus an orphan directory `<home>/indexes/orphan_00000000/`; start the in-process service; poll up to 5 s until the dead row is gone; assert the live row and its index dir survive and the orphan dir is gone.

**Acceptance criteria:**
- [x] Test passes by exact name.
- [x] Sweep runs once per service start, off the request path, and never blocks bind or the first request.
- [x] Commit: `feat(service): prune dead registry rows and orphan index dirs at start` (964d14dc).

---

## Task 7: Gates, live restart check, finding, ledger

**Files:**
- Create: `docs/findings/2026-09-11-index-hygiene.md`, `docs/plans/2026-09-11-index-hygiene-ledger.md`.

**Contract inputs:** design "Verification".
**File ownership:** as listed.
**Serialization required:** Yes.
**Dependency reason:** Lead-run after Task 6.

**What to build:**
1. Branch gate per the Verification Strategy. Before `dev`, record `select count(*) from workspaces` in `~/.julie/registry.db` and `stat --format=%W` of every `facts.sqlite` under `~/.julie/indexes/`. After `full`, both unchanged.
2. `cargo build --release`, copy the sidecar beside the binary, `target/release/julie-server service restart` twice in a row with the ten corpus repos registered. Record `facts.sqlite` birth times and `count(*) from vectors` before and after each restart. Bar: no births change, no counts drop, exactly one `julie-server … service` process after each restart, `service status` shows the `log` path and the file has the sweep line.
3. Finding: what changed, gate table, the before-and-after registry and index evidence, the restart evidence, and the sweep counts.
4. Ledger with every row.

**Acceptance criteria:**
- [x] All gates pass; registry and index evidence unchanged across `dev` and `full`.
- [x] Two restarts change nothing on disk (restarts 2 and 3 on 84c75ba5; restart 1 rebuilt the indexes the pre-Task-5 test pollution had deleted).
- [x] Commit: `docs(service): index hygiene gate finding and ledger`.
