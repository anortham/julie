# Autonomous Execution Report - Index and Registry Hygiene

**Status:** Awaiting publication approval
**Plan:** docs/plans/2026-09-11-index-hygiene-plan.md
**Branch:** index-hygiene (worktree `.worktrees/index-hygiene`, HEAD after this report's commit)
**PR:** pending — filled in after PR creation
**Publication authority:** local commit=authorized — owner "approved" the plan on 2026-09-11; push=missing — no user or repo instruction grants it (main is 151 commits ahead of origin, unpushed by the owner's choice); PR=missing — no instruction grants it; the owner has merged every branch in this line locally
**Duration:** about 7.5 hours wall (plan written 09:50 local, gates and live check done 12:13 local); Workflow agent time about 1.8 hours across 30 Opus agents
**Phases:** 1/1 complete
**Tasks:** 7/7 complete
**External-model policy:** no policy declared — no external provider received the diff (Opus workers ran inside this session's Workflow)

## What shipped
- Task 1 `bca54911`: both checkout-store openers delete `indexes/<id>/` only on the typed `VersionMismatch`; three duplicate delete layers removed; handler opens through the single opener and never caches a store it just rebuilt; dropped repair failures logged with the index root.
- Task 2 `27bf69ee`: `status` reads checkouts through the non-destructive opener.
- Task 3 `d1c39d99`: `discovery::pid_alive`; restart waits up to 30 s for the old pid; `try_connect` removes the record only when the pid is dead; the service removes only its own record; the shim reaps the service child.
- Task 4 `a490baaa`: `$JULIE_HOME/logs/julie-service.log.<date>`, reported in `/status.log`.
- Task 5 `2dd45611`: `.cargo/config.toml` pins `JULIE_HOME=target/test-julie-home` for every cargo-spawned process; `DaemonDatabase` keeps its path and the registry store derives its indexes dir from it.
- Task 6 `964d14dc`: one background cleanup sweep at service start, counts logged.
- Task 7 `84c75ba5`, `91e026f7`: the live check found the service installs no tracing subscriber; fixed in `ServiceApp::new`. Finding `docs/findings/2026-09-11-index-hygiene.md`, ledger `docs/plans/2026-09-11-index-hygiene-ledger.md`.

## Judgment calls (non-blocking decisions made)
- `src/tests/tools/workspace/store_open.rs:44` — Chose `PRAGMA locking_mode = EXCLUSIVE` over the brief's `BEGIN EXCLUSIVE` because facts.sqlite runs in WAL mode and an exclusive transaction does not block readers there; the brief's lock would have left the test green against the old code.
- `src/handler.rs:1387` — Chose to route `checkout_store_for_workspace` through `open_or_recreate` (outside Task 1's file list) over a retry arm in `store_for_workspace` because seven callers share the handler path and only one would have healed.
- `src/tools/workspace/indexing/store_open.rs` — Lead cap ruling after three Task 1 fix rounds: the last open finding (mismatch tests did not prove the whole directory was removed) was fixed by the lead with a stale-marker assertion; the four worker commits were squashed to one.
- `src/service/discovery.rs:66` — Chose a `/proc/<pid>/stat` state read on Linux over reaping in `spawn_detached_service` alone because a record can name a pid this shim never spawned; the shim also reaps its own child so macOS and BSD are covered for the common case.
- `src/service/client.rs:150` — Chose a named `reap_in_background` seam over an inline thread so the test does not spawn `current_exe() service`.
- `src/service/http.rs:166` — Chose `Json<Value>` with an inserted `log` key over a new `StatusDocument` field because `status.rs` was outside Task 4's ownership.
- `src/tests/registry/paths.rs:399` — Asserted `julie_home != ~/.julie` instead of "never under the home directory" because the worktree itself lives under `/home/murphy`.
- `src/tests/tools/workspace/global_targeting/global_remove.rs` — Fixed the two tests (outside Task 5's file list) to build the index path from the database, not the environment, because the approved design makes the database the source of truth.
- `src/service/mod.rs:98` — Sweep spawned in `serve` after the record is written, not in `new`, because the brief's ordering ("after bind, before serving") only holds there.
- `src/service/mod.rs:53` — Default tracing filter `info` rather than `julie=info` because the watcher and runtime lines come from `julie_runtime` and `julie_core` targets; `RUST_LOG` overrides.

External review: none (not requested for this run).

## Review campaign
- **State:** clean
- **Evidence:** lead-only (Opus reviewer agent per task, lead adjudication at the Task 1 cap)
- **Round:** 0
- **External invocations:** 0
- **Open critical/high:** 0
- **Open medium/low:** 0
- **Open at/above floor:** 0

## Tests
- 18 new tests by exact name; `system` 4 buckets, `dev` 30 buckets, `full` 47 buckets (`NEXTEST_TEST_THREADS=6`) at `964d14dc`; `system` and `dev` again at `91e026f7`; `cargo fmt --check` and `clippy` clean of errors. Live: two restarts on eleven open workspaces changed no `facts.sqlite` birth time and no vector count.

## Blockers hit
- None. One approval boundary: push and PR authority are missing (see Publication authority).

## Files changed
- 45 files changed, 2040 insertions(+), 255 deletions(-) over `main` at `537d9f8e`. Product: `store_open.rs`, `handler.rs`, `runtime_factory.rs`, `startup_repair_plan.rs`, `julie-runtime/workspace/mod.rs`, `status.rs`, `main.rs`, `service/{client,discovery,mod,http}.rs`, `paths.rs`, `project_log.rs`, `registry/database.rs`, `registry/mod.rs`, `.cargo/config.toml`. Tests: `store_open.rs`, `service/{client,control,http_api,durable_roots}.rs`, `registry/{database,paths}.rs`, `global_targeting/{list,global_remove}.rs`, `global_targeting.rs`, `tests/mod.rs`. Docs: design, plan, ledger, finding, checkpoints.

## Source control
- **Outstanding:** None — all commits ride on `index-hygiene`, which sits directly on `main` (`537d9f8e`).
- **Worktrees left in place:** `.worktrees/index-hygiene` (this run's branch; disposition after merge is the owner's call). The main checkout carries the owner's own modified `.codex/config.toml` and untracked `.memories/2026-09-11/133457_b2d2.md`; untouched.

## Next steps
- Owner merges `index-hygiene` into `main` locally (or grants push and PR authority).
- After the merge: `cargo build --release` on main, copy `julie-semantic-sidecar` beside it, `service restart`. The live service currently runs the worktree binary at `.worktrees/index-hygiene/target/release/julie-server`.
- Update the Goldfish brief: item 2 done; carry the deferred observations (ErrorBuffer layer never installed, partial vector backfill does not resume after restart, `new_files` repair on every restart, RSS 2.9 GB with eleven workspaces, stray `/tmp` registry row) into items 3, 5 and 7.
- Item 3: test speed.
