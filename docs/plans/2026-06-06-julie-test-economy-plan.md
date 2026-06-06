# Julie Test Economy Rescue Plan

**Date:** 2026-06-06
**Status:** Active; first dev-tier cut implemented
**Baseline:** `b5ed8ef8` plus this working-tree slice

---

## Goal

Make normal Julie development cheap enough for agents to use correctly.

The target is simple: `cargo xtask test dev` must stay under 10 minutes expected runtime, while `cargo xtask test full` keeps the broad release coverage. This fixes the practical "30 minute test suite" problem without deleting the coverage that protects Julie's tool behavior.

---

## Current Facts

From the current manifest after the first cut:

| Tier | Buckets | Expected Runtime |
| --- | ---: | ---: |
| `nano` | 2 | 65s / 1.1m |
| `smoke` | 4 | 80s / 1.3m |
| `dev` | 27 | 589s / 9.8m |
| `system` | 5 | 225s / 3.8m |
| `dogfood` | 2 | 380s / 6.3m |
| `full` | 44 | 2519s / 42.0m |

`dev` was 37 buckets / 1914s before this slice. The removed broad buckets are still present in `full`.

The first actual `cargo xtask test dev` run after calibration passed 27 buckets in 389.7s. A first attempt exposed an undersized `core-database` timeout: the package compile took 27s under a 30s bucket timeout. The timeout is now 90s; the expected runtime remains 5s, so this does not change the 600s `dev` cap.

Broad buckets removed from `dev`:

- `tools-workspace`
- `tools-search-line`
- `tools-search-file-mode`
- `tools-search-format-quality`
- `tools-search-unified`
- `tools-workspace-targeting`
- `tools-editing`
- `tools-call-path`
- `tools-refactoring`
- `extractor-dep-integration`

---

## Implementation Plan

### 1. Lock The New Tier Contract

Done in this slice:

- Add an xtask manifest test that fails when `dev` exceeds 600 expected seconds.
- Add a companion manifest test proving the broad buckets removed from `dev` remain in `full`.
- Reclassify `dev` as the fast branch gate and `full` as the broad pre-merge gate in agent docs.

### 2. Split The Slow Broad Buckets

Next target order:

1. `tools-workspace` (300s, 9 commands)
2. `tools-search-line` (250s, 1 serialized command)
3. `tools-editing` (200s, 8 commands)
4. `tools-workspace-targeting` (170s, 2 commands)
5. `tools-search-format-quality` (100s, 6 commands)
6. `tools-call-path` (80s, 2 commands)

For each bucket:

- Split by command ownership or fixture shape, not by arbitrary runtime guesses.
- Update `xtask/src/changed.rs` so localized file edits select the smallest new bucket.
- Keep the original broad coverage in `full` until the split buckets are verified.
- Re-admit only cheap representative slices into `dev` when the 600s contract still passes.

### 3. Move Handler-Free Tests Down

For each slow top-crate bucket, classify commands as:

- **Handler-bound:** must stay in the top crate because it exercises `JulieServerHandler`, workspace rebinding, registry/session behavior, or MCP-facing output.
- **Handler-free:** should move to `julie-tools`, `julie-runtime`, or another lower crate so it no longer rebuilds the top crate.

Do not move tests just to move them. Move them only when the dependency direction becomes cleaner and the narrow command gets cheaper.

### 4. Retire Stale Runtime Vocabulary

After test ownership is clearer:

- Rename the `daemon` bucket to `registry-runtime` or equivalent.
- Update `PROGRAM_TIERS`, changed-path sort order, docs contract tests, and agent docs together.
- Leave `DaemonDatabase` / `src/daemon` module cleanup for the later complexity slice unless the bucket rename exposes a low-risk wrapper.

---

## Acceptance Criteria

- `cargo xtask test dev` reports 600s or less expected runtime from the manifest.
- `cargo xtask test full` still includes every broad bucket removed from `dev`.
- `cargo xtask test changed` no longer turns common localized edits into a 30-minute default gate; when it falls back, it falls back to the fast `dev` tier.
- Slow bucket splits have changed-path routing tests.
- No user-facing tool behavior is changed by tier reclassification.

---

## Verification Ledger

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| Dev tier cap test proves the old manifest was over budget before the cut | `cargo nextest run -p xtask manifest_tests_dev_tier_stays_under_ten_minutes 2>&1 \| tail -40` | targeted-red | `b5ed8ef8 + dirty test-only` | fail as expected: 1914s across 37 buckets | 2026-06-06T14:08:38Z | no |
| Dev tier cap and full-retention contracts pass after the manifest cut | `cargo nextest run -p xtask manifest_tests_dev_tier_stays_under_ten_minutes manifest_tests_full_retains_broad_release_buckets 2>&1 \| tail -60` | targeted-xtask | `b5ed8ef8 + dirty` | pass: 2/2 tests | 2026-06-06T14:08:38Z | no |
| Manifest listing shows the new fast dev tier and preserved full tier | `cargo xtask test list \| sed -n '1,90p'` | manifest-list | `b5ed8ef8 + dirty` | pass: `dev` lists 27 buckets; `full` lists 44 buckets | 2026-06-06T14:08:38Z | no |
| Full xtask package passes after updating tier contracts and approved fixtures | `cargo nextest run -p xtask` | xtask-package | `b5ed8ef8 + dirty` | pass: 168/168 tests | 2026-06-06T14:18:03Z | no |
| Changed-path gate selects only xtask-runner for this slice and passes | `XTASK_CHANGED_PATHS=$'AGENTS.md\nCLAUDE.md\nxtask/test_tiers.toml\nxtask/src/manifest.rs\nxtask/src/changed.rs\nxtask/tests/manifest_contract_tests.rs\nxtask/tests/support/manifest_contract_expected.rs\ndocs/plans/2026-06-06-julie-rescue-current-status.md\ndocs/plans/2026-06-06-julie-test-economy-plan.md' cargo xtask test changed` | scoped-changed | `5b6ec712 + dirty timeout fix` | pass: `xtask-runner` 2.4s | 2026-06-06T14:30:00Z | no |
| First actual dev run exposed undersized core-database timeout | `cargo xtask test dev` | dev-calibration | `5b6ec712 + dirty timeout fix` | fail: `core-database` timed out at 30s after 27s compile | 2026-06-06T14:28:45Z | no |
| Core database bucket passes with calibrated timeout | `cargo xtask test bucket core-database` | bucket-calibration | `5b6ec712 + dirty timeout fix` | pass: `core-database` 6.3s | 2026-06-06T14:28:45Z | no |
| Fast dev gate passes after timeout calibration | `cargo xtask test dev` | dev-fast-gate | `5b6ec712 + dirty timeout fix` | pass: 27 buckets in 389.7s | 2026-06-06T14:28:45Z | no |
