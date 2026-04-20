# Autonomous Run Report: Search Analysis Diagnostic Rework

**Status:** Complete
**Branch:** `search-analysis-diagnostic-rework`
**Base:** `main`
**Plan:** `docs/plans/2026-04-20-search-analysis-diagnostic-rework-implementation-plan.md`
**Design:** `docs/plans/2026-04-20-search-analysis-diagnostic-rework-design.md`
**Date:** 2026-04-20

## What shipped

Reworked `/search/analysis` from a chronological episode feed into a diagnostic tool for improving search quality. The page now surfaces:

- **First-try success rate** as a headline metric (one_shot_success / total episodes)
- **Problem queries table** ranked by failure frequency, with canonical key grouping (handles CamelCase/snake_case/qualified name variants), recall-vs-ranking triage signal, avg scores, avg result counts
- **Reformulation pairs table** showing adjacent query transitions from reformulated episodes, deduplicated by canonical key
- **Filtered episode feed** defaulting to flagged-only (stalled + reformulated) with toggle for all

Additional fixes:
- Episode builder now splits on workspace_id change (cross-workspace searches no longer merge)
- SearchEpisodeQuery widened with 4 Option trace fields (score, result_count, strategy, relaxed) extracted from existing telemetry JSON
- EpisodeStats extended with outcome breakdown counts

## Commits

1. `69cf3c2d` feat(dashboard): widen search analysis data model and fix episode boundaries
2. `bb89cb2d` feat(dashboard): add search quality aggregation functions
3. `9f4a9af2` feat(dashboard): rework search analysis into diagnostic tool
4. `92c42994` fix(dashboard): address codex review findings

## Files changed

| File | Lines |
|------|-------|
| `src/dashboard/search_analysis.rs` | +333/-8 |
| `src/tests/dashboard/search_analysis.rs` | +362/-1 |
| `dashboard/templates/search_analysis.html` | +136/-7 |
| `dashboard/templates/partials/search_episode_table.html` | +19/-9 |
| `src/dashboard/routes/search_analysis.rs` | +21/-2 |

## Tests

- Dev tier: 10/10 buckets pass (272.9s)
- Search analysis unit tests: 22/22 pass
- Dashboard integration tests: all pass (including `/search/analysis` 200 check)

## External review

**Reviewer:** codex (gpt-5.4, xhigh reasoning)
**Verdict:** needs-attention (3 findings)
**Findings fixed:** 3/3

| # | Severity | Title | Classification | Action |
|---|----------|-------|---------------|--------|
| 1 | high | Triage treats any same-name hit as ranking win | real-improvement | Fixed: compare file path alongside symbol name |
| 2 | medium | Zero-result failures render as missing data | real-bug | Fixed: use Tera `is number` test instead of truthiness |
| 3 | medium | Reformulation pairs include unrelated hops | real-improvement | Fixed: filter to overlapping query pairs only |

Fix commit: `92c42994`

## Judgment calls

- Chose `is number` for Tera zero-value handling over server-side string formatting. Keeps the data model clean; Tera has native type tests.
- `fast_search_row_with_hit` test helper updated to use matching file path (`src/dashboard/routes/search.rs`) so triage test correctly exercises file-aware comparison.
- "explore one"/"explore two" test queries replaced with "database pool"/"centrality badge" because the originals shared the "explore" token, triggering false overlap detection.

## Blockers hit

None.

## Next steps

- Accumulate telemetry data over a few days, then review the analysis page with real data
- Consider adding a time-series trend for first-try success rate (requires storing historical snapshots)
- The canonical_key filler token list may need tuning based on real query patterns
