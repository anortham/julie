# Autonomous Execution Report - Test Intelligence Foundation

**Status:** Complete
**Plan:** docs/plans/2026-04-24-test-intelligence-foundation.md
**Design:** docs/plans/2026-04-24-test-intelligence-foundation-design.md
**Branch:** feat/test-intelligence-foundation
**PR:** (pending creation)
**Phases:** 4/4 complete
**Tasks:** 8/8 complete

## What shipped
- **Session 1:** TestRole enum (5 variants), TestAnnotationClasses replacing flat test/fixture config, post-extraction classify_symbols_by_role pipeline step, is_scorable_test/is_test_related helpers
- **Session 2:** TestEvidenceConfig with framework-specific assertion identifiers, test_linkage updated to filter on scorable tests and carry confidence, test_quality rewritten with evidence model (identifier kind=Call + regex fallback)
- **Session 3:** Confidence-gated change_risk (test_weakness_score takes confidence from test_linkage), all server-side consumers updated to use is_test_related helper, deep_dive displays test_role + confidence
- **Session 4:** SchedulerSignal, EntryPointLinkageGap, HighCentralityLinkageGap signals added to early warning report, cache schema bumped to v2

## Judgment calls (non-blocking decisions made)
- `src/tools/workspace/indexing/pipeline.rs:72` - Used `LanguageConfigs::load_embedded()` per indexing run rather than caching on handler. Cheap (embedded strings) and avoids lifecycle complexity.
- `src/analysis/early_warnings.rs:23` - Changed `EarlyWarningReport` from `PartialEq + Eq` to `PartialEq` only because `HighCentralityLinkageGap` contains `f64`. Necessary for compilation.
- `src/analysis/early_warnings.rs:560` - Over-fetch by 4x for high-centrality gaps when file_pattern is set, then filter in Rust. Avoids complex SQL LIKE patterns.
- `src/analysis/test_linkage.rs:103` - Used string interpolation for SQL scorable test filter rather than parameterized query. Safe because the filter is a compile-time constant, not user input.

## External review (codex, adversarial)

- **Findings:** 5
- **Verified real, fixed:** 3 (commit: 835bbdf6)
  - `has_identifier_evidence` too broad: languages with empty test_evidence entered identifier path with 0 matches, producing false high-confidence Stub. Fixed: require non-empty assertion_identifiers.
  - Substring matching in `count_identifier_evidence`: `lower.contains()` matched "assertion_report" against "assert". Fixed: exact match only.
  - Parent linkage aggregation missing `best_confidence`: parents got test_linkage without confidence, defaulting to 0.5. Fixed: added to SQL query, aggregation, and output.
- **Dismissed:** 2
  - Non-call identifiers in test linkage (high) - Out of scope. Pre-existing behavior in identifier linkage queries, not introduced by this branch. The plan modified only the test filter, not the identifier kind filter.
  - Stale test_linkage metadata suppressing gap signals (medium) - Out of scope. Migration concern for existing workspaces. New code is correct for fresh indexing runs; versioning legacy metadata is future work.
- **Flagged for your review:** 0

## Tests
- Dev tier: 6/7 buckets pass. tools-misc bucket times out (210s vs 60s expected) on pre-existing editing tests (all 87 individual tests pass, bucket calibration is too low). Not a regression from this branch.
- All new tests: 110+ tests added across test_roles (27), test_quality (65+), test_linkage (18), change_risk (13), early_warnings (13)

## Blockers hit
- None

## Files changed
```
 .gitignore                                       |   2 +
 crates/julie-extractors/src/base/mod.rs          |   2 +-
 crates/julie-extractors/src/base/types.rs        |  31 +
 crates/julie-extractors/src/lib.rs               |   2 +-
 languages/csharp.toml                            |  14 +-
 languages/java.toml                              |  14 +-
 languages/kotlin.toml                            |  14 +-
 languages/python.toml                            |  14 +-
 languages/rust.toml                              |   9 +-
 src/analysis/change_risk.rs                      |  34 +-
 src/analysis/early_warnings.rs                   | 162 ++++-
 src/analysis/mod.rs                              |  10 +-
 src/analysis/test_linkage.rs                     |  88 ++-
 src/analysis/test_quality.rs                     | 507 +++++++++---
 src/analysis/test_roles.rs                       | 179 +++++
 src/cli_tools/output.rs                          |  76 +-
 src/embeddings/metadata.rs                       |  15 +-
 src/search/language_config.rs                    | 313 +++++++-
 src/tests/analysis/change_risk_tests.rs          |  49 +-
 src/tests/analysis/early_warning_report_tests.rs | 251 +++++-
 src/tests/analysis/mod.rs                        |   2 +
 src/tests/analysis/test_linkage_tests.rs         | 145 ++++
 src/tests/analysis/test_quality_tests.rs         | 789 ++++++++++++------
 src/tests/analysis/test_roles_tests.rs           | 468 +++++++++++
 src/tests/core/early_warning_report_cache.rs     |   2 +-
 src/tests/tools/deep_dive_tests.rs               |  49 +-
 src/tools/deep_dive/data.rs                      |   8 +-
 src/tools/deep_dive/formatting.rs                |  49 +-
 src/tools/impact/mod.rs                          |   8 +-
 src/tools/impact/ranking.rs                      |  10 +-
 src/tools/workspace/indexing/pipeline.rs         |  16 +-
 33 files changed (+memories), ~3500 insertions, ~440 deletions
```

## Next steps
- Review PR
- The dismissed finding about non-call identifiers in test_linkage is worth considering for a future pass (filtering identifier linkage on kind='Call' for coverage computation)
- The stale-metadata concern for existing workspaces could be addressed by versioning test_linkage metadata or adding a recompute trigger on schema changes
- tools-misc bucket timeout needs calibration fix (bump expected time from 60s to 240s)
