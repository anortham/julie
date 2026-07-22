# Xtask Runner Module Boundary Verification Ledger

## Verification Ledger

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| Runner production modules remain within 500 lines and the test module remains within 1000 | `cargo nextest run -p xtask` | lead-xtask | `b7034de68566b3957df037ba272ba9dea8e5a871` | pass: 178/178, including `runner_implementation_files_stay_within_limit` | 2026-07-22T12:49:13Z | no |
| Runner behavior, output ordering, timing, and process-tree termination remain unchanged | `cargo nextest run -p xtask --test runner_tests` | lead-runner-behavior | `b7034de68566b3957df037ba272ba9dea8e5a871` | pass: 27/27 | 2026-07-22T12:52:59Z | no |
| Coverage transformation and prebuild ordering remain unchanged | `cargo nextest run -p xtask --test runner_coverage_tests` | lead-runner-coverage | `b7034de68566b3957df037ba272ba9dea8e5a871` | pass: 17/17 | 2026-07-22T12:53:00Z | no |
| Runner paths map to and pass the canonical focused bucket | `cargo xtask test bucket xtask-runner` | lead-mapped-bucket | `b7034de68566b3957df037ba272ba9dea8e5a871` | pass: 1 bucket; 0.9s warm, 0.4s prebuild, 1.2s cold wall | 2026-07-22T12:49:28Z | no |
| Completed Phase 2A batch passes the canonical development gate | `cargo xtask test dev` | lead-batch | `b7034de68566b3957df037ba272ba9dea8e5a871` | pass: 27 buckets; 220.4s warm, 3.8s prebuild, 224.1s cold wall | 2026-07-22T12:52:33Z | no |
| Touched runner files match the pinned formatter and declared boundaries | `rustfmt --edition 2024 --check <runner files> && wc -l <runner files>` | lead-structure | `b7034de68566b3957df037ba272ba9dea8e5a871` | pass: facade 305; prebuild 197; execution 408; rendering 107; tests 255 | 2026-07-22T12:53:20Z | no |
| Caller-facing runner files and integration suites required no edits | `git diff b7034de6^ --name-only -- xtask/src/main.rs xtask/src/changed.rs xtask/src/lib.rs xtask/tests/runner_tests.rs xtask/tests/runner_coverage_tests.rs` | lead-api-boundary | `b7034de68566b3957df037ba272ba9dea8e5a871` | pass: empty diff; callers compile in the 178-test xtask gate | 2026-07-22T12:53:20Z | no |

## TDD Evidence

- RED on the working tree based at `e9aeda8f`: the exact boundary test failed because `xtask/src/runner.rs` had 1242 lines against the 500-line limit.
- GREEN before the implementation commit: the exact boundary test passed after the split, with every production module under 500 lines.
- Exact-commit replay at `b7034de68566b3957df037ba272ba9dea8e5a871`: the full xtask gate passed all 178 tests, including the boundary test.

## Affected-Change Scope Correction

`cargo xtask test changed` reads the uncommitted working-tree diff. At the clean implementation commit it correctly reported `CHANGED: no code/test buckets matched local changes`, so it cannot prove branch-to-parent selection. The checked-in `changed_tests_xtask_paths_select_xtask_runner_bucket` routing contract passed in the exact-commit xtask gate, and the mapped `xtask-runner` bucket was then run directly and passed.
