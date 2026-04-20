# Search Analysis Diagnostic Rework

## Problem

Claude Code and other harnesses no longer display MCP tool results inline. Where developers used to observe search quality by watching `fast_search` results scroll by, they now see "Called julie 2 times (ctrl+o to expand)" with no result detail. The process of spotting search quality issues and debugging them has lost its primary feedback loop.

The search analysis dashboard page (`/search/analysis`) was built to replace this lost observability, but it renders a chronological episode feed instead of a diagnostic tool. The telemetry layer captures rich per-query data (scores, top hits, strategies, result counts) but the analysis page discards most of it at parse time. The page cannot answer "which queries fail?" or "why do they fail?" because the evidence gets thrown away before display.

## Goal

Make search better at delivering the right result the first time. The analysis page should surface actionable quality signals: what's failing, how often, and enough context to triage whether it's a scoring problem or a coverage gap. Detailed investigation belongs on the Compare page (`/search/compare`); detection and triage belong here.

## Design Decisions (Agreed)

1. **Query aggregation**: Token-set overlap (Jaccard on word tokens, reusing existing `token_set` + `queries_overlap` logic) to group similar failing queries. Prevents fragmented views of the same underlying problem.
2. **Episode feed**: Keep below analytics sections, filtered to flagged (stalled + reformulated) episodes by default, with a toggle for all.
3. **Diagnostic depth per problem query**: Failure signal + top hit context (score, result count). Enough to distinguish "scored low" from "no results" at a glance. Full trace investigation is the Compare page's job.
4. **Reformulation pairs**: Dedicated ranked table, not inline on episodes. Pattern detection requires scannability.

## Architecture

No new database queries, routes, or pages. This reworks the aggregation layer between the existing `list_tool_calls_for_search_analysis` DB query and the template.

```
daemon DB (tool_calls table, unchanged)
  -> list_tool_calls_for_search_analysis (unchanged)
    -> analyze_tool_calls (unchanged, produces Vec<SearchEpisode>)
      -> NEW: aggregate_problems (token-set grouping of failing queries)
      -> NEW: extract_reformulation_pairs (query pairs from reformulated episodes)
      -> episode_stats (extended with first-try rate + outcome breakdown)
        -> template (reworked layout, 4 sections)
```

## Data Model Changes

### Widen `SearchEpisodeQuery`

Add fields from the trace JSON that is already stored in the `metadata` column:

| Field | Type | Source in metadata JSON |
|-------|------|------------------------|
| `top_hit_score` | `Option<f32>` | `trace.top_hits[0].score` |
| `result_count` | `usize` | `trace.result_count` |
| `strategy` | `String` | `trace.strategy` |
| `relaxed` | `bool` | `trace.relaxed` |

These are extracted in `parse_search_query` from the same JSON blob already being parsed. No DB schema changes.

### New `QueryProblem` struct

One per aggregated problem query group:

| Field | Type | Purpose |
|-------|------|---------|
| `representative_query` | `String` | Most frequent query text in the group |
| `variants` | `Vec<String>` | Other query texts that token-overlap with representative |
| `failure_count` | `usize` | Total episodes where this group stalled or reformulated |
| `stall_count` | `usize` | Episodes where this group stalled |
| `reformulation_count` | `usize` | Episodes where this group required reformulation |
| `last_seen` | `i64` | Most recent failure timestamp |
| `avg_top_score` | `Option<f32>` | Average top hit score across failures (None when no results) |
| `avg_result_count` | `f32` | Average result count across failures |

### New `ReformulationPair` struct

| Field | Type | Purpose |
|-------|------|---------|
| `initial_query` | `String` | What the agent searched first (the failing query) |
| `successful_query` | `String` | What the agent searched that worked |
| `target_name` | `Option<String>` | Downstream symbol found |
| `target_file` | `Option<String>` | File where target was found |
| `occurrences` | `usize` | How many times this pair appeared |

### Extend `EpisodeStats`

Add to existing struct:

| Field | Type | Purpose |
|-------|------|---------|
| `first_try_rate` | `f64` | `one_shot_success / total_episodes` |
| `one_shot_count` | `usize` | Outcome breakdown |
| `reformulation_count` | `usize` | Outcome breakdown |
| `stall_count` | `usize` | Outcome breakdown |
| `exploratory_count` | `usize` | Outcome breakdown |

Existing `convergence_rate` and `stall_rate` stay (used by compare page).

## Aggregation Functions

### `aggregate_problems(episodes: &[SearchEpisode]) -> Vec<QueryProblem>`

1. Filter episodes to those with outcome `stalled` or `reformulation_converged`.
2. Collect all queries from those episodes.
3. Group queries by token-set overlap: for each query, check if it overlaps with an existing group's representative (using `token_set` and a Jaccard threshold). If yes, add to that group. If no, start a new group.
4. For each group, compute: representative (most frequent query text), variants, failure/stall/reformulation counts, last_seen, avg_top_score, avg_result_count.
5. Sort by `failure_count` descending.
6. Return top 20.

### `extract_reformulation_pairs(episodes: &[SearchEpisode]) -> Vec<ReformulationPair>`

1. Filter episodes to those with outcome `reformulation_converged`.
2. For each episode: take the first query as `initial_query`, the last query as `successful_query`, and the episode's `target_symbol_name`/`target_file_path` as the target.
3. Deduplicate pairs by token-set overlap on both initial and successful queries. Sum occurrences.
4. Sort by `occurrences` descending.
5. Return top 15.

## Template Layout

### Section 1: Headline Metrics (summary cards row)

- **First-try success rate** (percentage, prominent, larger font)
- **Outcome breakdown**: one-shot | reformulated | stalled | exploratory (counts)
- **Window** (days, same as current)

### Section 2: Problem Queries (table)

Columns: Query | Variants | Failures (stall/reform split) | Avg Score | Avg Results | Last Seen

Sorted by failure count descending. Capped at 20 rows. Note if truncated.

Empty state: "No problem queries detected. Either search is working well or there isn't enough data yet."

### Section 3: Reformulation Pairs (table)

Columns: Initial Query -> Successful Query | Target (symbol @ file) | Occurrences

Sorted by occurrences descending. Capped at 15 rows.

Empty state: "No reformulation patterns detected."

### Section 4: Flagged Episodes (existing cards, filtered)

Default: only stalled + reformulated episodes. Toggle link: "Show all N episodes".

Episode cards are unchanged except: add a small score badge next to each query line showing the top hit score (when available), so you can see at a glance whether results existed but scored low.

## Files Modified

| File | Change |
|------|--------|
| `src/dashboard/search_analysis.rs` | Widen `SearchEpisodeQuery` (4 fields), add `QueryProblem` struct, add `ReformulationPair` struct, extend `EpisodeStats` (5 fields), add `aggregate_problems` fn, add `extract_reformulation_pairs` fn, update `parse_search_query` to extract trace fields |
| `src/dashboard/routes/search_analysis.rs` | Call new aggregation functions, pass `problems`, `reformulations` to template context |
| `dashboard/templates/search_analysis.html` | Replace current layout with 4-section diagnostic layout |
| `dashboard/templates/partials/search_episode_table.html` | Add score badge per query, support `show_all` toggle |
| `src/tests/dashboard/search_analysis.rs` | Tests for `aggregate_problems`, `extract_reformulation_pairs`, extended `episode_stats` |

## Files Unchanged

- `src/tools/search/trace.rs` (trace infrastructure)
- `src/handler/search_telemetry.rs` (telemetry capture)
- `src/daemon/database.rs` (DB schema and queries)
- `src/dashboard/search_compare.rs` (compare page)
- Episode builder logic (`analyze_tool_calls`, `EpisodeBuilder`)

## Acceptance Criteria

- [ ] First-try success rate displayed as headline percentage on `/search/analysis`
- [ ] Outcome breakdown (one-shot, reformulated, stalled, exploratory) shown as counts
- [ ] Problem queries table ranks failing queries by frequency with token-set grouping
- [ ] Each problem query row shows avg top hit score and avg result count for triage
- [ ] Reformulation pairs table shows initial -> successful query with target and occurrences
- [ ] Episode feed defaults to flagged-only with toggle for all
- [ ] Episode query lines show top hit score badge when available
- [ ] All new aggregation functions have unit tests
- [ ] Existing search_analysis tests still pass
- [ ] Page renders correctly with zero episodes (empty states)
- [ ] Page renders correctly with episodes that have no trace data (pre-telemetry calls)
