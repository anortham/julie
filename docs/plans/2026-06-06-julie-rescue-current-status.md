# Julie Rescue Current Status

**Date:** 2026-06-06
**Current local baseline:** `4bfa2361` (`test(xtask): split workspace bucket`)
**Practical status:** Julie is saved in place, the daemon/adapter teardown has landed, the default `dev` tier is below 10 minutes expected and actual, and the first two broad bucket splits are done. The rescue is not done; the next bottleneck is splitting the remaining slow broad buckets without losing coverage.

---

## Current Answer

Do not reopen the daemon teardown as active implementation work. Phase 3d.3 is merged and branch-gated.

Do focus the next rescue slice on decomposing the remaining slow broad buckets and retiring stale runtime vocabulary. The repo has real crate-split wins, and the calibrated `dev` tier now targets 27 buckets / 589s expected; the first actual run passed in 389.7s. `full` still carries the broad release coverage at 48 buckets / 2534s expected.

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
| `full` | 48 | 2534s / 42.2m |

Broad buckets removed from `dev` and retained in `full`:

- `tools-workspace`: split into `tools-workspace-discovery` (1.1s actual), `tools-workspace-indexing` (282.9s actual), and `tools-workspace-management` (4.1s actual)
- `tools-search-line`: split into `tools-search-line-core` (31.6s actual), `tools-search-line-filters` (140.2s actual), and `tools-search-line-primary` (64.9s actual)
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
2. Split the remaining slow handler-bound buckets first: `tools-editing`, `tools-workspace-targeting`, `tools-search-format-quality`, and `tools-call-path`.
3. Move any still-handler-free tests from the top crate into `julie-tools` / `julie-runtime`; keep only real handler/in-process integration tests in the top crate.
4. Re-admit cheap representative slices into `dev` only when the 600s contract stays green.
5. Rename stale bucket/module vocabulary from `daemon` to `registry-runtime` or equivalent once the test ownership is clear.

### Complexity Work

1. Rename or wrap `DaemonDatabase` as `RegistryDatabase` and migrate tests away from temp `daemon.db` except where testing legacy migration.
2. Keep `src/daemon` only as long as it holds genuinely retained compatibility pieces; then split registry, leadership, project-log, and embedding-service code into accurate modules.
3. Keep MCP tool consolidation out of the active rescue path unless the user reopens it. Preserve the current tool surface while test economics improve.

---

## Recommended Next Slice

Do the next small, evidence-first PR:

1. Split `tools-editing` so edit-tool changes no longer require one 200s bucket with eight commands.
2. Route editing-specific paths to the new buckets in `xtask/src/changed.rs`.
3. Run `cargo nextest run -p xtask` plus the affected editing bucket(s), then leave `full` for the broader pre-merge gate.

That slice preserves Julie behavior while reducing the cost of changed-path verification and making the remaining complexity visible.

---

## References

- `docs/plans/2026-06-06-julie-test-economy-plan.md` — active plan for the test loop and slow bucket splits.
- `docs/plans/2026-06-05-julie-phase3d-delete-daemon-adapter.md` — daemon/adapter teardown verification ledger.
- `docs/plans/2026-06-03-julie-rescue-design.md` — original rescue strategy and rationale.
