# Index and Registry Hygiene Finding

**Verdict:** passed on 2026-09-11. The service deletes an index only on a typed facts version mismatch, `/status` reads through the non-destructive opener, restart waits for the old pid and only the owner removes `service.json`, the service log lives at `$JULIE_HOME/logs/julie-service.log.<date>` and carries tracing lines, tests never touch the developer's `~/.julie`, and a cleanup sweep runs once at service start. Two live restarts on eleven open workspaces changed no index birth time and dropped no vector count.

Plan: `docs/plans/2026-09-11-index-hygiene-plan.md`. Design: `docs/plans/2026-09-11-index-hygiene-design.md`. Ledger: `docs/plans/2026-09-11-index-hygiene-ledger.md`. Branch `index-hygiene`, product commits `bca54911..84c75ba5` over `main` at `f57a1655`.

## What changed

- `open_or_recreate` and `open_or_recreate_store` delete `indexes/<id>/` only when `CheckoutStore::open` returns the typed `VersionMismatch`. Every other failure propagates and names the store directory. The three duplicate delete layers (`store_for_workspace` retry, `startup_repair_plan`, `initialize_recovering_store`) are gone.
- `checkout_store_for_workspace` opens through the single opener and does not cache a store it just rebuilt. Dropped repair-scan failures at bind are now logged with the index root.
- `status` reads every checkout through `checkout_store_for_workspace`.
- `discovery::pid_alive` (Linux reads `/proc/<pid>/stat` so a zombie counts as dead; other unix `kill(pid, 0)`; Windows `OpenProcess`). Restart polls it for up to 30 s after shutdown. `try_connect` removes the record only when the pid is dead. `ServiceApp::serve` removes the record only when it names this process. `spawn_detached_service` reaps its child in a background thread.
- `RegistryPaths::logs_dir`, `ProjectLog::in_dir`, and `/status.log`. The live check then showed the service process installed no tracing subscriber at all, so `ServiceApp::new` now calls `install_file_tracing` with an `info` default. `logs/` is an accepted durable root.
- `.cargo/config.toml` sets `JULIE_HOME = target/test-julie-home` for every cargo-spawned process. `DaemonDatabase` keeps its path, and `registry_store_for` derives the indexes directory from it, so a temp registry can never sweep a foreign indexes tree.
- `ServiceApp::serve` spawns `run_cleanup_sweep` once after the record is written and logs the three counts.

## Gate table

| Gate | Result |
|---|---|
| Worker tests by exact name (Tasks 1 to 6, 18 tests) | pass |
| `cargo xtask test changed` after every task; `service`, `service-process`, `tools-workspace-targeting` buckets | pass |
| `cargo fmt --check`, `cargo clippy --workspace --all-targets` | pass, 0 errors (pre-existing warnings only) |
| `cargo xtask test system` | pass, 4 buckets, 7 s warm |
| `cargo xtask test dev` | pass, 30 buckets |
| `NEXTEST_TEST_THREADS=6 cargo xtask test full` | pass, 47 buckets, 94 s warm |
| Live registry across `dev` and `full` | 12 rows before and after; `~/.julie/indexes` untouched |
| Restart 2 and restart 3 on the fixed binary, eleven workspaces reopened | 11 of 11 `facts.sqlite` birth times unchanged; vector counts identical; one service process each time; restart wall 7.2 s and 6.5 s |
| `/status.log` and the sweep line | `~/.julie/logs/julie-service.log.2026-09-11`; `Cleanup sweep finished at service start pruned_workspaces=0 pruned_orphan_dirs=0 blocked_workspaces=0` |

## Evidence of the problem, before the fix

At 16:45Z, after the worker test runs for Tasks 1 to 4 and before Task 5 landed, `~/.julie/indexes` was empty. It had held the ten corpus indexes plus `julie` that morning. The live `registry.db` still had all 12 rows and no `orphan_index_dir` event, while `workspace_cleanup_events` held 50 `auto_prune missing_path` rows for `/tmp/.tmp*` and `julie_cli_*` test workspaces. That is the exact path the design named: a test with a temp registry built a `WorkspaceRegistryStore` whose indexes directory came from the real home, and its orphan sweep deleted every directory the temp registry did not know. After Task 5 the same tiers left the live home untouched.

Restart 1 on the new binary rebuilt the eleven indexes (births 16:55Z to 16:57Z) once the JSON API reopened them; the sidecar came up and vectors backfilled.

## Review rounds worth recording

- Task 1 took three fix rounds. Each found a real gap the brief had not named: the handler bypassed the opener (so a mismatched secondary checkout would never self-heal), non-mismatch errors lacked the store path, `.ok()` swallowed repair failures at both bind sites, and a rebuilt empty store was cached for the session.
- Task 3 took two. The first `pid_alive` treated an unreaped child as alive, which would have left a hard-killed service's record forever.
- Task 5's lead gate caught `global_remove` tests that built the index path from the environment while the product now builds it from the database.

## Deferred and observed

- The dashboard `ErrorBuffer` layer is never installed, so `/status.errors` is always empty. Plan item 7.
- Read paths (health checker, data plane, tool context) can still trigger the version-mismatch delete on a foreign checkout. That matches the CLAUDE.md invariant and only fires on a real version bump.
- `connect_or_start` no longer respawns over a live pid whose `/status` fails; a wedged service needs `service stop`.
- On macOS and the BSDs `pid_alive` still uses `kill(pid, 0)`; the shim reaps its own child, so only a service started by a different dead shim could leave a record there.
- A partial vector backfill does not resume after a restart: the repair plan schedules embeddings only when the count is zero. Vector counts stayed flat across restarts 2 and 3.
- Every restart logs `Found files not in the store - indexing needed reasons=new_files` for some workspaces and runs a repair pass. Nothing was deleted, but the cause deserves a look.
- RSS was 2.9 GB with eleven workspaces open. Report-only; Astra's review already flagged the missing memory bound.
- The live registry has a stray `tmp_e9671acd` row for `/tmp` from 2026-09-08. The sweep keeps it because the path exists.
- Workers reported `deep_dive` failing with `unable to open database file ~/.julie/indexes/julie_5cb3ea69/facts.sqlite` while the index was missing; it works again after the rebuild.
