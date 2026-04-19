# Autonomous Execution Report: Dependency upgrades, rusqlite + Tantivy 0.26

**Status:** Complete
**Plan:** docs/plans/2026-04-19-tantivy-0.26-implementation-plan.md
**Branch:** dep-audit-rusqlite-pilot
**PR:** https://github.com/anortham/julie/pull/10
**Duration:** 2h 20m
**Phases:** 5/5 complete
**Tasks:** 5/5 complete

## What shipped
- Captured the dependency audit findings and upgraded `rusqlite` to `0.39.0` with explicit SQLite integer conversions in query and migration paths.
- Upgraded `tantivy` to `0.26.0`, added `julie-search-compat.json`, and replaced the field-name-only compatibility check with schema and tokenizer signatures.
- Wired recreated-open search indexes to rebuild from canonical SQLite state through `SearchProjection` before they are treated as ready.
- Added a reusable search-quality comparison harness, a representative query fixture, a checked-in baseline snapshot, and a generated comparison report.

## Judgment calls (non-blocking decisions made)
- `src/search/index.rs:129` - Chose a Julie-owned `julie-search-compat.json` sidecar over Tantivy-open success alone because persisted-index safety depends on schema and tokenizer expectations Julie controls.
- `src/search/projection.rs:45` - Reused the existing projection state machine for recreated-open repair instead of adding a bespoke backfill path, because drift between two repair flows would be a maintenance trap.
- `src/tests/tools/search_quality/tantivy_upgrade_report.rs:72` - Classified zero top-N drift as `neutral` instead of `better` because the representative query set showed no measurable ranking gain, only no regression.
- `fixtures/search-quality/tantivy-upgrade-queries.json:35` - Swapped in a sentinel miss token for one OR-fallback query because the original query never exercised actual fallback on Julie's fixture data.

External review: none (not requested at approval time).

## Tests
- Post-commit verification passed: `cargo xtask test dogfood` and `cargo xtask test dev`.
- Earlier in the same branch, `cargo xtask test changed` passed.
- Exact tests passed for rusqlite registration and migration behavior, Tantivy compatibility-marker recreation, projection repair after recreated open, baseline-vs-current report generation, AND semantics, OR fallback, name-vs-body ranking, workspace tokenizer reopening, and tokenizer config wiring.

## Blockers hit
- None

## Files changed
```text
.memories/2026-04-19/201305_8818.md                |  70 +++
.memories/2026-04-19/203913_a3df.md                |  85 +++
.memories/2026-04-19/212729_007f.md                |  84 +++
.memories/2026-04-19/214700_b685.md                |  88 +++
.memories/2026-04-19/215933_46a1.md                |  89 +++
.memories/2026-04-19/221748_5934.md                |  93 +++
.memories/2026-04-19/223334_abbc.md                |  88 +++
Cargo.lock                                         | 251 +++++---
Cargo.toml                                         |   4 +-
docs/plans/2026-04-19-dependency-upgrade-audit-findings.md |  92 +++
docs/plans/2026-04-19-tantivy-0.26-baseline.json   | 633 +++++++++++++++++++++
docs/plans/2026-04-19-tantivy-0.26-comparison-report.md |  31 +
docs/plans/2026-04-19-tantivy-0.26-design.md       | 239 ++++++++
docs/plans/2026-04-19-tantivy-0.26-implementation-plan.md | 154 +++++
fixtures/search-quality/tantivy-upgrade-queries.json |  98 ++++
src/database/files.rs                              |   2 +
src/database/migrations.rs                         |  25 +-
src/database/symbols/search.rs                     |   4 +-
src/handler.rs                                     |  51 ++-
src/search/index.rs                                | 430 ++++++++++----
src/search/projection.rs                           |  25 ++
src/search/schema.rs                               |  30 ++
src/search/tokenizer.rs                            |  24 ++
src/tests/core/handler.rs                          |   8 +-
src/tests/integration/projection_repair.rs         |  62 ++++
src/tests/tools/search/tantivy_index_tests.rs      |  41 ++-
src/tests/tools/search_quality/comparison.rs       | 390 +++++++++++++
src/tests/tools/search_quality/helpers.rs          |  63 ++--
src/tests/tools/search_quality/mod.rs              |   4 +
src/tests/tools/search_quality/tantivy_upgrade_report.rs | 300 ++++++++++
src/tools/workspace/indexing/route.rs              |  27 +-
src/workspace/mod.rs                               |  28 +-
```

## Next steps
- Review PR: https://github.com/anortham/julie/pull/10
- Sanity-check the generated artifacts in the PR, especially `docs/plans/2026-04-19-tantivy-0.26-baseline.json` and `docs/plans/2026-04-19-tantivy-0.26-comparison-report.md`.
- Merge PR #10 and then remove the `dep-audit-rusqlite-pilot` branch and worktree.
