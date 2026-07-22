# Xtask Changed-Selection Module Boundary Verification

Implementation commit: `e07b0bf5e0aa9b1ecc61518af5a8bc7f51b3b0d1`

The TDD rows were run while the implementation was still a dirty diff over
`a2bd0df7eedd43efafdb5f5722763f5fe64db3a7`; their timestamp is the checkpoint
capture time immediately after the loop. All lead hard gates were rerun at the
exact implementation commit.

## Verification Ledger

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| Boundary test rejects the 1,842-line monolith before implementation | `cargo nextest run -p xtask --test changed_boundary_tests changed_implementation_files_stay_within_limit 2>&1 \| tail -20` | worker-exact-red | `a2bd0df7eedd43efafdb5f5722763f5fe64db3a7` + dirty test | pass (expected failure: 1,842 > 500) | 2026-07-22T13:12:54Z | no |
| Boundary test accepts the split implementation | `cargo nextest run -p xtask --test changed_boundary_tests changed_implementation_files_stay_within_limit 2>&1 \| tail -20` | worker-exact-green | `a2bd0df7eedd43efafdb5f5722763f5fe64db3a7` + dirty implementation | pass (1/1) | 2026-07-22T13:12:54Z | no |
| Dirty implementation diff maps only to the xtask runner | `cargo xtask test changed` | lead-changed | `a2bd0df7eedd43efafdb5f5722763f5fe64db3a7` + dirty implementation | pass (xtask-runner) | 2026-07-22T13:12:54Z | no |
| Public callers compile unchanged | `cargo check -p xtask` | lead-compile | `e07b0bf5e0aa9b1ecc61518af5a8bc7f51b3b0d1` | pass | 2026-07-22T13:14:22Z | no |
| Existing changed-selection behavior remains unchanged | `cargo nextest run -p xtask --test changed_tests` | lead-focused | `e07b0bf5e0aa9b1ecc61518af5a8bc7f51b3b0d1` | pass (43/43) | 2026-07-22T13:14:22Z | no |
| Full xtask test target passes, including the new boundary test | `cargo nextest run -p xtask` | lead-xtask | `e07b0bf5e0aa9b1ecc61518af5a8bc7f51b3b0d1` | pass (179/179) | 2026-07-22T13:14:22Z | no |
| Xtask formatting remains canonical | `cargo fmt -p xtask -- --check` | lead-format | `e07b0bf5e0aa9b1ecc61518af5a8bc7f51b3b0d1` | pass | 2026-07-22T13:14:30Z | no |
| Production files stay below 500 lines, tests stay below 1,000, public names remain exported, and forbidden caller files are unchanged | `wc -l xtask/src/changed.rs xtask/src/changed/*.rs xtask/src/changed/mapping/*.rs xtask/tests/changed_boundary_tests.rs && git diff a2bd0df7..HEAD --name-only -- xtask/src/main.rs xtask/src/lib.rs xtask/tests/changed_tests.rs && rg -n '^pub (enum\|struct\|fn\|use)' xtask/src/changed.rs xtask/src/changed/{diff,policy,rendering}.rs && git diff --check` | lead-structure | `e07b0bf5e0aa9b1ecc61518af5a8bc7f51b3b0d1` | pass (largest production file: 415; tests: 457; forbidden diff empty) | 2026-07-22T13:14:30Z | no |
| Canonical xtask-runner bucket passes directly | `cargo xtask test bucket xtask-runner` | lead-bucket | `e07b0bf5e0aa9b1ecc61518af5a8bc7f51b3b0d1` | pass | 2026-07-22T13:14:34Z | no |
| Batch-level branch gate passes | `cargo xtask test dev` | lead-branch | `e07b0bf5e0aa9b1ecc61518af5a8bc7f51b3b0d1` | pass (27/27 buckets) | 2026-07-22T13:15:11Z | no |
