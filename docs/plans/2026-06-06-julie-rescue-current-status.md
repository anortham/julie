# Julie Rescue Current Status

**Date:** 2026-06-06
**Current local baseline:** `b5ed8ef8` (`docs(rescue): record current status`)
**Practical status:** Julie is saved in place, the daemon/adapter teardown has landed, and the first test-economy cut has reduced the default `dev` tier below 10 minutes expected and actual. The rescue is not done; the next bottleneck is splitting the slow broad buckets without losing coverage.

---

## Current Answer

Do not reopen the daemon teardown as active implementation work. Phase 3d.3 is merged and branch-gated.

Do focus the next rescue slice on decomposing the slow broad buckets and retiring stale runtime vocabulary. The repo has real crate-split wins, and the calibrated `dev` tier now targets 27 buckets / 589s expected; the first actual run passed in 389.7s. `full` still carries the broad release coverage at 44 buckets / 2519s expected.

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
- A `julie-plugin` launcher fix is prepared in `/Users/murphy/source/julie-plugin-single-server` on `fix/single-server-launcher` at `4165fcb` (`fix: launch julie-server directly`). It launches `julie-server` directly and keeps only legacy split-daemon cleanup.

---

## Current Test-Suite State

From `cargo xtask test list` on 2026-06-06:

| Tier | Buckets | Expected Runtime |
| --- | ---: | ---: |
| `nano` | 2 | 65s |
| `smoke` | 4 | 80s |
| `dev` | 27 | 589s / 9.8m expected; 389.7s actual |
| `system` | 5 | 225s / 3.8m |
| `dogfood` | 2 | 380s / 6.3m |
| `full` | 44 | 2519s / 42.0m |

Broad buckets removed from `dev` and retained in `full`:

- `tools-workspace`: 300s
- `tools-search-line`: 250s
- `tools-editing`: 200s
- `tools-workspace-targeting`: 170s
- `tools-search-format-quality`: 100s
- `tools-call-path`: 80s

This addresses the user's "30 minute test suite" complaint for the default branch gate, while preserving the broad release coverage in `full`.

---

## What Is Left

### Release-Blocking Cleanup

1. Merge or publish the prepared `julie-plugin` launcher branch so release packaging invokes the current `julie-server` binary, not deleted `julie-adapter` / `julie-daemon` binaries.
2. Finish current-status wording cleanup in code comments and docs that still say "daemon mode" for the in-process registry/session path.

### Test-Economy Work

1. Run actual wall-clock timing for the broad buckets now retained only in `full`.
2. Split the slow handler-bound buckets first: `tools-workspace`, `tools-search-line`, `tools-editing`, `tools-workspace-targeting`, `tools-search-format-quality`, and `tools-call-path`.
3. Move any still-handler-free tests from the top crate into `julie-tools` / `julie-runtime`; keep only real handler/in-process integration tests in the top crate.
4. Re-admit cheap representative slices into `dev` only when the 600s contract stays green.
5. Rename stale bucket/module vocabulary from `daemon` to `registry-runtime` or equivalent once the test ownership is clear.

### Complexity Work

1. Rename or wrap `DaemonDatabase` as `RegistryDatabase` and migrate tests away from temp `daemon.db` except where testing legacy migration.
2. Keep `src/daemon` only as long as it holds genuinely retained compatibility pieces; then split registry, leadership, project-log, and embedding-service code into accurate modules.
3. Resume Phase 4 only after the test loop is cheap enough: tool taxonomy consolidation (`edit_*` trio, `fast_refs`/`call_path`) and tool-list contract tests.

---

## Recommended Next Slice

Do the next small, evidence-first PR:

1. Split `tools-workspace` into smaller command-owned buckets.
2. Route workspace-specific paths to those buckets in `xtask/src/changed.rs`.
3. Run `cargo nextest run -p xtask` plus the narrow affected workspace bucket(s), then leave `full` for the broader pre-merge gate.

That slice preserves Julie behavior while reducing the cost of changed-path verification and making the remaining complexity visible.

---

## References

- `docs/plans/2026-06-06-julie-test-economy-plan.md` — active plan for the test loop and slow bucket splits.
- `docs/plans/2026-06-05-julie-phase3d-delete-daemon-adapter.md` — daemon/adapter teardown verification ledger.
- `docs/plans/2026-06-03-julie-rescue-design.md` — original rescue strategy and rationale.
