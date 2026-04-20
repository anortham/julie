# Search Analysis Workbench — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use razorback:executing-plans to implement this plan. Tasks are sequential (shared file prevents parallelism).

**Goal:** Rework `/search/analysis` from a misleading metrics dashboard into a search observability workbench with friction flags, observational summary cards, and a filterable trace table.

**Architecture:** Delete ~300 lines of aggregation code (canonical_key, aggregate_problems, extract_reformulation_pairs, and supporting types). Replace EpisodeStats headline fields with a new SearchSummary struct. Add compute_flags (per-episode friction signals) and compute_summary (observational stats). Rewrite templates from card feed to trace table.

**Tech Stack:** Rust (axum, serde, tera), HTML (tera templates)

**Design Spec:** `docs/plans/2026-04-20-search-analysis-workbench-design.md`

---

### Task 1: Backend — Delete Aggregation, Add Flags + Summary

**Files:**
- Modify: `src/dashboard/search_analysis.rs:51-60` (EpisodeStats — strip to compare-page-only fields)
- Modify: `src/dashboard/search_analysis.rs:118-144` (episode_stats fn — simplify)
- Delete: `src/dashboard/search_analysis.rs:275-579` (all aggregation code)
- Modify: `src/tests/dashboard/search_analysis.rs` (delete aggregation tests, add new tests)

**What to build:**

1. **Delete aggregation code** (lines 275-579): `FILLER_TOKENS`, `canonical_key`, `split_camel_case`, `QueryProblem`, `aggregate_problems`, `ProblemGroup` + impl, `ReformulationPair`, `extract_reformulation_pairs`, `ReformulationPairBuilder`, `pair_queries_overlap`, `format_relative_time`. Move `has_trace_data` (currently at line 554) up to just after `episode_stats`, before the deleted block.

2. **Strip EpisodeStats** to only what the compare page needs: keep `total_episodes`, `convergence_rate`, `stall_rate`. Remove `first_try_rate`, `one_shot_count`, `reformulation_count`, `stall_count`, `exploratory_count`. Simplify `episode_stats` accordingly.

3. **Add `SearchSummary` struct** (new, public):
   - `episode_count: usize`
   - `zero_hit_count: usize`
   - `zero_hit_rate: f64`
   - `median_top_score: Option<f32>`
   - `repeat_query_count: usize`
   - `repeat_query_rate: f64`

4. **Add `compute_summary(episodes: &[SearchEpisode]) -> SearchSummary`**: Count episodes. Count zero-hit episodes (all queries have `result_count == Some(0)` or `top_hit_name.is_none()`). Compute median of all `top_hit_score` values (collect into vec, sort, take middle). Count episodes where `queries_overlap` returns true. Compute rates as count / total.

5. **Add `compute_flags(episode: &mut SearchEpisode)`**: Compute friction flags and store them in a new `pub flags: Vec<String>` field on `SearchEpisode`. Flags:
   - `"zero_hits"` — any query has `result_count == Some(0)`
   - `"repeat_query"` — `queries_overlap(&episode.queries)` returns true (existing function)
   - `"low_score"` — any query has `top_hit_score < Some(5.0)`
   - `"no_follow_up"` — `episode.downstream_tool.is_none()`
   - `"relaxed"` — any query has `relaxed == Some(true)`

   Add `flags: Vec<String>` field to `SearchEpisode`, initialized to empty vec in `EpisodeBuilder::finish`. The `compute_flags` function mutates it in place.

6. **Update tests**: Delete all tests for `canonical_key`, `aggregate_problems`, `extract_reformulation_pairs` (lines 282-456 in test file). Remove those imports. Delete the `fast_search_row_with_hit` helper (only used by deleted tests). Remove `episode_stats` import if no longer tested (check). Add tests:
   - `test_compute_flags_zero_hits` — episode with `result_count: Some(0)` gets `"zero_hits"` flag
   - `test_compute_flags_no_follow_up` — episode with no downstream tool gets `"no_follow_up"` flag
   - `test_compute_flags_repeat_query` — episode with overlapping queries gets `"repeat_query"` flag
   - `test_compute_flags_low_score` — episode with `top_hit_score: Some(2.0)` gets `"low_score"` flag
   - `test_compute_flags_relaxed` — episode with `relaxed: Some(true)` gets `"relaxed"` flag
   - `test_compute_summary_zero_episodes` — empty input returns zeroed summary
   - `test_compute_summary_counts` — verify episode_count, zero_hit_count, repeat_query_count, median_top_score

**Approach:**
- `compute_flags` takes `&mut SearchEpisode` (mutates in place). The route calls it on each episode after `analyze_tool_calls`.
- For median score: collect all `Some` scores into a `Vec<f32>`, sort with `sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal))`, take middle element. Return `None` if empty.
- The `std::time` import can be removed after `format_relative_time` is deleted.
- The `std::collections::HashMap` import stays (used elsewhere? check — if not, remove).

**Acceptance criteria:**
- [ ] All aggregation code deleted (no `QueryProblem`, `ReformulationPair`, `canonical_key`, etc.)
- [ ] `EpisodeStats` has only `total_episodes`, `convergence_rate`, `stall_rate`
- [ ] `SearchSummary` struct with episode_count, zero_hit_count/rate, median_top_score, repeat_query_count/rate
- [ ] `compute_flags` populates flags vec on SearchEpisode
- [ ] `compute_summary` computes observational stats
- [ ] `SearchEpisode` has `flags: Vec<String>` field
- [ ] Aggregation tests deleted, flag and summary tests added
- [ ] Existing episode builder tests still pass
- [ ] Tests pass, committed

---

### Task 2: Route Update

**Files:**
- Modify: `src/dashboard/routes/search_analysis.rs`

**What to build:** Rewire the route to use the new functions and support flag filtering.

1. **Update imports**: Replace `aggregate_problems`, `extract_reformulation_pairs` with `compute_flags`, `compute_summary`. Keep `analyze_tool_calls`, `has_trace_data`.

2. **Add `flag` query param** to `SearchAnalysisParams`: `pub flag: Option<String>`.

3. **Rework handler logic**:
   - After building episodes and filtering pre-telemetry, call `compute_flags` on each episode (mutable iteration).
   - Call `compute_summary` on the flagged episodes.
   - If `flag` param is set, filter episodes to only those whose `flags` vec contains the param value.
   - Pass to template: `summary`, `episodes`, `active_flag` (the flag param or empty string), `window_label`, `window_param`, `show_all`, `total_episode_count`.

4. **Remove `days` from context** — replaced by `window_label`.

**Approach:**
- Flag filtering: `episodes.retain(|e| e.flags.contains(&flag_value))`. Apply after compute_summary so summary reflects unfiltered data.
- `active_flag` is used by the template to highlight which flag filter button is active.

**Acceptance criteria:**
- [ ] Route accepts `?flag=zero_hits` (or any flag name) and filters episodes
- [ ] Summary computed from all episodes (before flag filtering)
- [ ] No imports of deleted aggregation functions
- [ ] Template receives summary, episodes, active_flag
- [ ] Tests pass, committed

---

### Task 3: Templates

**Files:**
- Modify: `dashboard/templates/search_analysis.html` (full rewrite)
- Modify: `dashboard/templates/partials/search_episode_table.html` (full rewrite)

**What to build:** Replace the metrics dashboard layout with a trace viewer.

1. **search_analysis.html**:
   - Keep: header with title + Playground/Analysis/Compare nav buttons
   - Keep: time range selector (1h, 6h, 1d, 7d, 30d buttons)
   - **Summary cards** (replace old KPI row): Episodes count, Zero-Hit count with rate, Median Score, Repeat Query count with rate. Use `julie-card` wrapper, `label-text` + `mono` styling.
   - **Flag filter buttons**: A row of small buttons, one per flag (`zero_hits`, `repeat_query`, `low_score`, `no_follow_up`, `relaxed`), plus an "All" button. Active flag gets `is-primary` class. Links use `?flag=<name>&<window_param>`.
   - **Episode trace table**: `{% include "partials/search_episode_table.html" %}`
   - Remove: problem queries table, reformulation pairs table

2. **search_episode_table.html** (rewrite as trace table):
   - Show/all toggle at top (keep existing pattern with `window_param`)
   - Table with columns: Queries, Searches, Flags, Top Score, Results, Downstream, Workspace, Actions
   - **Queries column**: Show first query's text. If `episode.search_count > 1`, add a `<details>` element that expands to show all queries with per-query detail (query text, intent, search_target, top_hit_name, top_hit_file, score, result_count).
   - **Flags column**: Render each flag as a small tag. Color coding: `zero_hits` = `is-danger is-light`, `low_score` = `is-warning is-light`, `repeat_query` = `is-info is-light`, `no_follow_up` / `relaxed` = `is-dark`.
   - **Actions column**: "Compare" link to `/search/compare?query=<first_query_text>`
   - Empty state: "No episodes to show."

**Approach:**
- Follow existing dashboard table patterns from `dashboard/templates/partials/metrics_table.html` and `dashboard/templates/projects.html`.
- Use `<details><summary>` for expandable query rows (native HTML, no JS needed).
- URL-encode the query text in the Compare link using Tera's `urlencode` filter.
- Flag filter buttons should preserve the current `window_param` and `show_all` state.

**Acceptance criteria:**
- [ ] No "First-Try Success" card or outcome breakdown
- [ ] No problem queries table
- [ ] No reformulation pairs table
- [ ] Summary cards show: episode count, zero-hit count/rate, median score, repeat-query count/rate
- [ ] Flag filter buttons filter the episode table
- [ ] Episode table is tabular with correct columns
- [ ] Multi-query episodes expandable via `<details>`
- [ ] "Compare" action link on each row
- [ ] Page renders with zero episodes (empty state)
- [ ] Page renders with no trace data episodes (pre-telemetry filtered)
- [ ] Tests pass, committed
