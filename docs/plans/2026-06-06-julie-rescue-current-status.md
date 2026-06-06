# Julie Rescue Current Status

**Date:** 2026-06-06
**Current `main`:** `49b86689` (`refactor(rescue): finish phase 3d daemon teardown`)
**Practical status:** Julie is saved in place, the daemon/adapter teardown has landed, but the rescue is not done. The next bottleneck is test-suite economics.

---

## Current Answer

Do not reopen the daemon teardown as active implementation work. Phase 3d.3 is merged and branch-gated.

Do focus the next rescue slice on shrinking the default verification loop. The repo has real crate-split wins, but the calibrated `dev` tier is still configured as 37 buckets with about 35 minutes of expected runtime. The latest 3d.3 affected-change gate passed in 958.5s, which is better than the expectation but still too slow for normal agent iteration.

---

## What Is Done

- **Moat decision:** Julie beat Miller in the one-shot retrieval bakeoff strongly enough to keep rescuing Julie instead of switching hosts.
- **Phase 0:** `julie-core` extracted; pure database tests moved into their own crate; relink proof recorded.
- **Phase 1:** `julie-index` extracted for search + analysis; search/analysis tests moved down where they are handler-free.
- **Phase 2:** the workspace now has `julie-core`, `julie-index`, `julie-pipeline`, `julie-context`, `julie-tools`, `julie-runtime`, and `julie-test-support`. The top crate is smaller, but still owns handler-bound integration behavior.
- **Phase 3b/3c:** the resident embedding host and in-process stdio leader path replaced the old daemon serving path.
- **Phase 3d.3:** the HTTP daemon, stdio adapter, PID/discovery runtime, search-compare dashboard/data surface, and old migration path are gone. The dashboard is now a standalone read-only `registry.db` reader.

---

## Quality Read On The 3d.3 Work

Good:

- It landed as deletion-first work, not a rewrite in disguise.
- The branch gates recorded in `docs/plans/2026-06-05-julie-phase3d-delete-daemon-adapter.md` cover the critical surfaces: `changed`, `system`, `reliability`, dashboard, registry migrations, target-workspace metrics, cleanup safety, and `search-matrix baseline`.
- The two prior 3d.2b review findings were actually addressed in 3d.3: current-workspace cleanup is blocked, and target-workspace `source_bytes` attribution uses the target binding.

Caution:

- Naming debt remains. The central registry type is still `DaemonDatabase`, many tests still construct temp `daemon.db` files, and some internal comments still say "daemon mode" where they now mean "in-process/shared registry mode".
- The `src/daemon` module still contains kept code: registry DB, leader-lock compatibility, embedding-service glue, project logs, and recovery markers. That is not a correctness bug, but it keeps the code harder to reason about.
- The `julie-plugin` repo still has adapter/daemon launcher references and must be fixed before release packaging.

---

## Current Test-Suite State

From `cargo xtask test list` on 2026-06-06:

| Tier | Buckets | Expected Runtime |
| --- | ---: | ---: |
| `nano` | 2 | 65s |
| `smoke` | 4 | 80s |
| `dev` | 37 | 2102s / 35.0m |
| `system` | 5 | 360s / 6.0m |
| `reliability` | 3 | 195s / 3.2m |
| `full` | 44 | 2462s / 41.0m |

Largest `dev` estimates:

- `tools-workspace`: 300s
- `tools-search-line`: 250s
- `tools-editing`: 200s
- `tools-deep-dive`: 200s
- `tools-workspace-targeting`: 170s

This explains the user's "30 minute test suite" complaint: the rescue moved substantial tests into lower crates, but the default branch gate still aggregates too much handler-bound coverage.

---

## What Is Left

### Release-Blocking Cleanup

1. Fix `julie-plugin` launcher/update packaging so it invokes the current `julie-server` binary, not deleted `julie-adapter` / `julie-daemon` binaries.
2. Finish current-status wording cleanup in code comments and docs that still say "daemon mode" for the in-process registry/session path.

### Test-Economy Work

1. Run a bucket timing audit on the current `dev` tier and record actual wall-clock per bucket.
2. Split `dev` into a smaller default branch gate and a broader release/full gate. `changed` should stop falling back to all of `dev` for ordinary localized edits.
3. Attack the slow handler-bound buckets first: `tools-workspace`, `tools-search-line`, `tools-editing`, `tools-deep-dive`, and `tools-workspace-targeting`.
4. Move any still-handler-free tests from the top crate into `julie-tools` / `julie-runtime`; keep only real handler/in-process integration tests in the top crate.
5. Rename stale bucket/module vocabulary from `daemon` to `registry-runtime` or equivalent once the test ownership is clear.

### Complexity Work

1. Rename or wrap `DaemonDatabase` as `RegistryDatabase` and migrate tests away from temp `daemon.db` except where testing legacy migration.
2. Keep `src/daemon` only as long as it holds genuinely retained compatibility pieces; then split registry, leadership, project-log, and embedding-service code into accurate modules.
3. Resume Phase 4 only after the test loop is cheap enough: tool taxonomy consolidation (`edit_*` trio, `fast_refs`/`call_path`) and tool-list contract tests.

---

## Recommended Next Slice

Do one small, evidence-first PR:

1. Fix `julie-plugin` packaging references.
2. Rename the `daemon` xtask bucket label/notes if it now covers registry-runtime behavior.
3. Add a timing report for the current `dev` tier and choose a new default branch gate target under 10 minutes.

That slice preserves Julie behavior while directly targeting the remaining rescue goal: less complexity and a faster verification loop.
