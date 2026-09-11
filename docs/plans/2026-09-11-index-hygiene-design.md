# Index and Registry Hygiene Design

**Status:** draft for owner review
**Owner direction (2026-09-11):** fix this before anything else; two full index rebuilds in one day cost an hour each.
**Path:** fast path. Two read-only Opus investigations (checkpoint `6bbb54be` context) supplied every file:line below.

## What actually happened

Three separate defects, all confirmed in code:

1. **Any open failure deletes the store.** `open_or_recreate` (`src/tools/workspace/indexing/store_open.rs:21-33`) and `open_or_recreate_store` (`crates/julie-runtime/src/workspace/mod.rs:102-116`) run `remove_dir_all(indexes/<id>/)` on *any* `Err` from `CheckoutStore::open`. A 5 s SQLite busy timeout, a WAL recovery conflict, or a racing `Index::create_in_dir` on `tantivy/` all return plain errors that look the same as the one intended trigger, `VersionMismatch` (`crates/julie-index/src/checkout_store/mod.rs:36-41,79-91`). Three more layers delete again on the same errors: `store_open.rs:80-91`, `src/startup_repair_plan.rs:50-67`, `src/request_engine/runtime_factory.rs:283-300`. The last one deletes, retries once, and if the retry fails leaves nothing behind. That is Event B: both julie checkout indexes gone, never recreated.

2. **Restart overlaps two services, then `/status` opens every checkout through the deleting opener.** `service restart` (`src/main.rs:111-128`) posts `/shutdown`, waits at most 1 s for `service.json` to vanish, then spawns. The old process removes the record (`src/service/mod.rs:114`) *before* it drops its stores and SQLite handles. `try_connect` (`src/service/client.rs:100-103`) also deletes the record itself when `status()` fails, so a second service spawns while the first still owns every index; the dying first process then deletes the second's record. Restart ends with `client.status()`, and `/status` (`src/service/http.rs:166-186`) runs `manage_workspace status` over every registry row through `store_for_workspace` (`status.rs:111` → `store_open.rs:88`). Every non-primary checkout that fails to open under that contention is wiped. The primary survives because it is served from memory (`store_open.rs:70-74`). That is Event A, both times.

3. **Tests run against the live home.** `.cargo/config.toml` sets `JULIE_EMBEDDING_PROVIDER=none` but not `JULIE_HOME`, so `RegistryPaths::try_new` (`crates/julie-core/src/paths.rs:255-287`) falls back to `~/.julie`. Twelve tests in the `cli` bucket write registry rows there on every `dev` run (`src/tests/cli_execution_tests.rs:514-645`, `src/tests/cli/mod.rs:36` and nine call sites, `src/tests/cli_input_contract.rs:548-646`). Worse, `registry_store_for` (`src/tools/workspace/commands/registry/mod.rs:15-24`) pairs *whatever database it is handed* with the env-derived `~/.julie/indexes`, so the four `list` tests in `src/tests/tools/workspace/global_targeting/{list,rebind_index}.rs` (full tier only) run `prune_orphan_index_dirs` with a temp registry against the real indexes directory and delete every live index, logging the event into the temp database. That also explains why `workspace_cleanup_events` showed nothing.

Also found: the service log goes to a `tempfile::tempdir()` root (`runtime_factory.rs:239-241`) that is deleted, so no service log exists on disk. `service status` shows `vector_count` 0 for stores it has not loaded.

## Decision

Six changes, smallest first. Together they are one plan.

1. **Delete only on `VersionMismatch`.** In both `open_or_recreate` functions: `Err(err) if err.downcast_ref::<VersionMismatch>().is_some()` deletes and reopens; every other error propagates. Remove the duplicate delete layers at `store_open.rs:80-91`, `startup_repair_plan.rs:50-67`, and `runtime_factory.rs:288-300` so one classified decision is made once.
2. **`/status` never opens through a deleting path.** `status.rs:111` calls `handler.checkout_store_for_workspace` (non-destructive, `handler.rs:1366`); its existing `Err(_)` arm already reports the checkout as not loaded.
3. **Restart waits for the old process, and only the owner touches the record.** `main.rs:117-122` polls the record's `pid` for exit with a ceiling of 30 s. `client.rs:100-103` does not remove a record whose `pid` is alive. `service/mod.rs:114` removes the record only if it names `std::process::id()`. No lock file, no lease: the pid in the record we already write is enough.
4. **Service log at `$JULIE_HOME/logs/julie-service.log.<date>`.** Reuse `install_file_tracing` (`src/logging.rs:126`) with the registry paths' home instead of the unbound runtime's temp root.
5. **Hermetic tests.** `.cargo/config.toml` gains `JULIE_HOME = { value = "target/test-julie-home", relative = true }`. This reaches in-process tests and the eleven tests that spawn `target/debug/julie-server`, because children inherit the env. An exported `JULIE_HOME` still wins. `registry_store_for` derives the indexes directory from the database it is given: `DaemonDatabase` gains a `path` field and `indexes_dir()` returns `path.parent().join("indexes")`. The two tests that assert the `~/.julie` default already unset the var first.
6. **Cleanup sweep at service start, in the background.** `ServiceApp::new` spawns `run_cleanup_sweep` once after bind, on the machine registry. It prunes rows whose path is gone and orphan index dirs, exactly what `manage_workspace list` does today. With change 5 the sweep can never see a foreign indexes directory.

Not in scope: bounding store memory, the `/status` full-store cost, `readiness.status` timing, `vector_count` after restart. Those stay on the brief under Astra's findings.

## Acceptance criteria

- [ ] A store whose SQLite is locked by another connection is not deleted; the open returns an error and the directory is intact. A store with a mismatched `meta.engine_version` is deleted and rebuilt.
- [ ] `/status` and `service status` open no store through a deleting path; a checkout that fails to open reports as not loaded.
- [ ] `service restart` waits for the previous pid to exit; a second service is never spawned while the first pid is alive; a dying process never removes a record it does not own.
- [ ] `service status` reports a `log` path under `$JULIE_HOME/logs/` and the file exists after one request.
- [ ] `cargo xtask test dev` and `full` add zero rows to `~/.julie/registry.db` and touch nothing under `~/.julie/indexes/`. A `list` test with a temp registry deletes nothing outside its own home.
- [ ] Service start prunes dead registry rows and orphan index dirs in the background and logs the counts.
- [ ] Net lines negative or flat; three duplicate delete layers removed.

## Verification

Worker tests per change, exact names in the plan. Lead gates: `changed`, `system` (startup and service changed), `dev`, `full` with `NEXTEST_TEST_THREADS=6`. Then a live check: restart the service twice in a row on a rebuilt binary with all ten corpus repos open and confirm no `facts.sqlite` birth time changes and no vector count drops. Finding at `docs/findings/2026-09-1X-index-hygiene.md`.
