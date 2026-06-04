# Julie Rescue Phase 1 — `julie-index` Extraction Results

**Branch:** `julie-rescue` · **HEAD:** `23f2e222` · **Date:** 2026-06-03
**Plan:** `docs/plans/2026-06-03-julie-index-extraction-plan.md`
**Design:** `docs/plans/2026-06-03-julie-index-extraction-design.md`

## Summary

Extracted a new `julie-index` crate (search + analysis **merged**, because they
cross-reference each other and cannot be split) one layer above the `julie-core`
leaf crate from Phase 0. All of `src/search/**` and `src/analysis/**` moved down
into the crate; the top crate re-exports them via `pub use` shims so every
path-qualified caller compiles unchanged. The 344 decoupled search/analysis
tests now live in julie-index's own test binary — **the relink payoff**: editing
one of those tests no longer relinks the 126k-LOC monolith.

Crate DAG now: `julie-core` (leaf) → **`julie-index`** → top `julie` crate.

## What shipped

- **New crate** `crates/julie-index/` — `search` + `analysis` modules merged, owns
  the embedded `languages/` asset dir (the `include_str!` paths resolve in-crate).
- **One upward edge severed** in T1: the glob cluster (`matches_glob_pattern` + 4
  private helpers) moved down to `julie_core::glob`, shimmed at the old tools path.
- **Downward repoints** (compiler-driven, in 12+ files): `crate::database` →
  `julie_core::database`, `crate::embeddings::EmbeddingProvider` →
  `julie_core::embeddings_contract`, `crate::extractors` → `julie_extractors`,
  `crate::tests::helpers::db` → `julie_test_support`. search↔analysis cross-refs
  now resolve internally to julie-index.
- **Test relocation** (T3): 37 test files (10 analysis + 27 search incl. subdirs)
  moved into `crates/julie-index/src/tests/**`; 535 test functions preserved
  exactly (191 retained top-crate + 344 relocated = 535, zero dropped).
- **Cross-crate test surface** (ADR-0006 pattern): a `test-support` feature on
  julie-index gates the helper methods (`symbol_from_parts`, `search_content`,
  etc.) that retained top-crate tests call across the crate boundary; the top
  crate enables it via a feature-flagged dev-dependency. No two-rlib mismatch.
- **Minimal pub surface**: only 4 `pub(crate)` items promoted to `pub` (the ones
  retained coupled tests reference cross-crate). 14 reranker constants + `kind_boost`
  stayed `pub(crate)` because their tests relocated in-crate.
- **Dep-direction tripwire** `crates/julie-index/tests/no_upward_deps.rs` — fails
  if julie-index ever references a top-crate module (`crate::{tools,handler,daemon,
  watcher,workspace,cli,indexing_core,embeddings}`) or grows a cyclic manifest dep.
  Spike-verified (fails on a planted ref, passes after removal).
- **xtask routing**: new `core-index` bucket (`cargo nextest run -p julie-index`),
  added to dev + full tiers; `crates/julie-index/src/**` edits route to core-index
  (+ retained search/analysis buckets), with the `manifest_contract_expected.rs`
  mirror + `changed_tests.rs` representative-path tests in lockstep.

## Verification Ledger

| Scope | Invariant | Command | Commit | Result | Time |
|-------|-----------|---------|--------|--------|------|
| relink-cure | Editing a relocated julie-index test relinks only julie-index | `cargo nextest run -p julie-index --no-run` (after touching a relocated test) | 23f2e222 | **2.95s** real (1.42s compile) | — |
| relink-cure | Monolith test binary is insulated from a julie-index test edit | `cargo nextest run -p julie --no-run` (after touching a julie-index test) | 23f2e222 | **0.53s** real, no-op (no recompile) | — |
| relink-cure (baseline) | Old tax: editing a monolith test relinks the 126k-LOC binary | `cargo nextest run -p julie --no-run` (after touching a top-crate test) | 23f2e222 | **9.46s** real (6.34s compile) | — |
| branch-gate | Dev tier green | `cargo xtask test dev` | 23f2e222 | **35 buckets passed** | 1025.8s |
| branch-gate | System tier green | `cargo xtask test system` | 23f2e222 | **7 buckets passed** | 193.1s |
| affected-change | xtask routing + manifest mirror + tripwire agree | `cargo nextest run -p xtask` | 23f2e222 | 164/164 passed | 9.95s |
| affected-change | julie-index's own test binary green | `cargo nextest run -p julie-index` | 19997d9c | 373/373 passed, 1 skipped | — |
| smoke | search resolves the relocated glob + shims + julie-index consumers | `julie-server search "matches_glob_pattern" --target definitions` | 23f2e222 | PASS (glob.rs:24 + shims + julie-index uses) | — |
| smoke | get_symbols on a relocated julie-index file | `julie-server symbols crates/julie-index/src/search/scoring.rs` | 23f2e222 | PASS (34 symbols) | — |
| smoke | deep_dive resolves a promoted pub fn + cross-crate callers | `julie-server tool deep_dive --params '{"symbol":"apply_reranker_to_content_results"}'` | 23f2e222 | PASS (public @ index.rs:2185, 5 callers incl. retained test) | — |

### Relink-cure headline

Editing a search/analysis **test** now costs **2.95s instead of 9.46s — a 3.2×
faster relink loop** for the 344 relocated tests, and the monolith test binary is
**fully insulated** (0.53s no-op) from those edits.

## Lessons

- **Stale `--lib` bucket filters are a hidden cost of test relocation.** Moving a
  test module out of the top crate orphans any `test_tiers.toml` bucket command
  that filters `--lib` by that module path — `nextest` exits 4 ("no tests to run")
  and the bucket fails. This bit us twice: the `tests::tools::search::*` /
  `tests::analysis::*` integration filters (T6) **and** the `search::*` source-unit
  filters that moved back in T2 (T6 follow-up). **Lesson for the next increment:**
  after any test relocation, run a comprehensive 0-match scan over *every* `--lib`
  filter (`cargo nextest list -p julie --lib` vs each filter), not just the
  obviously-related ones, before declaring the branch gate.
- **`cargo check` never compiles `#[cfg(test)]` modules.** T2's worker gates were
  all `cargo check`-based and reported green while the top-crate test tree had 134
  errors. The crate-extraction acceptance gate must be
  `cargo nextest run -p julie --no-run`, not `cargo check`.
- **Test-helper methods can't ride the thin re-export crate.** julie-core's
  `test_support` re-exports *free functions*; julie-index's helpers are *methods on
  types*, so they need the `test-support` feature enabled on a dev-dependency, not
  a re-export shim. Empirically this produced no two-rlib mismatch under resolver 2
  (the dev-dep + normal-dep features union to one rlib in the test build).

## Commits

```
a45781e0 docs(plan): julie-index extraction design
461cbd1b docs(plan): revise spec per Codex review
6f859383 docs(plan): julie-index extraction implementation plan
a182b08c refactor(core): relocate glob cluster to julie_core::glob (T1)
ea1e7510 refactor(core): extract julie-index crate — search+analysis merged (T2)
3921b0e8 test(core): dep-direction tripwire + xtask routing for julie-index (T4)
19997d9c refactor(julie-index): relocate decoupled tests into julie-index binary (T3)
d88c83ca refactor(xtask): prune stale top-crate bucket refs after T3 relocation (T6)
23f2e222 fix(xtask): prune stale search::query_parse::tests bucket ref (T6 follow-up)
```

126 files changed, 1403 insertions(+), 363 deletions(-) since `main` (e3735abe).

## Next

1. **julie-extractors 2.1 upgrade** (separate, deliberately out of this slice):
   re-pin the git dep in `Cargo.toml` + sync `SEMANTIC_INDEX_ENGINE_VERSION`
   (`src/tools/workspace/indexing/engine_version.rs`) to 2.1's
   `EXTRACTION_CONTRACT_VERSION`, then `cargo xtask test bucket parser-upgrade`.
2. **Later increments**: julie-pipeline → julie-tools → julie-runtime → julie-daemon,
   each peeling another layer off the monolith test binary.
