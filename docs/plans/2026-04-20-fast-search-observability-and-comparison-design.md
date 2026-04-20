# Fast Search Observability and Comparison Design

## Summary

Build a dashboard-centered observability and comparison system for `fast_search`.

The project has three linked goals:

1. Make the dashboard the source of truth for `fast_search` behavior now that harness transcripts no longer show tool output reliably.
2. Capture enough live search telemetry to explain what happened during real agent sessions without pushing analysis work onto the hot path.
3. Add a side-by-side comparison bench so Julie can replay real query corpora against baseline and candidate search strategies and judge whether ranking changes help or hurt.

The design uses `tool_calls` as the raw event log, extends `fast_search` metadata with compact top-hit telemetry, analyzes behavior in terms of search episodes instead of isolated calls, and adds a dashboard comparison surface that runs offline or background replay jobs against captured corpora.

## Why this is a separate project

This is not a cosmetic dashboard polish task.

- `fast_search` is Julie's highest-volume tool, so blind spots here matter more than blind spots elsewhere.
- The harness layer now hides tool output in many sessions, which removes the old feedback loop.
- The current dashboard search playground does not execute the same path as `fast_search`. It queries `SearchIndex` directly in [`src/dashboard/routes/search.rs`](</Users/murphy/source/julie/src/dashboard/routes/search.rs:46>), so it cannot explain hybrid merge, path priors, filter defaults, or future ranking variants truthfully.
- The current metrics page counts calls and latency, but it cannot answer the hard questions: Did search converge? Did it stall? Did docs outrank source? Did the agent pick rank 1 or rank 7?

Without this project, search tuning stays guesswork.

## Goals

1. Capture structured `fast_search` telemetry from live sessions with low request-path cost.
2. Group bursts of related `fast_search` calls into search episodes so exploratory bursts are not mislabeled as failure.
3. Record enough downstream-tool target metadata to tell whether search converged on a symbol or file.
4. Expose trace, episode, and comparison views in the dashboard.
5. Refactor the dashboard search playground onto the same shared execution core as `fast_search`.
6. Add a replay bench that compares baseline and candidate search strategies over a captured or hand-built corpus.

## Non-goals

- No live quality scoring on the `fast_search` hot path.
- No model-based judge in v1.
- No harness UI changes.
- No attempt to infer agent intent from prompt text outside Julie's own tool-call stream.
- No rewrite of Julie's ranking stack before observability exists.

## Current pressure points

### 1. Search output is hidden upstream

The user can no longer trust the harness transcript as a record of `fast_search` behavior. Julie needs first-party visibility.

### 2. The current search playground is not faithful

The search playground route in [`src/dashboard/routes/search.rs`](</Users/murphy/source/julie/src/dashboard/routes/search.rs:46>) calls `search_symbols` and `search_content` on the index directly. That path bypasses the actual `fast_search` tool pipeline in [`src/handler.rs`](</Users/murphy/source/julie/src/handler.rs:2378>) and [`src/tools/search/text_search.rs`](</Users/murphy/source/julie/src/tools/search/text_search.rs:25>).

### 3. Repeated search bursts are ambiguous

Agents often fire many `fast_search` calls in close succession on purpose. A raw count of repeated searches will over-report failure unless Julie reasons in episodes and looks at what those bursts lead to.

### 4. Existing metrics are too coarse

The metrics page in [`src/dashboard/routes/metrics.rs`](</Users/murphy/source/julie/src/dashboard/routes/metrics.rs:20>) can show volume, latency, and output size, but not search quality or convergence.

## Primary code areas

### Existing files

- [`src/handler.rs`](</Users/murphy/source/julie/src/handler.rs:1627>) for tool-call recording and `fast_search` entrypoint metadata.
- [`src/tools/search/text_search.rs`](</Users/murphy/source/julie/src/tools/search/text_search.rs:25>) for shared search behavior.
- [`src/search/hybrid.rs`](</Users/murphy/source/julie/src/search/hybrid.rs:174>) for hybrid merge behavior.
- [`src/daemon/database.rs`](</Users/murphy/source/julie/src/daemon/database.rs:495>) for raw tool-call persistence and replay-result persistence.
- [`src/dashboard/mod.rs`](</Users/murphy/source/julie/src/dashboard/mod.rs:109>) for route registration.
- [`src/dashboard/routes/search.rs`](</Users/murphy/source/julie/src/dashboard/routes/search.rs:24>) for the search playground.
- [`src/dashboard/routes/metrics.rs`](</Users/murphy/source/julie/src/dashboard/routes/metrics.rs:20>) for summary cards and tool breakdown.
- [`src/dashboard/routes/events.rs`](</Users/murphy/source/julie/src/dashboard/routes/events.rs:13>) and [`src/dashboard/state.rs`](</Users/murphy/source/julie/src/dashboard/state.rs:30>) for live dashboard events.
- [`dashboard/templates/search.html`](</Users/murphy/source/julie/dashboard/templates/search.html:1>) and [`dashboard/templates/partials/search_results.html`](</Users/murphy/source/julie/dashboard/templates/partials/search_results.html:1>) for the playground UI.
- [`dashboard/templates/partials/search_detail.html`](</Users/murphy/source/julie/dashboard/templates/partials/search_detail.html:1>) for expandable hit detail.
- [`dashboard/templates/metrics.html`](</Users/murphy/source/julie/dashboard/templates/metrics.html:1>) and [`dashboard/templates/partials/metrics_table.html`](</Users/murphy/source/julie/dashboard/templates/partials/metrics_table.html:1>) for metrics surfaces.

### Planned new files

- `src/tools/search/trace.rs`
- `src/dashboard/search_analysis.rs`
- `src/dashboard/search_compare.rs`
- `src/dashboard/routes/search_analysis.rs`
- `src/dashboard/routes/search_compare.rs`
- `dashboard/templates/search_analysis.html`
- `dashboard/templates/partials/search_episode_table.html`
- `dashboard/templates/partials/search_compare_results.html`

These modules keep route handlers lean and avoid pushing large analysis code into existing files that are already busy.

## Design decisions

### 1. The dashboard must run the real search path

Chosen behavior: extract a shared structured search execution layer and route both `fast_search` and the dashboard search playground through it.

The current split is not acceptable:

- `fast_search` goes through the tool path and can use hybrid search, heuristics, metadata capture, and future strategy variants.
- the dashboard playground uses a narrower path and shows a partial truth

The new shared execution layer should return a structured result object with:

- ranked hits
- trace diagnostics
- result counts
- strategy id
- telemetry-ready top-hit summaries

The MCP tool can still render its current text output from that structure. The dashboard can render rich panels from the same structure without parsing the tool's formatted text.

This is the foundation that makes the rest of the design honest.

### 2. Raw telemetry stays on the existing metrics path

Chosen behavior: capture compact search telemetry in `tool_calls.metadata` and reuse the existing bounded async metrics writer.

This keeps request-path cost close to today's cost envelope:

- `record_tool_call` already serializes metadata and `try_send`s it through the bounded metrics channel in [`src/handler.rs`](</Users/murphy/source/julie/src/handler.rs:1665>).
- the background writer already handles SQLite persistence in [`src/handler.rs`](</Users/murphy/source/julie/src/handler.rs:176>).

New search telemetry fields for `fast_search` should include:

- `query`
- `normalized_query`
- `search_target`
- `language`
- `file_pattern`
- `limit`
- `intent`
- `strategy`
- `result_count`
- `top_hits` with top 3 hit summaries
- `trace_version`

Each top-hit summary should include:

- `rank`
- `symbol_id` when available
- `name`
- `kind`
- `file_path`
- `line`
- `score`

No live clustering, no replay judgment, and no cross-query comparison work belongs on the request path.

### 3. Intent labels are telemetry-first, not ranking-first

Chosen behavior: infer a small stable intent label for telemetry and dashboard grouping, but do not let that label drive ranking until it proves useful.

The first label set should be:

- `symbol_lookup`
- `code_investigation`
- `api_tool_lookup`
- `conceptual_code`
- `content_grep`
- `unknown`

The classifier should use cheap heuristics:

- query shape
- identifier signals
- word count
- `search_target`
- tool-oriented phrases such as `find references`, `wrapper`, `handler`, `mcp`, `call path`

This keeps the label set interpretable and avoids baking a shaky classifier into retrieval before Julie has evidence that it helps.

### 4. Search episodes are the unit of analysis

Chosen behavior: group related `fast_search` calls into episodes and analyze what the episode led to.

Episode rule set:

1. An episode starts with a `fast_search`.
2. Additional `fast_search` calls join the same episode while they occur within 10 seconds of the prior search and no non-search tool fires in between.
3. The first non-search tool closes the episode.
4. A new `fast_search` after that boundary starts a new episode.

This matches the user requirement:

- bursts of many searches can be intentional exploration
- the burst itself is not suspicious
- the outcome is what matters

### 5. Useful downstream action is explicit

Chosen behavior: treat the following tools as useful downstream actions for search episodes:

- `deep_dive`
- `get_symbols`
- `fast_refs`
- `call_path`
- `get_context`
- `edit_file`
- `rewrite_symbol`
- `rename_symbol`

This set treats `get_context` as valid exploration and not dead air.

### 6. Convergence is symbol-first with file fallback

Chosen behavior: define convergence in terms of the same symbol when possible, with file-path fallback when symbol resolution is absent or the tool is file-oriented.

Downstream useful-action metadata should capture:

- `target_symbol_id` when resolvable
- `target_symbol_name` when available
- `target_file_path`
- `target_line` when available

Suspicious search episodes under the default balanced rule are:

- overlapping or near-duplicate queries that converge on the same downstream symbol or file
- episodes that end without a useful downstream action

For v1, overlap detection should use a deterministic normalized-query comparison:

- exact normalized match
- one normalized query contains the other
- token-overlap threshold

That logic belongs in dashboard analysis code, not the hot path.

### 7. Search outcomes should be classified, not guessed

Chosen behavior: assign each episode one outcome label.

The first outcome set should be:

- `one_shot_success`
- `exploratory_success`
- `reformulation_converged`
- `stalled`
- `dispersed`
- `insufficient_data`

Meaning:

- `one_shot_success`: one search, then a useful downstream action
- `exploratory_success`: multiple searches, then a useful downstream action without suspicious convergence
- `reformulation_converged`: overlapping searches collapsed onto the same target
- `stalled`: no useful downstream action after the episode
- `dispersed`: burst ended in multiple unrelated downstream targets
- `insufficient_data`: missing telemetry or dropped metrics left the result ambiguous

### 8. The metrics page stays light, the search area gets the heavy surfaces

Chosen behavior: leave the current metrics page focused on volume and latency, add search quality summary cards there, and put detailed episode analysis and replay comparison under the search area.

Reasoning:

- [`dashboard/templates/metrics.html`](</Users/murphy/source/julie/dashboard/templates/metrics.html:1>) auto-refreshes and should stay cheap.
- search analysis and replay are heavier and more interactive.
- the existing search playground is the natural home for search transparency.

The search area should gain three tabs or sibling views:

- `Playground`
- `Analysis`
- `Compare`

### 9. Comparison is a first-class replay bench

Chosen behavior: add side-by-side replay comparison against captured or hand-built corpora.

Corpus sources:

- captured live `fast_search` rows from `tool_calls`
- filtered by workspace, time range, intent, or outcome
- optional hand-entered query list for focused inspection

Comparison behavior:

- choose baseline strategy and candidate strategy
- run both against the same corpus
- show per-query hit diffs and aggregate metrics

Aggregate metrics should include:

- top-1 match rate against later chosen symbol or file when known
- top-3 containment rate against later chosen symbol or file when known
- source-over-docs win rate for `code_investigation`
- episode stall rate
- reformulation-convergence rate
- duplicate-result rate
- latency deltas

### 10. Replay runs are persisted, episode analysis is derived

Chosen behavior: keep raw event capture in `tool_calls`, derive episodes from raw events, and persist replay-run artifacts in dedicated daemon tables.

This split keeps the live path lean while still saving the expensive comparison work.

New daemon tables:

- `search_compare_runs`
- `search_compare_cases`

`search_compare_runs` should store:

- run id
- created_at
- workspace scope
- corpus spec
- baseline strategy
- candidate strategy
- case count
- summary metrics
- status

`search_compare_cases` should store:

- run id
- ordinal
- query
- expected downstream target when known
- baseline top hits
- candidate top hits
- per-case judgment summary

Episode analysis does not need its own persistence in v1. It can be rebuilt from `tool_calls` for the requested time window.

## Data flow

### Live capture flow

1. `fast_search` executes through the shared structured search core.
2. The tool records compact telemetry in `ToolCallReport.metadata`.
3. `record_tool_call` writes the raw event through the existing bounded metrics channel.
4. The background writer persists the event into workspace `tool_calls` and daemon `tool_calls`.
5. Useful downstream tools write target metadata into their own `tool_calls.metadata`.
6. The dashboard analysis layer reconstructs episodes from chronological tool calls within a session.

### Dashboard analysis flow

1. The dashboard reads recent tool calls for the selected workspace and time window.
2. It filters to sessions that include `fast_search`.
3. It groups `fast_search` calls into episodes using the agreed 10-second plus boundary rule.
4. It attaches the first downstream useful action when present.
5. It computes outcome labels and summary aggregates.
6. It renders trace tables, episode tables, and drill-down views.

### Replay comparison flow

1. The dashboard builds a corpus from captured tool calls or a manual query set.
2. It schedules a background comparison job.
3. The job runs baseline and candidate strategies against the same corpus.
4. It computes aggregate metrics and per-case diffs.
5. It stores the run and case results in the daemon database.
6. The dashboard renders the finished run and allows later inspection.

## Error handling and failure behavior

### Metrics backpressure

The current metrics path drops records under backpressure. Search analysis must account for that and surface gaps as `insufficient_data` and never pretend the trace is complete.

### Missing target metadata

When downstream symbol resolution fails, convergence falls back to file path. If both are absent, the episode remains analyzable but with weaker judgment.

### Dashboard comparison failures

Replay jobs should be background tasks with explicit status:

- `queued`
- `running`
- `succeeded`
- `failed`

The compare page should never block on a long-running replay in the request thread.

### Strategy drift

Each telemetry row and replay result should include `trace_version` and `strategy_id` fields so old traces remain interpretable after future search changes.

## Verification strategy

### Unit tests

- intent classification tests
- top-hit summary extraction tests
- normalized-query overlap tests
- episode builder tests for burst boundaries
- outcome-label tests

### Integration tests

- handler metrics recording with enriched `fast_search` metadata
- daemon DB migration and replay-table persistence
- dashboard route tests for new search analysis and compare views
- replay comparison tests with deterministic fixture corpora

### Dogfood validation

- run the dashboard against real Julie sessions
- confirm the search playground and `fast_search` agree on ranked hits for the same query and workspace
- confirm intentional exploratory bursts are not mislabeled as failure
- confirm known reformulation cases are surfaced as `reformulation_converged`

## Acceptance criteria

- [ ] The dashboard search playground uses the same shared search execution core as `fast_search`.
- [ ] `fast_search` persists compact structured telemetry with top 3 hits through the existing metrics path.
- [ ] Useful downstream tools persist enough target metadata for symbol-or-file convergence analysis.
- [ ] The dashboard can reconstruct search episodes from real tool-call history using the agreed 10-second plus boundary rule.
- [ ] The dashboard exposes search episode analysis with outcome labels and drill-down traces.
- [ ] The dashboard exposes a side-by-side replay comparison view for baseline and candidate strategies.
- [ ] Replay runs are persisted in the daemon database and can be revisited later.
- [ ] The request-path cost stays within the current metrics shape by avoiding live quality analysis on the hot path.
- [ ] Tests cover intent labels, episode construction, replay comparison, and dashboard rendering for the new surfaces.

## Deliverables

- shared structured search execution path for tool and dashboard use
- enriched `fast_search` telemetry in `tool_calls`
- useful-action target telemetry for downstream tools
- dashboard `Analysis` and `Compare` surfaces
- persisted replay-run artifacts in daemon DB
- search-quality summary cards that point from metrics into the search analysis views

## Follow-up questions for implementation planning

- Which search-strategy enum gives the cleanest baseline-versus-candidate comparison story without infecting the hot path with experimental branches?
- Which downstream tools can resolve a stable `target_symbol_id` cheaply, and where does file-path fallback need to carry more weight?
- How should the compare page sample corpora so large workspaces stay usable without hiding important outliers?
