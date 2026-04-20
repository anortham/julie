# Search Analysis Diagnostic Rework

## Problem

Claude Code and other harnesses no longer display MCP tool results inline. Where developers used to observe search quality by watching `fast_search` results scroll by, they now see "Called julie 2 times (ctrl+o to expand)" with no result detail. The process of spotting search quality issues and debugging them has lost its primary feedback loop.

The search analysis dashboard page (`/search/analysis`) was built to replace this lost observability, but it renders a chronological episode feed instead of a diagnostic tool. The telemetry layer captures rich per-query data (scores, top hits, strategies, result counts) but the analysis page discards most of it at parse time. The page cannot answer "which queries fail?" or "why do they fail?" because the evidence gets thrown away before display.

## Goal

Make search better at delivering the right result the first time. The analysis page should surface actionable quality signals: what's failing, how often, and enough context to triage whether it's a scoring problem or a coverage gap. Detailed investigation belongs on the Compare page (`/search/compare`); detection and triage belong here.

## Design Decisions (Agreed)

1. **Query aggregation**: Canonical key grouping (lowercase, split on case/punctuation/`::` boundaries, drop filler tokens) for problem queries. Handles `SearchHandler` / `search_handler` / `search::handler` as one group. No fuzzy overlap on reformulation pairs (exact canonical match only).
2. **Episode feed**: Keep below analytics sections, filtered to flagged (stalled + reformulated) episodes by default, with a toggle for all.
3. **Diagnostic depth per problem query**: Failure signal + top hit context (score, result count) + recall-vs-ranking triage signal. Enough to distinguish "scored low" from "no results" and "had the answer but ranked wrong" at a glance. Full trace investigation is the Compare page's job.
4. **Reformulation pairs**: Dedicated ranked table, not inline on episodes. Extract adjacent query transitions (not just first-vs-last endpoints). Pattern detection requires scannability.
5. **Episode boundaries**: Add workspace_id as a boundary condition in `analyze_tool_calls`. Cross-workspace searches in the same session must not merge into one episode.

## Architecture

No new database queries, routes, or pages. This reworks the episode builder boundary logic and the aggregation layer between the existing `list_tool_calls_for_search_analysis` DB query and the template.

```
daemon DB (tool_calls table, unchanged)
  -> list_tool_calls_for_search_analysis (unchanged)
    -> analyze_tool_calls (FIX: add workspace_id boundary)
      -> NEW: aggregate_problems (canonical key grouping of failing queries)
      -> NEW: extract_reformulation_pairs (adjacent transitions from reformulated episodes)
      -> episode_stats (extended with first-try rate + outcome breakdown)
        -> template (reworked layout, 4 sections)
```

## Data Model Changes

### Widen `SearchEpisodeQuery`

Add fields from the trace JSON that is already stored in the `metadata` column. All new fields are `Option` to handle pre-telemetry calls and failed executions where `trace` is null:

| Field | Type | Source in metadata JSON |
|-------|------|------------------------|
| `top_hit_score` | `Option<f32>` | `trace.top_hits[0].score` |
| `result_count` | `Option<usize>` | `trace.result_count` |
| `strategy` | `Option<String>` | `trace.strategy` |
| `relaxed` | `Option<bool>` | `trace.relaxed` |

These are extracted in `parse_search_query` from the same JSON blob already being parsed. No DB schema changes.

### New `QueryProblem` struct

One per aggregated problem query group:

| Field | Type | Purpose |
|-------|------|---------|
| `representative_query` | `String` | Most frequent query text in the group |
| `variants` | `Vec<String>` | Other query texts that canonicalize to the same key |
| `failure_count` | `usize` | Total episodes where this group stalled or reformulated |
| `stall_count` | `usize` | Episodes where this group stalled |
| `reformulation_count` | `usize` | Episodes where this group required reformulation |
| `last_seen` | `i64` | Most recent failure timestamp |
| `avg_top_score` | `Option<f32>` | Average top hit score across failures (None when no results) |
| `avg_result_count` | `Option<f32>` | Average result count across failures (None when no trace data) |
| `triage_signal` | `String` | "recall_gap", "ranking_problem", "mixed", or "unknown" |

### New `ReformulationPair` struct

| Field | Type | Purpose |
|-------|------|---------|
| `initial_query` | `String` | What the agent searched in this transition step |
| `successful_query` | `String` | What the agent searched next (that led to convergence) |
| `target_name` | `Option<String>` | Downstream symbol found |
| `target_file` | `Option<String>` | File where target was found |
| `occurrences` | `usize` | How many times this adjacent pair appeared |

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

## Episode Builder Fix

### Workspace boundary in `analyze_tool_calls`

Add `workspace_id` mismatch as a boundary condition in the `should_start_new` check. Current boundaries: different session, >10s gap, episode closed. New boundary: different workspace.

```rust
let should_start_new = current.as_ref().is_none_or(|episode| {
    episode.session_id != row.session_id
        || episode.workspace_id != row.workspace_id  // NEW
        || row.timestamp - episode.last_search_ts > 10
        || episode.closed
});
```

This prevents cross-workspace searches from merging into one episode, which would corrupt all downstream metrics.

### Problem query extraction

For stalled episodes: all queries are candidates for the problem query table.

For reformulated episodes: exclude the terminal successful query (the last one). The successful query solved the problem; including it in the "problem" pool inflates failure counts and pollutes groupings. Count at most once per episode per group to prevent multi-query episodes from inflating a single group.

## Aggregation Functions

### Canonical key for query grouping

```
fn canonical_key(query: &str) -> Vec<String>
```

1. Split on whitespace, `::`, `_`, `.`, and camelCase boundaries
2. Lowercase all tokens
3. Drop filler tokens: "find", "get", "the", "a", "an", "for", "in", "of", "to", "with"
4. Sort tokens alphabetically (order-independent)
5. Return as `Vec<String>`

Two queries match when their canonical keys are identical. This handles `SearchHandler`, `search_handler`, `search::handler`, and `search handler` as the same group.

### `aggregate_problems(episodes: &[SearchEpisode]) -> Vec<QueryProblem>`

1. Filter episodes to those with outcome `stalled` or `reformulation_converged`.
2. Collect candidate queries: for stalled episodes, all queries; for reformulated episodes, all except the last query.
3. Group by canonical key. For each group, count at most one failure per episode to prevent inflation.
4. For each group, compute: representative (most frequent raw query text), variants (other raw texts), failure/stall/reformulation counts, last_seen, avg_top_score, avg_result_count.
5. Compute `triage_signal` per group (see below).
6. Sort by `failure_count` descending.
7. Return top 20.

### Recall-vs-ranking triage

For reformulated episodes where we have both the initial query's `top_hits` (from trace) and the episode's `target_symbol_name`:

- If the target symbol name appears in the initial query's top_hits: **ranking_problem** (search had the answer but ranked it too low)
- If the target symbol name does not appear in the initial query's top_hits: **recall_gap** (search didn't find it at all)
- If trace data is missing or target is unknown: **unknown**

Aggregate per problem group: if >50% ranking, "ranking_problem"; if >50% recall, "recall_gap"; otherwise "mixed".

This is the strongest diagnostic signal: it tells you whether to fix scoring or fix indexing/tokenization.

### `extract_reformulation_pairs(episodes: &[SearchEpisode]) -> Vec<ReformulationPair>`

1. Filter episodes to those with outcome `reformulation_converged`.
2. For each episode with N queries, extract N-1 adjacent transitions: `(query[0] -> query[1])`, `(query[1] -> query[2])`, etc. Each transition carries the episode's downstream target.
3. Deduplicate by exact canonical key match on both sides of the pair. Sum occurrences.
4. Sort by `occurrences` descending.
5. Return top 15.

## Template Layout

### Section 1: Headline Metrics (summary cards row)

- **First-try success rate** (percentage, prominent, larger font)
- **Outcome breakdown**: one-shot | reformulated | stalled | exploratory (counts)
- **Window** (days, same as current)

### Section 2: Problem Queries (table)

Columns: Query | Variants | Failures (stall/reform split) | Triage | Avg Score | Avg Results | Last Seen

The Triage column shows a compact label: "ranking" (amber), "recall" (red), "mixed" (grey), or "unknown" (muted).

Sorted by failure count descending. Capped at 20 rows. Note if truncated.

Empty state: "No problem queries detected. Either search is working well or there isn't enough data yet."

### Section 3: Reformulation Pairs (table)

Columns: Initial Query -> Successful Query | Target (symbol @ file) | Occurrences

Sorted by occurrences descending. Capped at 15 rows.

Empty state: "No reformulation patterns detected."

### Section 4: Flagged Episodes (existing cards, filtered)

Default: only stalled + reformulated episodes. Toggle link: "Show all N episodes".

Episode cards are unchanged except: add a small score badge next to each query line showing the top hit score (when available), so you can see at a glance whether results existed but scored low.

The toggle is controlled via a query parameter (`?show_all=true`), requiring the route to accept it.

## Files Modified

| File | Change |
|------|--------|
| `src/dashboard/search_analysis.rs` | Widen `SearchEpisodeQuery` (4 `Option` fields), add `QueryProblem` struct (with `triage_signal`), add `ReformulationPair` struct, extend `EpisodeStats` (5 fields), add `canonical_key` fn, add `aggregate_problems` fn, add `extract_reformulation_pairs` fn, add recall-vs-ranking triage logic, update `parse_search_query` to extract trace fields, fix problem query extraction to exclude terminal successful query |
| `src/dashboard/routes/search_analysis.rs` | Accept `show_all` query param, call new aggregation functions, pass `problems`, `reformulations`, `show_all` to template context |
| `dashboard/templates/search_analysis.html` | Replace current layout with 4-section diagnostic layout |
| `dashboard/templates/partials/search_episode_table.html` | Add score badge per query, support filtered/all toggle |
| `src/tests/dashboard/search_analysis.rs` | Tests for `canonical_key`, `aggregate_problems`, `extract_reformulation_pairs`, extended `episode_stats`, recall-vs-ranking triage, workspace boundary, null trace handling |

## Files With Targeted Fix

| File | Change |
|------|--------|
| `src/dashboard/search_analysis.rs` (`analyze_tool_calls`) | Add `workspace_id` to the `should_start_new` boundary check. One-line addition to existing condition. |

## Files Unchanged

- `src/tools/search/trace.rs` (trace infrastructure)
- `src/handler/search_telemetry.rs` (telemetry capture)
- `src/daemon/database.rs` (DB schema and queries)
- `src/dashboard/search_compare.rs` (compare page)

## Test Coverage Requirements

Beyond the acceptance criteria, the following edge cases must be covered:

- Null trace metadata (pre-telemetry tool calls)
- Zero-hit trace (trace exists but `top_hits` is empty, `result_count` is 0)
- 3+ query reformulation episodes (adjacent pair extraction)
- Workspace switches within a session (boundary check)
- >10s gaps between searches (episode split)
- Mixed `search_target` within an episode (definitions vs content)
- Repeated query variants within a single episode (one-per-episode counting)
- Empty episode list (all empty states render)
- `show_all` toggle (route param handling)

## Acceptance Criteria

- [ ] Episode builder splits on workspace_id change (cross-workspace searches never merge)
- [ ] First-try success rate displayed as headline percentage on `/search/analysis`
- [ ] Outcome breakdown (one-shot, reformulated, stalled, exploratory) shown as counts
- [ ] Problem queries table ranks failing queries by canonical key grouping
- [ ] Problem query extraction excludes terminal successful query from reformulated episodes
- [ ] Each problem query row shows triage signal (ranking/recall/mixed/unknown)
- [ ] Each problem query row shows avg top hit score and avg result count
- [ ] Reformulation pairs table shows adjacent transitions with target and occurrences
- [ ] Reformulation pair dedup uses exact canonical key match (no fuzzy)
- [ ] Episode feed defaults to flagged-only with toggle for all (`?show_all=true`)
- [ ] Episode query lines show top hit score badge when available
- [ ] All trace-derived fields are `Option` and handle null trace gracefully
- [ ] All new aggregation functions have unit tests covering edge cases listed above
- [ ] Existing search_analysis tests still pass
- [ ] Page renders correctly with zero episodes (empty states)
- [ ] Page renders correctly with episodes that have no trace data (pre-telemetry calls)
