# Autonomous Run Report: Search Analysis Workbench

**Status:** Complete
**Branch:** `search-analysis-workbench`
**Base:** `main`
**Plan:** `docs/plans/2026-04-20-search-analysis-workbench-implementation-plan.md`
**Design:** `docs/plans/2026-04-20-search-analysis-workbench-design.md`
**Date:** 2026-04-20

## What shipped

Reworked `/search/analysis` from a misleading metrics dashboard into a search observability workbench. The previous page computed first-try success rate and problem queries from inferred episode classifications, but live testing proved the inferences were unreliable (a session where every search worked correctly showed 29% success).

The new page:
- **Summary cards**: episode count, zero-hit count/rate, median top-hit score, repeat-query count/rate (observational, not judgmental)
- **Friction flags**: per-episode signals (zero_hits, repeat_query, low_score, no_follow_up, relaxed) replacing success/failure classification
- **Flag filter buttons**: click a flag to see only episodes with that signal
- **Trace table**: expandable episode rows with per-query detail (query, intent, target, score, result count)
- **Aggregated row values**: best_score and min_result_count across all queries (not just first query)

Deleted ~300 lines of aggregation code (canonical_key, aggregate_problems, extract_reformulation_pairs, QueryProblem, ReformulationPair, and all supporting types).

## Commits

1. `3c02e0ed` refactor(dashboard): replace search analysis metrics with friction flags
2. `fab98226` feat(dashboard): rework search analysis into trace viewer
3. `19daed92` fix(dashboard): address codex review findings on trace viewer

## Files changed

| File | Lines |
|------|-------|
| `src/dashboard/search_analysis.rs` | -322/+97 (net -225) |
| `src/dashboard/routes/search_analysis.rs` | rewritten |
| `dashboard/templates/search_analysis.html` | rewritten |
| `dashboard/templates/partials/search_episode_table.html` | rewritten |
| `src/tests/dashboard/search_analysis.rs` | -179/+173 (replaced aggregation tests with flag/summary tests) |

## Tests

- Dev tier: 10/10 buckets pass (268s)
- Search analysis unit tests: 16/16 pass
- Dashboard integration: all pass

## External review

**Reviewer:** codex (gpt-5.4, xhigh reasoning)
**Verdict:** needs-attention (2 findings)
**Findings fixed:** 2/2

| # | Severity | Title | Classification | Action |
|---|----------|-------|---------------|--------|
| 1 | high | Compare button targets endpoint that ignores query param | real-bug | Fixed: removed dead button |
| 2 | medium | Collapsed rows show first query data when flags triggered by later queries | real-improvement | Fixed: added best_score/min_result_count aggregates |

## Blockers hit

None.

## Next steps

- Add GET query param support to the search playground so trace viewer can link "try this query"
- Add "Promote to dogfood case" action once the pipeline for adding test cases is designed
- Consider adding annotation support (good/bad/expected) for manual labeling
