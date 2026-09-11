# Index Hygiene Verification Ledger

Plan: `docs/plans/2026-09-11-index-hygiene-plan.md`. Finding: `docs/findings/2026-09-11-index-hygiene.md`.

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| Task 1: locked store survives a failed open; mismatch rebuilds and removes the whole dir; recovering store returns an error without deleting | `cargo nextest run --lib <each Task 1 test>` | worker-red-green | bca54911 | pass | 2026-09-11T15:30:00Z | no |
| Task 1 diff-scoped buckets | `XTASK_CHANGED_PATHS=<task paths> cargo xtask test changed` | affected-change | bca54911 | pass, 35 buckets | 2026-09-11T15:40:00Z | no |
| Task 2: status over a locked checkout deletes nothing | `cargo nextest run --lib status_over_a_locked_checkout_reports_not_loaded_and_deletes_nothing` | worker-red-green | 27bf69ee | pass | 2026-09-11T15:44:00Z | no |
| Task 2 diff-scoped buckets | `cargo xtask test changed`; `cargo xtask test fast` | affected-change | 27bf69ee | pass | 2026-09-11T15:50:00Z | no |
| Task 3: record survives while the pid is alive; record removed when the pid is dead; shutdown removes only an owned record; zombie counts as dead; spawned child is reaped | `cargo nextest run --lib <each Task 3 test>` | worker-red-green | d1c39d99 | pass | 2026-09-11T16:05:00Z | no |
| Task 3 service buckets | `cargo xtask test changed`; `cargo xtask test bucket service`; `cargo xtask test bucket service-process` | affected-change | d1c39d99 | pass | 2026-09-11T16:10:00Z | no |
| Task 4: status reports a service log under the Julie home | `cargo nextest run --lib status_reports_a_service_log_under_the_julie_home` | worker-red-green | a490baaa | pass | 2026-09-11T16:16:00Z | no |
| Task 4 service bucket | `cargo xtask test changed`; `cargo xtask test bucket service` | affected-change | a490baaa | pass | 2026-09-11T16:20:00Z | no |
| Task 5: indexes dir sits beside the database; temp-registry sweep deletes nothing outside its home; tests never see the real Julie home | `cargo nextest run --lib <each Task 5 test>` | worker-red-green | 2dd45611 | pass | 2026-09-11T16:30:00Z | no |
| Task 5 targeting bucket (after the global_remove fixture fix) | `cargo xtask test changed`; `cargo xtask test fast`; `cargo xtask test bucket tools-workspace-targeting` | affected-change | 2dd45611 | pass | 2026-09-11T16:38:00Z | no |
| Task 6: service start prunes dead rows and orphan dirs in the background | `cargo nextest run --lib service_start_prunes_dead_registry_rows_in_the_background` | worker-red-green | 964d14dc | pass | 2026-09-11T16:46:00Z | no |
| Task 6 service bucket | `cargo xtask test changed`; `cargo xtask test bucket service` | affected-change | 964d14dc | pass | 2026-09-11T16:47:00Z | no |
| Formatting and lints | `cargo fmt --check`; `cargo clippy --workspace --all-targets` | branch-gate | 964d14dc | pass, 0 errors | 2026-09-11T16:47:30Z | no |
| Startup and integration buckets | `cargo xtask test system` | branch-gate | 964d14dc | pass, 4 buckets, 7.1 s warm | 2026-09-11T16:48:26Z | no |
| Batch regression tier | `cargo xtask test dev` | branch-gate | 964d14dc | pass, 30 buckets | 2026-09-11T16:49:26Z | no |
| Broad pre-merge tier | `NEXTEST_TEST_THREADS=6 cargo xtask test full` | expensive-specialist | 964d14dc | pass, 47 buckets, 94.3 s warm | 2026-09-11T16:51:06Z | no |
| Tests leave the live home alone | `sqlite3 ~/.julie/registry.db 'select count(*) from workspaces'`; `ls ~/.julie/indexes` before and after `dev` and `full` | branch-gate | 964d14dc | pass, 12 rows both times, indexes dir untouched | 2026-09-11T16:51:30Z | no |
| Service log carries tracing lines (sweep summary) and tool calls | `cargo nextest run --lib status_reports_a_service_log_under_the_julie_home` (RED then GREEN); `cargo xtask test bucket service`; `cargo xtask test changed` | worker-red-green, affected-change | 84c75ba5 | pass | 2026-09-11T17:02:00Z | no |
| Two restarts change nothing on disk | `target/release/julie-server service restart` twice with eleven workspaces reopened through `/api/manage_workspace open`; `stat --format=%W` and `select count(*) from vectors` per `facts.sqlite` before and after | expensive-specialist (live) | 84c75ba5 | pass, 11/11 births unchanged, vector counts identical, one service pid, sweep line logged | 2026-09-11T17:07:28Z | no |
| Security scope | none declared | branch-gate | 84c75ba5 | not applicable | 2026-09-11T17:07:28Z | no |
