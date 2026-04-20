# Search Analysis Diagnostic Rework — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use razorback:executing-plans to implement this plan. Tasks are sequential (shared file prevents parallelism).

**Goal:** Rework `/search/analysis` from a chronological episode feed into a diagnostic tool that surfaces first-try success rate, problem queries, and reformulation pairs.

**Architecture:** No new DB queries or routes. Widen the data model to carry trace fields through, add aggregation functions (canonical key grouping, adjacent pair extraction, recall-vs-ranking triage), and rework the template into 4 diagnostic sections.

**Tech Stack:** Rust (axum, serde, tera), HTML (tera templates)

**Design Spec:** `docs/plans/2026-04-20-search-analysis-diagnostic-rework-design.md`

---

### Task 1: Data Model + Episode Builder Fix

**Files:**
- Modify: `src/dashboard/search_analysis.rs:18-26` (SearchEpisodeQuery)
- Modify: `src/dashboard/search_analysis.rs:44-48` (EpisodeStats)
- Modify: `src/dashboard/search_analysis.rs:59-61` (should_start_new in analyze_tool_calls)
- Modify: `src/dashboard/search_analysis.rs:103-119` (episode_stats fn)
- Modify: `src/dashboard/search_analysis.rs:188-207` (parse_search_query)
- Modify: `src/tests/dashboard/search_analysis.rs:4-35` (fast_search_row helper)
- Modify: `src/tests/dashboard/search_analysis.rs` (add new tests)

**What to build:** Foundation changes that all subsequent tasks depend on. Three things:

1. Widen `SearchEpisodeQuery` with 4 new `Option` fields: `top_hit_score: Option<f32>`, `result_count: Option<usize>`, `strategy: Option<String>`, `relaxed: Option<bool>`. Update `parse_search_query` to extract these from the trace JSON that's already being parsed (lines 188-207). The trace can be null for pre-telemetry calls, so all fields must gracefully handle missing data.

2. Add `workspace_id` boundary to `analyze_tool_calls`. In the `should_start_new` condition (line 59-61), add `|| episode.workspace_id != row.workspace_id`. Also add the same check in the non-search branch (line 75) where `session_id` is already checked.

3. Extend `EpisodeStats` with `first_try_rate: f64`, `one_shot_count: usize`, `reformulation_count: usize`, `stall_count: usize`, `exploratory_count: usize`. Update `episode_stats` fn to compute these from episode outcomes. Keep existing `convergence_rate` and `stall_rate`.

**Approach:**
- The `parse_search_query` function already accesses `trace["top_hits"][0]["name"]`. Add sibling extractions for `trace["top_hits"][0]["score"]`, `trace["result_count"]`, `trace["strategy"]`, `trace["relaxed"]`. Use `.as_f64().map(|v| v as f32)` for score, `.as_u64().map(|v| v as usize)` for result_count, `.as_str().map(ToOwned::to_owned)` for strategy, `.as_bool()` for relaxed.
- Update the `fast_search_row` test helper to include `score`, `result_count`, `strategy`, `relaxed` in its trace JSON. Add a second helper `fast_search_row_no_trace` that sets `metadata` to a JSON blob with no `trace` key (for null-trace tests).
- Outcome counting in `episode_stats`: iterate episodes, match on `outcome.as_str()` for "one_shot_success", "reformulation_converged", "stalled", and count "exploratory_success" as the remainder.

**Acceptance criteria:**
- [ ] `SearchEpisodeQuery` has 4 new Option fields populated from trace JSON
- [ ] `parse_search_query` handles null trace (all new fields are None)
- [ ] Episodes split on workspace_id change
- [ ] `EpisodeStats` has first_try_rate and outcome breakdown counts
- [ ] Test: workspace boundary splits episodes correctly
- [ ] Test: null trace metadata produces None fields (not panics or fake zeros)
- [ ] Test: episode_stats computes first_try_rate and outcome counts correctly
- [ ] Existing 3 tests still pass
- [ ] Tests pass, committed

---

### Task 2: Aggregation Functions

**Files:**
- Modify: `src/dashboard/search_analysis.rs` (add structs and functions after existing code)
- Modify: `src/tests/dashboard/search_analysis.rs` (add tests)

**What to build:** Three new public functions and two new structs that transform episodes into diagnostic views.

1. **`canonical_key(query: &str) -> Vec<String>`**: Split on whitespace, `::`, `_`, `.`, and camelCase boundaries (insert split before each uppercase letter that follows a lowercase). Lowercase all tokens. Drop filler tokens: `["find", "get", "the", "a", "an", "for", "in", "of", "to", "with"]`. Sort alphabetically. Return as Vec.

2. **`QueryProblem` struct** and **`aggregate_problems(episodes: &[SearchEpisode]) -> Vec<QueryProblem>`**: Filter to stalled + reformulated episodes. For reformulated episodes, exclude the last query (it succeeded). Group remaining queries by canonical key. Count at most one failure per episode per group (prevent inflation from multi-query episodes). Compute avg_top_score (average of `top_hit_score` values where Some), avg_result_count (average of `result_count` values where Some). Compute `triage_signal`: for reformulated episodes in the group, check if `target_symbol_name` appears in any of the group's queries' `top_hit_name` — if yes, "ranking_problem"; if no, "recall_gap"; mixed if both occur; "unknown" if no target data. Sort by failure_count desc, return top 20.

3. **`ReformulationPair` struct** and **`extract_reformulation_pairs(episodes: &[SearchEpisode]) -> Vec<ReformulationPair>`**: Filter to reformulated episodes. For each episode with N queries, extract N-1 adjacent transitions: `(queries[i].query, queries[i+1].query)`. Each pair carries the episode's `target_symbol_name` and `target_file_path`. Deduplicate by exact canonical key match on both sides (canonical_key of initial == canonical_key of existing initial AND canonical_key of successful == canonical_key of existing successful). Sum occurrences. Sort by occurrences desc, return top 15.

**Approach:**
- CamelCase splitting: iterate chars, when an uppercase char follows a lowercase char, start a new token. This handles `SearchHandler` -> `["search", "handler"]` and `HTMLParser` -> `["html", "parser"]` (consecutive uppercase stays together until a lowercase follows).
- For triage, the comparison is between `episode.target_symbol_name` and the query's `top_hit_name` field. Both are `Option<String>`. A match means the target was in results (ranking problem); no match means it wasn't (recall gap).
- `avg_top_score` and `avg_result_count` should be `Option` — None when no queries in the group have trace data.

**Acceptance criteria:**
- [ ] `canonical_key("SearchHandler")` == `canonical_key("search_handler")` == `canonical_key("search::handler")`
- [ ] `canonical_key` drops filler tokens: `canonical_key("find the handler")` == `canonical_key("handler")`
- [ ] `aggregate_problems` excludes terminal successful query from reformulated episodes
- [ ] `aggregate_problems` counts at most one failure per episode per group
- [ ] `aggregate_problems` computes triage_signal (ranking_problem / recall_gap / mixed / unknown)
- [ ] `extract_reformulation_pairs` extracts N-1 adjacent pairs from N-query episodes
- [ ] `extract_reformulation_pairs` deduplicates by exact canonical key on both sides
- [ ] Both functions handle empty episode list (return empty vec)
- [ ] Both functions handle episodes with no trace data gracefully
- [ ] Tests pass, committed

---

### Task 3: Route + Templates

**Files:**
- Modify: `src/dashboard/routes/search_analysis.rs:9` (imports)
- Modify: `src/dashboard/routes/search_analysis.rs:12-14` (SearchAnalysisParams)
- Modify: `src/dashboard/routes/search_analysis.rs:16-36` (index handler)
- Modify: `dashboard/templates/search_analysis.html` (full rewrite)
- Modify: `dashboard/templates/partials/search_episode_table.html` (score badge + filter)
- Modify: `src/tests/dashboard/integration.rs` (if analysis route integration tests exist)

**What to build:** Wire the new aggregation functions into the route and rework the template into 4 diagnostic sections.

1. **Route changes**: Add `show_all: Option<bool>` to `SearchAnalysisParams`. Import `aggregate_problems`, `extract_reformulation_pairs` alongside existing imports. After computing episodes and stats, call `aggregate_problems(&episodes)` and `extract_reformulation_pairs(&episodes)`. Filter episodes for template: if `show_all` is not true, filter to only episodes where `suspicious == true`. Insert `problems`, `reformulations`, `filtered_episodes`, `show_all`, and `total_episode_count` into the template context.

2. **Template rework** (`search_analysis.html`): Replace the current layout with 4 sections. Keep the header with Playground/Analysis/Compare nav buttons. Section 1: summary cards row — first_try_rate as percentage (multiply by 100, format to 1 decimal), outcome counts (one_shot_count, reformulation_count, stall_count, exploratory_count), window days. Section 2: problem queries table. Section 3: reformulation pairs table. Section 4: episode feed (using the existing partial).

3. **Episode table partial** (`search_episode_table.html`): Add a score badge after each query line — when `query.top_hit_score` is present, show a small `<span class="tag">` with the score formatted to 2 decimals. Add a toggle link above the feed: if not show_all, show "Showing N flagged episodes. Show all M episodes" linking to `?show_all=true&days=D`; if show_all, show "Showing all M episodes. Show flagged only" linking to `?days=D`.

**Approach:**
- Follow the existing dashboard styling conventions: `julie-card` class for sections, `label-text` for labels, `mono` for values, `tag` classes for badges. Check `dashboard/templates/metrics.html` or `dashboard/templates/projects.html` for table styling patterns.
- The triage signal column in the problem queries table should use color-coded tags: `is-danger is-light` for recall_gap (red), `is-warning is-light` for ranking_problem (amber), no special color for mixed/unknown.
- Empty states: use a centered `julie-card` with muted text, matching the existing empty state in `search_episode_table.html` line 2-5.
- The `first_try_rate` value from stats is 0.0-1.0. Multiply by 100 in the template and show with 1 decimal: `{{ (episode_stats.first_try_rate * 100) | round(precision=1) }}%`.

**Acceptance criteria:**
- [ ] Route accepts `?show_all=true` param and filters episodes accordingly
- [ ] Route passes problems, reformulations, filtered episodes to template
- [ ] Headline metrics section shows first-try success rate as percentage
- [ ] Outcome breakdown shows counts for all 4 outcome types
- [ ] Problem queries table renders with columns: Query, Variants, Failures, Triage, Avg Score, Avg Results, Last Seen
- [ ] Triage column uses color-coded tags (red for recall, amber for ranking)
- [ ] Reformulation pairs table renders with columns: Initial -> Successful, Target, Occurrences
- [ ] Episode feed shows flagged-only by default with toggle to show all
- [ ] Score badge appears on query lines when top_hit_score is present
- [ ] Empty states render correctly for each section
- [ ] Page renders without errors when there are zero episodes
- [ ] Page renders without errors when episodes have no trace data
- [ ] Tests pass, committed
