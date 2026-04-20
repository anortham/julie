# Search Analysis Workbench

## Problem

The search analysis page (`/search/analysis`) computes quality metrics (first-try success rate, problem queries, reformulation pairs) from inferred episode classifications. Testing with live searches proved the inferences are unreliable:

- "Stalled" episodes (no downstream tool call) are counted as failures, but many are successes where the user read search results directly
- "Reformulations" (multiple similar queries) are counted as failures, but many are natural scope-narrowing
- The page reported a 29% success rate on a session where every search worked correctly

The root cause: we have no ground truth about whether a search was successful. We only have the query, results, and what happened next. Inferring quality from downstream behavior produces misleading metrics.

## Goal

Transform the analysis page from a misleading scorecard into a search observability workbench. The page becomes a discovery surface: show raw search traces with filters and friction flags, let the operator spot patterns, and funnel discoveries into the compare page (direct measurement) or dogfood test suite (regression guard).

**Quality judgment stays with:**
- Dogfood test suite (`search_quality` bucket): curated queries with expected results, deterministic pass/fail
- Compare page (`/search/compare`): run same query with different strategies, see results side by side

**The analysis page provides:** visibility into what agents are searching for, what they're getting back, and which episodes have friction signals worth investigating.

## Design

### Remove

**Headline KPI cards:**
- First-try success rate percentage
- Outcome breakdown (one-shot, reformulated, stalled, exploratory counts)

**Aggregation tables:**
- Problem queries table (grouped by canonical key, triage signal)
- Reformulation pairs table (adjacent transitions)

**Backend code to delete:**
- `QueryProblem` struct
- `ReformulationPair` struct
- `aggregate_problems` function
- `extract_reformulation_pairs` function
- `canonical_key` function and `split_camel_case` helper
- `FILLER_TOKENS` constant
- `ProblemGroup` struct and impl
- `ReformulationPairBuilder` struct
- `pair_queries_overlap` function
- `format_relative_time` function (move inline to route or keep as utility if needed)

**Keep the dead code removal clean:** `EpisodeStats` loses the outcome breakdown fields (`one_shot_count`, `reformulation_count`, `stall_count`, `exploratory_count`, `first_try_rate`). Keep `convergence_rate` and `stall_rate` only if the compare page still uses them; check and remove if not.

### Keep

- `analyze_tool_calls` (episode builder) with workspace_id boundary fix
- `SearchEpisodeQuery` with trace fields (top_hit_score, result_count, strategy, relaxed)
- `SearchEpisode` struct
- `has_trace_data` filter
- Time range selector (1h, 6h, 1d, 7d, 30d)
- `parse_search_query`, `parse_metadata`, `queries_overlap`, `token_set`
- Pre-telemetry filtering in the route

### Add

#### 1. Observational Summary Cards

Replace the KPI row with metrics that describe what's happening without judging quality:

| Card | Computation | Why useful |
|------|-------------|-----------|
| Episodes | `traced_episodes.len()` | Volume indicator |
| Zero-Hit | Count of episodes where all queries have `result_count == Some(0)` or `result_count.is_none()` with no `top_hit_name` | Genuine signal: search returned nothing |
| Median Score | Median of all `top_hit_score` values across all queries in all episodes | Distributional health; sudden drops indicate index problems |
| Repeat Query | Count of episodes flagged `repeat_query` | Friction indicator (not failure indicator) |

These are observations, not judgments. A high repeat-query rate might mean friction, or it might mean agents are thorough. The operator decides.

#### 2. Friction Flags

Replace the success/failure outcome classification with neutral descriptive flags. An episode can have multiple flags.

| Flag | Condition | What it signals |
|------|-----------|----------------|
| `zero_hits` | Any query in the episode has `result_count == Some(0)` | Search found nothing for this query |
| `repeat_query` | `queries_overlap` returns true (existing logic) | Agent rephrased a similar query |
| `low_score` | Top hit score of any query < 5.0 (configurable threshold) | Results existed but scored poorly |
| `no_follow_up` | No downstream tool call after search | Agent didn't act on results (ambiguous) |
| `relaxed` | Any query has `relaxed == Some(true)` | Search widened the query to find results |

Implementation: add `pub flags: Vec<String>` to `SearchEpisode`. Compute flags in a new `compute_flags` function called from the route (not in the episode builder, which stays pure). The episode builder's existing `outcome` and `suspicious` fields can stay for backward compat with the compare page, or be removed if unused.

#### 3. Episode Trace Table

Replace the card-based feed with a tabular layout. More scannable, filterable.

**Columns:**
| Column | Content |
|--------|---------|
| Queries | First query text, with expand to show all queries in the episode |
| Count | Number of searches in the episode |
| Flags | Colored tags for each friction flag |
| Top Score | Highest `top_hit_score` across queries in the episode |
| Results | Total `result_count` from first query |
| Downstream | Tool called after search (or "-") |
| Workspace | workspace_id |

**Filtering:** A row of flag toggle buttons above the table. Click a flag to filter to only episodes with that flag. "All" shows everything. State managed via query params (`?flag=zero_hits&hours=1`).

**Expansion:** Clicking a row expands it to show all queries in the episode with per-query detail: query text, intent, search_target, top_hit_name, top_hit_file, score, result_count, strategy, relaxed.

#### 4. Episode Actions

Each episode row has a small action link:

- **"Compare"** — links to `/search/compare?query=<first_query_text>` (pre-fills the compare page with the episode's first query)

No backend changes needed for this; it's a template-only link.

### Route Changes

`src/dashboard/routes/search_analysis.rs`:

- Accept `flag` query param for filtering: `pub flag: Option<String>`
- After building episodes and filtering pre-telemetry, call `compute_flags` on each episode
- Compute summary stats (episode count, zero-hit count, median score, repeat-query count)
- Filter by flag if param is set
- Pass `episodes`, `summary`, `active_flag`, `window_label`, `window_param` to template
- Remove imports for `aggregate_problems`, `extract_reformulation_pairs`

### New Summary Stats Struct

Replace `EpisodeStats` with `SearchSummary`:

```rust
pub struct SearchSummary {
    pub episode_count: usize,
    pub zero_hit_count: usize,
    pub zero_hit_rate: f64,
    pub median_top_score: Option<f32>,
    pub repeat_query_count: usize,
    pub repeat_query_rate: f64,
}
```

### Files Modified

| File | Change |
|------|--------|
| `src/dashboard/search_analysis.rs` | Delete aggregation code (~250 lines). Add `compute_flags` fn, `SearchSummary` struct, `compute_summary` fn. Keep episode builder and helpers. |
| `src/dashboard/routes/search_analysis.rs` | Remove aggregation calls. Add flag param, compute_flags call, summary computation, flag filtering. |
| `dashboard/templates/search_analysis.html` | Replace KPI cards with summary cards. Replace problem queries and reformulation tables with episode trace table. Add flag filter buttons. |
| `dashboard/templates/partials/search_episode_table.html` | Rewrite as trace table with expandable rows. Add per-query detail. Add Compare link. |
| `src/tests/dashboard/search_analysis.rs` | Remove tests for deleted functions. Add tests for `compute_flags`, `compute_summary`. Keep episode builder tests. |

### Files Unchanged

- `src/daemon/database.rs`
- `src/handler/search_telemetry.rs`
- `src/tools/search/trace.rs`
- `src/dashboard/search_compare.rs`

### Acceptance Criteria

- [ ] No "First-Try Success" or outcome breakdown on the page
- [ ] No problem queries table
- [ ] No reformulation pairs table
- [ ] Summary cards show: episode count, zero-hit count/rate, median top score, repeat-query count/rate
- [ ] Episodes displayed as a table with columns: Queries, Count, Flags, Top Score, Results, Downstream, Workspace
- [ ] Friction flags computed per episode: zero_hits, repeat_query, low_score, no_follow_up, relaxed
- [ ] Flag filter buttons above the table, controlled via `?flag=` query param
- [ ] Each episode row expandable to show per-query detail
- [ ] "Compare" action link on each episode row
- [ ] Time range selector preserved (1h, 6h, 1d, 7d, 30d)
- [ ] Pre-telemetry episode filtering preserved
- [ ] Page renders correctly with zero episodes
- [ ] Deleted aggregation code has no remaining callers
- [ ] Tests for compute_flags and compute_summary
- [ ] Existing episode builder tests still pass
