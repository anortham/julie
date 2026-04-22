# Search Quality Hardening — Task 3 Diagnosis

_Replay harness: `cargo nextest run --lib zero_hit_replay_task3 -- --ignored`_

* Fixture: `fixtures/search-quality/zero-hit-replay-task3.json`
* Raw results: `fixtures/search-quality/zero-hit-replay-task3-results.json`
* Workspace: `julie_528d4264`
* Entries replayed: 47
* Queries now returning ≥ 1 result: 44 (instrumented build vs. original telemetry)
* Queries still returning 0 results: 3

> **Context.** The captured entries are historical zero-hits from daemon telemetry. Between capture and replay, the `search-quality-hardening` branch landed Tasks 1, 2, 9, and 11 (file_pattern parser + boundary normalization, fake content-hit score removal, dashboard fix). The high `non-zero-now` count is expected: it measures how many of those historical zero-hits have already been resolved by upstream fixes on the branch, not a regression in the replay.

## Post-Recovery Live Telemetry (2026-04-22)

The replay fixture still has value, but the live daemon telemetry is the real scorecard now.

Fresh post-rollout dogfood on the rebuilt binary confirms the new scoped miss hint is live:

> `0 content matches for "line_matches" with file_pattern=src/ui/**.`
>
> `Content search found no candidate files inside file_pattern=src/ui/**.`
>
> `Try broadening that scope or removing file_pattern if you are not sure where the code lives.`

The live windows below still straddle pre-rollout traffic, so the aggregate counts include older rows that predate the new out-of-scope hint. That matters when reading the `hint_kind` split: the new `out_of_scope_content_hint` is present in fresh rows, but older `no_in_scope_candidates` rows still carry `multi_token_hint` or no hint because they were recorded before the rollout.

### Live rates

| Window | All known-target `fast_search` | Content | Definitions | Content without-recourse |
| --- | ---: | ---: | ---: | ---: |
| Last 24h | `48/174 = 27.6%` | `45/139 = 32.4%` | `3/35 = 8.6%` | `34/139 = 24.5%` |
| Last 7d | `94/362 = 26.0%` | `86/277 = 31.0%` | `8/85 = 9.4%` | `75/277 = 27.1%` |

`Without-recourse` here means content zero-hits with `hint_kind IS NULL`. On that stricter denominator, the live windows are still ugly:

- Last 24h: `34/45 = 75.6%` of content zero-hits had no hint
- Last 7d: `75/86 = 87.2%` of content zero-hits had no hint

So the recovery work is helping known buckets, but the overall live miss rate is still well above the `<= 20%` and `<= 8%` goals from the recovery plan.

### Live 24h content zero-hit split

| zero_hit_reason | file_pattern_diagnostic | hint_kind | count |
| --- | --- | --- | ---: |
| `∅` | `∅` | `∅` | 25 |
| `line_match_miss` | `∅` | `multi_token_hint` | 5 |
| `line_match_miss` | `∅` | `∅` | 5 |
| `file_pattern_filtered` | `no_in_scope_candidates` | `multi_token_hint` | 3 |
| `file_pattern_filtered` | `no_in_scope_candidates` | `∅` | 3 |
| `file_pattern_filtered` | `no_in_scope_candidates` | `out_of_scope_content_hint` | 2 |
| `file_pattern_filtered` | `∅` | `multi_token_hint` | 1 |
| `file_pattern_filtered` | `∅` | `∅` | 1 |

### Live 7d content zero-hit split

| zero_hit_reason | file_pattern_diagnostic | hint_kind | count |
| --- | --- | --- | ---: |
| `∅` | `∅` | `∅` | 66 |
| `line_match_miss` | `∅` | `multi_token_hint` | 5 |
| `line_match_miss` | `∅` | `∅` | 5 |
| `file_pattern_filtered` | `no_in_scope_candidates` | `multi_token_hint` | 3 |
| `file_pattern_filtered` | `no_in_scope_candidates` | `∅` | 3 |
| `file_pattern_filtered` | `no_in_scope_candidates` | `out_of_scope_content_hint` | 2 |
| `file_pattern_filtered` | `∅` | `multi_token_hint` | 1 |
| `file_pattern_filtered` | `∅` | `∅` | 1 |

### What moved

- The scoped root-cause split is real in production data. `file_pattern_filtered` is now split from `no_in_scope_candidates`, instead of being one opaque bucket.
- The new `out_of_scope_content_hint` is live and shows up in fresh daemon rows after the rebuild.
- `LineMatchMiss` is still a material live bucket, and the giant `∅ / ∅ / ∅` rows show there is still a pile of unattributed or unhinted content pain left.

### Verdict

The rollout cleared the narrow recovery tasks and the new hint is live, but the live telemetry says the job is not finished. The next pass should focus on the unattributed `∅ / ∅ / ∅` bucket and on shrinking `LineMatchMiss`, not on inventing more fallback hints.

## 1. Classification counts

| Class | Count |
| --- | ---: |
| `and_reached_but_dropped` | 44 |
| `or_rescued` | 0 |
| `tantivy_no_candidates` | 3 |

Of the `tantivy_no_candidates` class: **0** were single-word AND-misses (OR gate gated out by word-count), **3** were multi-word queries where OR itself produced zero candidates, and **3** of those multi-word rows are degenerate inputs (all tokens filtered out by `CodeTokenizer`, triggering the `original_terms.is_empty()` early return in `search_content`).

Degenerate-input queries (shown for completeness; they can never match anything):

* `[ ]` (filter: docs/plans/2026-04-20-fast-search-observability-and-comparison-design.md) — tokenises to zero terms
* `[ ]` (filter: docs/plans/2026-04-19-miller-retrieval-port-design.md) — tokenises to zero terms
* `[ ]` (filter: docs/plans/2026-04-19-tantivy-0.26-design.md) — tokenises to zero terms

## 2. Interpretation

The three classes map onto the implementation as follows:

* **`tantivy_no_candidates`** — `search_content` returned zero candidates. The query, as tokenised, does not intersect the corpus at the content-field + language-filter level. Causes are either (a) the tokeniser losing the term, (b) the `SearchFilter.language` narrowing the corpus, (c) the term genuinely not in the indexed code, or (d) the query tokenising to zero terms (degenerate input; see §1).
* **`or_rescued`** — AND returned zero but the OR fallback recovered candidates. The OR gate is firing; the original zero-hit must have been lost **downstream of Tantivy** (per-file filters in `line_mode_matches`, the Task 5 second-pass filter, the ranker, or the final empty-result formatter).
* **`and_reached_but_dropped`** — Tantivy AND already had candidates. If the replay also shows `final_result_count == 0`, the original telemetry's zero-hit came from a downstream drop. If `final_result_count > 0`, the upstream bug that produced the original zero-hit has **already been fixed on this branch** (Tasks 1, 2, 9, 11).

## 3. Per-query breakdown

Columns: `class`, `and`, `or`, `results` (final after ranking), `words`, and the query string with its captured filter.

| class | and | or | results | words | query | filter |
| --- | ---: | ---: | ---: | ---: | --- | --- |
| `and_reached_but_dropped` | 20 | 0 | 20 | 3 | `target_symbol_name target_file_path metadata["target"]` | lang=rust file_pattern=src/** |
| `and_reached_but_dropped` | 20 | 0 | 20 | 1 | `search_quality` | file_pattern=src/**\|xtask/**\|docs/**\|fixtures/** |
| `and_reached_but_dropped` | 4 | 0 | 4 | 7 | `dogfood search quality expected results curated queries` | file_pattern=docs/**\|src/**\|fixtures/**\|xtask/** |
| `and_reached_but_dropped` | 20 | 0 | 20 | 1 | `query=` | file_pattern=dashboard/templates/search_compare.html |
| `and_reached_but_dropped` | 3 | 0 | 3 | 1 | `prefetch` | file_pattern=docs/** |
| `and_reached_but_dropped` | 7 | 0 | 7 | 6 | `blast radius callers grouped by risk` | file_pattern=src/** |
| `and_reached_but_dropped` | 3 | 0 | 3 | 4 | `live buffer overlay unsaved` | file_pattern=src/** |
| `and_reached_but_dropped` | 13 | 0 | 13 | 1 | `buffer.update` | file_pattern=src/** |
| `and_reached_but_dropped` | 5 | 0 | 5 | 1 | `sdl-mcp` | — |
| `and_reached_but_dropped` | 5 | 0 | 5 | 6 | `tool_calls raw event log replay-result persistence` | file_pattern=src/** |
| `tantivy_no_candidates` | 0 | 0 | 0 | 2 | `[ ]` | file_pattern=docs/plans/2026-04-20-fast-search-observability-and-comparison-design.md |
| `tantivy_no_candidates` | 0 | 0 | 0 | 2 | `[ ]` | file_pattern=docs/plans/2026-04-19-miller-retrieval-port-design.md |
| `tantivy_no_candidates` | 0 | 0 | 0 | 2 | `[ ]` | file_pattern=docs/plans/2026-04-19-tantivy-0.26-design.md |
| `and_reached_but_dropped` | 38 | 0 | 38 | 1 | `run_compare` | file_pattern=src/tests/** |
| `and_reached_but_dropped` | 7 | 0 | 7 | 2 | `TODO FIXME` | file_pattern=src/dashboard/** |
| `and_reached_but_dropped` | 7 | 0 | 7 | 2 | `TODO FIXME` | file_pattern=src/search/** |
| `and_reached_but_dropped` | 7 | 0 | 7 | 2 | `TODO FIXME` | file_pattern=src/tools/search/** |
| `and_reached_but_dropped` | 6 | 0 | 6 | 10 | `token budget allocation truncation compact format deep_dive overview context full` | file_pattern=src/** |
| `and_reached_but_dropped` | 3 | 0 | 3 | 1 | `refs/heads` | file_pattern=.git/** |
| `and_reached_but_dropped` | 20 | 0 | 20 | 1 | `linked_tests` | file_pattern=src/analysis/test_linkage.rs |
| `and_reached_but_dropped` | 3 | 0 | 3 | 5 | `miller OR sdl-mcp OR TODO.md` | file_pattern={docs/**,src/**,TODO.md,*.md} |
| `and_reached_but_dropped` | 5 | 0 | 5 | 1 | `sdl-mcp` | file_pattern=docs/** |
| `and_reached_but_dropped` | 3 | 0 | 3 | 13 | `enforcing Julie tool usage in subagents OR py -3.12 OR dry-run diff output` | file_pattern=TODO.md |
| `and_reached_but_dropped` | 20 | 0 | 20 | 1 | `KIND_WEIGHTS` | file_pattern=src/search/** |
| `and_reached_but_dropped` | 4 | 0 | 4 | 6 | `entrypoint prior OR apply_entrypoint_prior OR entrypoint-aware` | file_pattern=src/search/** |
| `and_reached_but_dropped` | 6 | 0 | 6 | 8 | `alias table OR public API alias OR fast_refs` | file_pattern=src/search/** |
| `and_reached_but_dropped` | 4 | 0 | 4 | 9 | `noise filter OR exclude_docs OR broader default noise filters` | file_pattern=src/search/** |
| `and_reached_but_dropped` | 17 | 0 | 17 | 1 | `TODO.md` | file_pattern=TODO.md |
| `and_reached_but_dropped` | 20 | 0 | 20 | 1 | `get_identifiers_by_names` | file_pattern=src/tools/impact/seed.rs |
| `and_reached_but_dropped` | 20 | 0 | 20 | 4 | `from_option(value: Option<&str>) -> Self` | file_pattern=src/tools/get_context/formatting.rs |
| `and_reached_but_dropped` | 8 | 0 | 8 | 5 | `default readable compact get_context blast_radius` | file_pattern=src/tools/**/*.rs |
| `and_reached_but_dropped` | 28 | 0 | 28 | 1 | `revision_file_changes` | file_pattern=src/database/migrations.rs src/database/schema.rs src/database/mod.rs src/database/workspace.rs src/database/bulk_operations.rs |
| `and_reached_but_dropped` | 7 | 0 | 7 | 1 | `snapshot_file_hashes_tx` | file_pattern=src/database/bulk_operations.rs src/database/workspace.rs |
| `and_reached_but_dropped` | 11 | 0 | 11 | 5 | `prefer_tests stack_trace failing_test edited_files entry_symbols` | file_pattern=src/tools/get_context/scoring.rs |
| `and_reached_but_dropped` | 10 | 0 | 10 | 2 | `initialize_schema()?; run_migrations()?;` | file_pattern=src/database/mod.rs src/daemon/database.rs |
| `and_reached_but_dropped` | 14 | 0 | 14 | 1 | `SpilloverStore` | file_pattern=src/tests/tools/blast_radius_tests.rs |
| `and_reached_but_dropped` | 5 | 0 | 5 | 4 | `No impacted symbols found` | file_pattern=src/tests/tools/blast_radius_tests.rs |
| `and_reached_but_dropped` | 17 | 0 | 17 | 1 | `TODO.md` | file_pattern=TODO.md |
| `and_reached_but_dropped` | 20 | 0 | 20 | 1 | `blast_radius` | file_pattern=.claude/skills/** |
| `and_reached_but_dropped` | 12 | 0 | 12 | 5 | `blast_radius spillover_get get_context task inputs` | file_pattern=README.md\|JULIE_AGENT_INSTRUCTIONS.md\|docs/**\|.claude/skills/** |
| `and_reached_but_dropped` | 20 | 0 | 20 | 1 | `blast_radius` | file_pattern=README.md |
| `and_reached_but_dropped` | 10 | 0 | 10 | 1 | `identifier_incoming_edges` | file_pattern=src/tests/** |
| `and_reached_but_dropped` | 5 | 0 | 5 | 2 | `ImpactCandidate walk_impacts` | file_pattern=src/tests/** |
| `and_reached_but_dropped` | 10 | 0 | 10 | 2 | `from_line_match score` | file_pattern=src/tools/search/line_mode.rs |
| `and_reached_but_dropped` | 2 | 0 | 2 | 4 | `!docs/** matches_glob_pattern exclusion test` | lang=rust file_pattern=src/tests/** |
| `and_reached_but_dropped` | 20 | 0 | 20 | 7 | `3.1 OR 3.5 OR 3.6 OR 3.10` | file_pattern=docs/plans/2026-04-21-search-quality-hardening-design.md |
| `and_reached_but_dropped` | 16 | 0 | 16 | 4 | `file_pattern.clone trim filter_map empty` | file_pattern=src/tools/search/mod.rs |

## 4. Verdict on the OR-fallback gate

* OR branch fired on **0** of 47 replayed queries (`relaxed == true`).
* Suspicious rows where the gate looks like it should have fired but didn't (AND=0, multi-word, `relaxed=false`, OR>0): **0**.
* Rows attributable to the `original_terms.is_empty()` early return in `SearchIndex::search_content`: **3**.

**The replay fixture does not stress the OR-fallback gate.** No query in this set entered the OR branch, so the fixture neither confirms nor denies a gate bug. The telemetry-observed zero-hits all classified as either `and_reached_but_dropped` (44 rows, now returning results thanks to Tasks 1/2/9/11) or `tantivy_no_candidates` with a degenerate tokeniser output (3 rows). No `SearchIndex::search_content` logic fix is required for this fixture. §3.2 post-handling stays as-is; instrumentation is the deliverable.

## 5. Key finding for Task 5 (second-pass filter investigation)

While wiring the per-stage drop counters in `line_mode_matches`, the narrow test `stage_language_filter_is_redundant_with_tantivy_filter` pinned a structural observation: `line_mode_matches` propagates the caller's `language` into the `SearchFilter.language` field before calling `search_content`, so Tantivy itself drops non-matching languages and the per-file `file_matches_language` check (`line_mode.rs`, inside the Primary loop) never fires. The `language_dropped` counter is therefore dead in the current pipeline. Task 5 should either remove the redundant per-file check or reintroduce it as a safety net after the next refactor.

## 6. What Task 3 ships

* `ContentSearchResults::{and_candidate_count, or_candidate_count}` populated inside `SearchIndex::search_content`.
* `LineModeSearchResult::stage_counts: LineModeStageCounts` populated inside `line_mode_matches` (both Primary and Target-workspace paths). Counters: `and_candidates`, `or_candidates`, `tantivy_file_candidates`, `file_pattern_dropped`, `language_dropped`, `test_dropped`, `file_content_unavailable_dropped`, `line_match_miss_dropped`. The second-pass filter folds into `line_match_miss_dropped` pending Task 5.
* Narrow fixture tests at `src/tests/tools/search/line_mode_or_fallback_tests.rs` (8 tests, all green).
* Replay fixture at `fixtures/search-quality/zero-hit-replay-task3.json` (47 entries; plan quoted 44).
* Ignored replay harness at `src/tests/integration/zero_hit_replay_task3.rs` — regenerates this report.

## 7. Next steps wired from this report

* **Task 4** — use the new `stage_counts` to attribute `zero_hit_reason` per stage in `LineModeSearchResult`.
* **Task 5** — resolve the redundant per-file language filter finding above; decide whether to delete it or reintroduce it pre-Tantivy-filter.
* **Task 12** — acceptance replay will re-run this harness after Tasks 4/7/8/9/10 land and compare class counts.

## Task 12 — Acceptance replay (FastSearchTool end-to-end)

_Replay harness: `cargo nextest run --lib acceptance_replay_against_captured_zero_hits -- --ignored`_

* Fixture: `fixtures/search-quality/zero-hit-replay-task3.json`
* Entries replayed: 47
* Still zero hits after full pipeline: **37** (78.7%) — ceiling 20%
* Zero hits without an actionable hint (without-recourse): **5** (10.6%) — ceiling 8%
* Fixture entries with `limit_param > 500` (would hit the tool clamp): **0**
* Promotion rescues (content→definitions): **10**

### Zero-hit reason distribution

| reason | count |
| --- | ---: |
| `file_pattern_filtered` | 22 |
| `line_match_miss` | 6 |
| `promoted` | 6 |
| `tantivy_no_candidates` | 3 |

### Hint distribution on zero-hit results

| hint | count |
| --- | ---: |
| `multi_token_hint` | 26 |
| `none` | 5 |
| `out_of_scope_definition_hint` | 6 |
