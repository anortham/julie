# Autonomous Run Report — Julie Rescue Phase 1: julie-index Extraction

**Status:** ✅ Complete
**Date:** 2026-06-03
**Branch:** `julie-rescue` → PR [#23](https://github.com/anortham/julie/pull/23) (base `main`)
**HEAD:** `36071956`
**Plan:** `docs/plans/2026-06-03-julie-index-extraction-plan.md`
**Results:** `docs/plans/2026-06-03-julie-index-extraction-results.md`

## Outcome

Phases: 1/1 · Tasks: 6/6 complete (T1–T6; T5 lead-owned, T6 emerged from the dev gate).
Extracted `julie-index` (search + analysis merged) above the Phase 0 `julie-core` leaf crate via the re-export-shim playbook. The 344 decoupled search/analysis tests now relink in julie-index's own binary instead of the 126k-LOC monolith.

## What shipped

| Task | Commit | Result |
|------|--------|--------|
| T1 glob → `julie_core::glob` | `a182b08c` | severed the one upward edge; shimmed |
| T2 extract `julie-index` (search+analysis merged) | `ea1e7510` | crate builds standalone; `languages/` moved; 0 upward refs |
| T3 relocate decoupled tests + test-support surface | `19997d9c` | 37 files / 344 tests moved; 535 fns preserved; 4 pub promotions |
| T4 dep tripwire + xtask routing | `3921b0e8` | `core-index` bucket; tripwire spike-verified |
| T6 prune stale bucket refs | `d88c83ca` | 9 orphaned `--lib` filters + dead analysis bucket removed |
| T6 follow-up (lead) | `23f2e222` | last stale filter `search::query_parse::tests` pruned |
| T5 results doc + checkpoint (lead) | `36071956` | ledger + relink-cure proof + goldfish checkpoint |

## Relink cure (headline)

| Scenario | Command | real |
|----------|---------|------|
| Cured — edit a relocated julie-index test | `nextest -p julie-index --no-run` | **2.95s** |
| Monolith insulated from that edit | `nextest -p julie --no-run` | **0.53s** (no-op) |
| Old tax — edit a monolith test | `nextest -p julie --no-run` | **9.46s** |

**3.2× faster relink** for the 344 relocated tests; monolith fully insulated.

## External review

reviewer=none (no external pre-merge review for this slice, per plan handoff).

## Tests

- **Branch gate** (`23f2e222`): `cargo xtask test dev` → 35 buckets / 1025.8s; `cargo xtask test system` → 7 buckets / 193.1s. Evidence carries to HEAD `36071956` (only delta = 2 markdown files, zero code).
- `cargo nextest run -p julie-index` → 373/373 + 1 skipped.
- `cargo nextest run -p xtask` → 164/164 (manifest mirror + changed_tests lockstep).
- `cargo nextest run -p julie --no-run` → green (the crate-extraction gate that `cargo check` could not provide).
- No tests dropped: 535 fns parent → 535 (191 top-crate + 344 julie-index).
- Live smoke (debug binary): `search` (glob relocation + shims resolve), `symbols` (relocated scoring.rs, 34 symbols), `deep_dive` (promoted pub fn + cross-crate callers) all PASS.

## Judgment calls

1. **T2 marked complete despite a red top-crate test tree.** The worker's `cargo check` gates never compiled `#[cfg(test)]` modules; I found 134 errors with `nextest --no-run`. Diagnosed as inherent to a mid-split crate (cfg(test) helpers don't cross crate boundaries; pub(crate) items referenced cross-crate; include_str! at moved paths) — not a T2 redo. T2's library-level deliverable was correct; the test-tree restoration was folded into an expanded T3. The minimal pub-promotion set is only knowable post-relocation, which justified the sequencing.
2. **ADR-0006 pattern for test helpers.** julie-index's helpers are *methods on types*, so the thin re-export crate (which works for julie-core's *free functions*) doesn't apply. Used a `test-support` feature enabled via feature-flagged dev-dependency. Empirically no two-rlib mismatch under resolver 2.
3. **Stale-bucket prune dispatched, then a second round done inline.** First scan (T6) only covered `tests::*` filters from T3's relocation; the dev gate then surfaced a `search::*` source-unit filter orphaned back in T2. Lead fixed the last one inline after a comprehensive 0-match scan over *every* `--lib` filter.
4. **Removed the `analysis` bucket entirely** rather than repoint it — its coverage is fully in `core-index`; keeping it would double-run analysis tests in the dev tier.
5. **Kept the pre-existing `OsString` unused-import warning** (xtask/src/process.rs, from `836d2c9b`). It's a platform-conditional false-positive on macOS (real usage on Windows); fixing needs untested cfg-gating, out of scope for this slice.

## Blockers hit

None unresolved. The dev gate caught two rounds of stale `--lib` bucket filters (lockstep gap from test relocation); both fixed and re-verified green.

## Files changed

126 files changed, 1403 insertions(+), 363 deletions(-) since `main` (`e3735abe`).

## Next steps

1. **julie-extractors 2.1 upgrade** (separate task): re-pin git dep in `Cargo.toml` + sync `SEMANTIC_INDEX_ENGINE_VERSION` (`src/tools/workspace/indexing/engine_version.rs`) to 2.1's `EXTRACTION_CONTRACT_VERSION`, then `cargo xtask test bucket parser-upgrade`.
2. **Later rescue increments**: julie-pipeline → julie-tools → julie-runtime → julie-daemon.
3. PR #23 awaits human review/merge (autonomous mode never merges).
