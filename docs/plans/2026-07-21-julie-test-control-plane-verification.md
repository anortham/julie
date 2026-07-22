# Julie Test Control Plane — Verification

The integrated target-aware runner was recalibrated on the final review working tree at `af929c558881bd08a55e3aea96d3721348d14b97`.

## Timing contract

- Warm wall is the sum of selected bucket command elapsed times after every Rust test target selected by those buckets has completed its target-specific prebuild.
- Prebuild is the summed elapsed time of deterministic, de-duplicated `--no-run` commands derived from selected `cargo nextest run` and `cargo test` package/target selectors.
- Non-test commands are not prebuilt and remain part of warm bucket execution.
- Cold wall is `prebuild + warm`; the declared fast budget applies to manifest `expected_seconds`, not cold compilation time.

## Current fast recalibration

Three complete current-membership runs were measured after all review-remediation tasks were integrated. The p95 uses linear interpolation over the three observed warm values. Superseded root-only-prebuild samples were not reused.

| Bucket or tier | Warm runs (s) | p50 | p95 | Current expected | Decision | Status |
|---|---|---|---|---|---|---|
| Final `fast` membership | 23.6, 22.9, 22.7 | 22.9 | 23.5 | 31.0 | Keep current membership; observed warm wall remains below the declared sum and 60-second fast budget. | pass |

## Cold sample

The first integrated sample reported `SUMMARY: 2 buckets passed in 23.6s (warm)`, `PREBUILD: 1.5s`, and `COLD WALL: 25.1s`.

## Verification ledger

Record commands only when they have run against the final integrated tree. Use the exact current commit SHA or explicitly label working-tree evidence; do not substitute task names or planned timestamps.

| Invariant | Command | Scope label | Commit or tree state | Result | Timestamp (UTC) | Evidence reused |
|---|---|---|---|---|---|---|
| Target-aware runner exact tests | `cargo nextest run -p xtask` plus exact runner/coverage regressions | worker-exact | working tree at `af929c558881bd08a55e3aea96d3721348d14b97` | 176/176 passed after the stale dispatch assertion was corrected | 2026-07-22T00:02:00Z | no |
| Final fast timing sample | `CARGO_INCREMENTAL=0 cargo xtask test fast` (three complete runs) | measurement-fast | working tree at `af929c558881bd08a55e3aea96d3721348d14b97` | warm 23.6s, 22.9s, 22.7s; p50 22.9s; p95 23.5s | 2026-07-22T00:21:00Z | no |
| Broad changed/dev/dogfood gate | `CARGO_INCREMENTAL=0 cargo xtask test changed` | affected-change | working tree at `af929c558881bd08a55e3aea96d3721348d14b97` | 41 buckets passed; 1067.4s warm; 4.4s prebuild | 2026-07-22T00:19:00Z | no |
| Startup and indexing system gate | `CARGO_INCREMENTAL=0 cargo xtask test system` | system | working tree at `af929c558881bd08a55e3aea96d3721348d14b97` | 5 buckets passed; 90.7s warm; 1.0s prebuild | 2026-07-22T00:22:55Z | no |

The macOS linker emitted the existing object-version warning because Rust and native dependency objects target macOS 26.x while the project deployment target is 11.0.0. It did not fail compilation or any test gate.
