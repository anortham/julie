# Search Quality Hardening Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use razorback:team-driven-development (on Claude Code) or razorback:subagent-driven-development (elsewhere) to implement this plan. Fall back to razorback:executing-plans for single-task or tightly-sequential plans.

**Goal:** Cut `fast_search` content-target zero-hit rate from 30% to ≤20% raw and ≤8% without-recourse, eliminate silent `file_pattern` parsing failures, and add per-stage zero-hit telemetry covering the live `line_mode_matches` pipeline.

**Architecture:** Asymmetric forgiveness — labeled auto-promotion for single-identifier content→definitions flips (format-preserving), structured hints for multi-token concept queries (format-changing). New composite `SearchExecutionKind::Promoted` variant carries both result sets. New `hint_kind` persisted field makes the without-recourse metric measurable from `tool_calls.metadata`. Boundary normalization of `file_pattern` at `execute_search` covers every caller path.

**Tech Stack:** Rust, Tantivy (live content search), globset (glob matching), SQLite (telemetry persistence), tree-sitter extractors (unchanged).

**Design doc:** `docs/plans/2026-04-21-search-quality-hardening-design.md` (rev 8, approved).

---

## Execution Order and Batching

Task 6 (new `trace.rs` types) is a blocker for Tasks 4, 7, 8, 9, 10, 11. Task 3 (OR-fallback diagnosis) is a logical gate for Tasks 4, 5 — the diagnosis report determines whether §3.2 post-handling requires a fix or is a no-op.

**Batch 1 (parallel, independent starts):**
- Task 1 — `query.rs` multi-pattern parser
- Task 2 — `execution.rs` boundary normalization
- Task 3 — `line_mode.rs` + `search/index.rs` OR-fallback instrumentation + diagnosis report
- Task 6 — `trace.rs` new types (BLOCKER for Batch 2)

**Batch 2 (starts after Task 6 lands):**
- Task 4 — `line_mode.rs` zero_hit_reason per-stage attribution (needs Task 3)
- Task 5 — `line_mode.rs` second-pass filter investigation (needs Task 3)
- Task 7 — `execution.rs` + `mod.rs` single-identifier promote logic
- Task 8 — `mod.rs` multi-token hint formatter
- Task 9 — `execution.rs` drop content score
- Task 11 — `routes/search.rs` dashboard compile-fix

**Batch 3 (starts after Batch 2):**
- Task 10 — `search_telemetry.rs` persistence of new fields
- Task 13 — skill doc + tool description update

**Batch 4 (validation, lead-owned):**
- Task 12 — replay fixture, 24h diagnosis report, acceptance replay

**TDD expectation (stated once):** Every task follows RED → GREEN → commit. Write the failing test first, verify it fails with the expected error, implement minimal code to pass, verify green with `cargo nextest run --lib <test_name>`. Worker subagents run the narrow test only; the lead runs `cargo xtask test changed` between batches and `cargo xtask test dev` once per batch complete.

---

## Task 1: Multi-pattern `file_pattern` parser

**Files:**
- Modify: `src/tools/search/query.rs:13-68` (`matches_glob_pattern`)
- Test: `src/tests/tools/search/file_pattern_tests.rs` (new)

**What to build:** Extend `matches_glob_pattern` to accept comma-separated and brace-expanded multi-pattern forms. Preserve literal-space globs. Introduce a private `compile_patterns(pattern: &str) -> CompiledPatterns` helper that splits on top-level commas only (brace-aware) and returns inclusion/exclusion lists.

**Approach:**
- Top-level comma split walks the string tracking brace depth — do not split commas inside `{...}`.
- Patterns starting with `!` go into the exclusions list (strip the `!` before compiling).
- Match semantics: `(any(inclusions).matches(path) || inclusions.is_empty()) && !any(exclusions).matches(path)`. Inclusions-empty means "implicit include-all" — preserves `!docs/**` = "match everything except docs".
- Both-empty branch is unreachable by construction (Task 2 normalizes empty/whitespace to `None`); panic or return an error from `compile_patterns` for diagnostic clarity.
- Whitespace-separated globs stay as a single literal pattern (same semantics as today — matches nothing). Do NOT split on whitespace.

**Acceptance criteria:**
- [ ] `"src/database/*.rs,src/database/**/*.rs"` matches `src/database/workspace.rs` (new OR form)
- [ ] `"{src/**,tests/**}"` matches either tree (brace form)
- [ ] `"!docs/**,src/**"` matches `src/lib.rs`, does not match `docs/README.md` (mixed include/exclude)
- [ ] `"!docs/**"` alone matches `src/lib.rs`, does not match `docs/README.md` (exclusion-only)
- [ ] `"**/file name.rs"` matches a path with literal space (pinned regression test at `src/tests/integration/search_regression_tests.rs:253-260` stays green)
- [ ] `"a/** b/**"` (whitespace-separated) returns false for every file — same as today (literal pattern, no match)
- [ ] Tests pass, committed

---

## Task 2: `file_pattern` boundary normalization in `execute_search`

**Files:**
- Modify: `src/tools/search/execution.rs:45-55` (`execute_search`)
- Test: `src/tests/tools/search/file_pattern_tests.rs` (extend from Task 1)

**What to build:** Normalize empty or whitespace-only `file_pattern` to `None` at the shared entry point `execute_search`. This covers every caller: `FastSearchTool`, dashboard route, compare bench.

**Approach:**
- At the top of `execute_search`, before dispatching to `execute_definition_search` / `execute_content_search`, rebind `params.file_pattern`: if `Some(s)` where `s.trim().is_empty()`, treat as `None`.
- Since `SearchExecutionParams` holds `file_pattern: &'a Option<String>`, create a normalized owned `Option<String>` at the call site and pass a reference to the inner functions. Use a local variable.
- Do NOT normalize at individual caller entries; the design (rev 6) explicitly consolidates this at the shared entry.

**Acceptance criteria:**
- [ ] `FastSearchTool { file_pattern: Some("".to_string()) }` returns identical result set to `file_pattern: None`
- [ ] Dashboard search execution with `file_pattern=""` returns identical result set to omission
- [ ] Dashboard search execution with `file_pattern="   "` returns identical result set to omission
- [ ] Values tested: `""`, `"   "`, `"\t"`, `"\n"`
- [ ] Narrow tests assert on execution semantics (result sets), not UI display
- [ ] Tests pass, committed

---

## Task 3: OR-fallback instrumentation and diagnosis report

**Files:**
- Modify: `src/tools/search/line_mode.rs:73-289` (`line_mode_matches`)
- Modify: `src/search/index.rs:514-582` (`SearchIndex::search_content`) — add instrumentation to return AND-candidate-count and OR-candidate-count
- Test: `src/tests/tools/search/line_mode_or_fallback_tests.rs` (new)
- Create: `docs/plans/2026-04-21-search-quality-hardening-diagnosis.md` (diagnosis report artifact)

**What to build:** Instrument `SearchIndex::search_content` and `line_mode_matches` to record per-stage candidate counts. Replay the 44 captured content zero-hits from 24h telemetry against the instrumented build. Produce a diagnosis report classifying each zero-hit by the last stage that reduced the set to zero.

**Approach:**
- Extend `ContentSearchResults` (`src/search/index.rs:126-130`) with `and_candidate_count: usize` and `or_candidate_count: usize` fields. Populate in `search_content`.
- Extend `LineModeSearchResult` (`src/tools/search/line_mode.rs:17-22`) with counters for each filter stage (file_pattern drop, language drop, test drop, content-unavailable drop, line-miss drop).
- Narrow fixture test: File A = `{x, y}`, File B = `{y, z}`, no file contains all three of `{x, y, z}`. Query `"x y z"` under FileLevel strategy. Assert `and_candidate_count == 0 && or_candidate_count > 0 && relaxed == true`.
- After instrumentation lands, run the zero-hit replay from Task 12's fixture and commit the diagnosis report to `docs/plans/2026-04-21-search-quality-hardening-diagnosis.md`. The report is a table: query → last-draining-stage → count.
- If the report shows a class where OR should have fired but didn't, file a follow-up in Task 3 itself: fix the gate in `SearchIndex::search_content`. If OR fires correctly everywhere, §3.2 post-handling is a no-op and the telemetry is the deliverable.

**Acceptance criteria:**
- [ ] `ContentSearchResults` exposes AND/OR candidate counts; unit test confirms they populate correctly
- [ ] `LineModeSearchResult` exposes per-stage drop counters; unit test for each stage
- [ ] Fixture test (3 tokens, no single file contains all three) passes: `and_candidate_count == 0 && or_candidate_count > 0`
- [ ] Diagnosis report committed at `docs/plans/2026-04-21-search-quality-hardening-diagnosis.md` with counts per zero-hit reason for all 44 captured content zero-hits
- [ ] If report reveals OR-gate bug, fix landed in same PR; otherwise explicit "no-op confirmed" statement in report
- [ ] Tests pass, committed

---

## Task 4: Per-stage `zero_hit_reason` attribution in `line_mode_matches`

**Files:**
- Modify: `src/tools/search/line_mode.rs:73-289` (populate the reason based on per-stage tracking)
- Modify: `src/tools/search/execution.rs:106-164` (propagate `LineModeSearchResult.zero_hit_reason` into `SearchTrace.zero_hit_reason`)
- Test: `src/tests/tools/search/line_mode_zero_hit_reason_tests.rs` (new)

**Depends on:** Task 3 (per-stage instrumentation), Task 6 (`ZeroHitReason` enum defined on `trace.rs` and `SearchTrace.zero_hit_reason` field).

**What to build:** When `line_mode_matches` returns zero matches, populate the `zero_hit_reason` enum value indicating the last filter stage that dropped the set. Propagate it from `LineModeSearchResult` into `SearchTrace.zero_hit_reason` in `execute_content_search`.

**Approach:**
- Add `zero_hit_reason: Option<ZeroHitReason>` field to `LineModeSearchResult` (line_mode.rs:17-22). This is on top of the per-stage counters from Task 3; the reason is the post-hoc attribution, the counters are the raw data.
- In `line_mode_matches`, track which stage reduced the candidate set to the first zero. Algorithm: walk the stages top-down (Tantivy, file_pattern, language, test, content-available, line-match); the first stage where `count_before > 0 && count_after == 0` wins.
- In `execute_content_search`, after the `line_mode_matches` call completes with zero matches, copy `result.zero_hit_reason` into `SearchTrace.zero_hit_reason`.
- `SearchExecutionKind::Content` and `Promoted` paths both surface the reason; `Definitions` leaves it `None`.
- Narrow tests drive each enum variant: construct a fixture that drops at exactly one stage; assert `zero_hit_reason == <expected>`.

**Acceptance criteria:**
- [ ] Narrow test per enum variant (6 variants: `TantivyNoCandidates`, `FilePatternFiltered`, `LanguageFiltered`, `TestFiltered`, `FileContentUnavailable`, `LineMatchMiss` — `Promoted` is set by Task 7, not here)
- [ ] `LineModeSearchResult` exposes the drop-stage reason when `matches.is_empty()`
- [ ] `SearchExecutionResult.trace.zero_hit_reason` populated for every zero-result content search
- [ ] Tests pass, committed

---

## Task 5: Second-pass filter investigation in `line_mode_matches`

**Files:**
- Modify: `src/tools/search/line_mode.rs:263-281` (the post-filter block)
- Test: existing `line_mode_*` tests stay green; may add a narrow test for the removed path if it was redundant

**Depends on:** Task 3 instrumentation.

**What to build:** Investigate whether the second-pass filter at `line_mode.rs:263-281` is redundant with the per-file loop filters at lines 146, 152, 158. Either remove it (with a regression test confirming no behavior change) or keep it with a code comment explaining why it is necessary.

**Approach:**
- Trace every path through `line_mode_matches` to determine whether any match can reach line 263 without having passed the per-file loop filters at 146/152/158.
- If redundant: delete lines 263-281. Add a narrow test reproducing a case the second pass handled; confirm it still passes without the second pass.
- If necessary (because of, e.g., the target-workspace branch at line 245 that uses different filters): add a code comment at the second-pass block explaining the specific case it covers. The second pass reapplies language, file_pattern, test filters, so any second-pass drop maps to the existing `LanguageFiltered` / `FilePatternFiltered` / `TestFiltered` variants from Task 6 — no new variant needed.

**Acceptance criteria:**
- [ ] Either: second-pass filter removed, narrow test confirms no regression in pre-existing behavior
- [ ] Or: second-pass filter retained with a code comment identifying the specific case it covers
- [ ] `cargo xtask test changed` green on the `search_quality` bucket after the change
- [ ] Tests pass, committed

---

## Task 6: Composite `SearchExecutionKind::Promoted` variant, `HintKind`, and `ZeroHitReason`

**Files:**
- Modify: `src/tools/search/trace.rs:115-165`
- Test: `src/tests/tools/search/promotion_tests.rs` (new)

**What to build:** Add `Promoted` variant to `SearchExecutionKind`, add `PromotionInfo` struct, extend `SearchTrace` with `promoted`, `zero_hit_reason`, `hint_kind` fields. Add `HintKind` and `ZeroHitReason` enums.

**Approach:**
- Extend `SearchExecutionKind` (line 149-155):
  ```
  pub enum SearchExecutionKind {
      Definitions,
      Content { workspace_label: Option<String>, file_level: bool },
      Promoted {
          requested_target: String,
          effective_target: String,
          requested_result_count: usize,
          effective_result_count: usize,
          promotion_reason: String,
          inner_content: Box<SearchExecutionKind>,
          inner_definitions: Box<SearchExecutionKind>,
      },
  }
  ```
- Extend `SearchTrace` (line 115-119): add `promoted: Option<PromotionInfo>`, `zero_hit_reason: Option<ZeroHitReason>`, `hint_kind: Option<HintKind>`.
- Define `PromotionInfo` as a new struct alongside (fields: `requested_target`, `effective_target`, `requested_result_count`, `promotion_reason`).
- Define `HintKind` enum: `MultiTokenHint`, `OutOfScopeDefinitionHint`, `CommaGlobHint`. `#[derive(Serialize)]` + `#[serde(rename_all = "snake_case")]`.
- Define `ZeroHitReason` enum: `TantivyNoCandidates`, `FilePatternFiltered`, `LanguageFiltered`, `TestFiltered`, `FileContentUnavailable`, `LineMatchMiss`, `Promoted`. `#[derive(Serialize)]` + `#[serde(rename_all = "snake_case")]`.
- Update `SearchTrace::from_hits` to leave new fields as `None`; callers populate them based on context (Task 4 sets `zero_hit_reason`, Task 7 sets `promoted` + `hint_kind`, Task 8 sets `hint_kind = MultiTokenHint`).
- Update `SearchExecutionResult::new` signature as needed so existing callers still compile; add a builder method or separate constructor for promoted results.
- Add doc comment explicitly enumerating which paths leave `hint_kind = None` (per design §3.5): "single identifier where symbol doesn't exist anywhere," "content hit with result_count > 0," "definitions search with hits."

**Acceptance criteria:**
- [ ] `SearchExecutionKind::Promoted` carries `requested_result_count`, `effective_result_count`, both inner kinds
- [ ] `SearchTrace` has `promoted`, `zero_hit_reason`, `hint_kind` fields; all serialize correctly
- [ ] `HintKind` enum has exactly three variants (`MultiTokenHint`, `OutOfScopeDefinitionHint`, `CommaGlobHint`); serializes to snake_case
- [ ] `ZeroHitReason` enum has exactly seven variants; serializes to snake_case
- [ ] Existing callers of `SearchExecutionResult::new` still compile (migration path for new fields is `None`)
- [ ] Narrow test constructs a `SearchTrace` with each `HintKind` and `ZeroHitReason` variant; asserts JSON shape
- [ ] Tests pass, committed

---

## Task 7: Single-identifier content → definitions auto-promote

**Files:**
- Modify: `src/tools/search/execution.rs:106-164` (`execute_content_search`)
- Modify: `src/tools/search/mod.rs:127-458` (`execute_with_trace` formatter output)
- Test: `src/tests/tools/search/promotion_tests.rs` (extend from Task 6)

**Depends on:** Task 6 (`Promoted` variant, `PromotionInfo`, `HintKind`).

**What to build:** When `execute_content_search` returns zero hits AND the query is a single identifier-shaped token, internally call `execute_definition_search` with the same filters and wrap results in `SearchExecutionKind::Promoted`. Formatter prepends a fixed header to the agent-facing output.

**Approach:**
- Firing rule (strict, per §3.4):
  - Query after trim has no whitespace
  - Matches `^[A-Za-z_][A-Za-z0-9_]*$`, `^[A-Z][A-Za-z0-9_]*$`, or `^[A-Za-z][A-Za-z0-9_]*(-[A-Za-z][A-Za-z0-9_]*)+$`
  - Length ≥ 3
  - Not in keyword deny-list: `impl, fn, class, def, async, public, private, static, const, let, var, function, method, struct, enum, trait, type, module, mod, use, import, from, as, return, if, else, for, while, loop, match, case, switch, break, continue, true, false, null, none, void, self, this, super`
  - `line_mode_matches` returned `result_count = 0` (any `zero_hit_reason` qualifies)
- Decision tree (per §3.6):
  1. Run `execute_definition_search` with caller's `file_pattern` unchanged.
  2. If ≥ 1 hit inside the filter: emit `Promoted` with `hint_kind = None`, `promotion_reason = "single_identifier_content_zero_hit"`.
  3. If 0 hits inside filter: run second scope-free definitions query (drop `file_pattern`).
     - If hit outside filter: emit zero-hit response with `hint_kind = OutOfScopeDefinitionHint` and advisory hint text (location reference only, not in `hits[]`).
     - If no hit anywhere: emit zero-hit response with `hint_kind = None` (truly no recourse; symbol doesn't exist).
- Formatter branch in `src/tools/search/mod.rs` output formatter: when `kind` is `Promoted`, prepend header:
  ```
  (promoted from content → definitions because your query looks like a symbol name;
   0 literal content matches for "<query>" with file_pattern=<pattern>)
  ```
  Followed by the normal definitions output.
- Narrow tests:
  - `"SpilloverStore"` with `file_pattern = "src/tools/spillover/store.rs"` (assume definition is there) → promoted with `hint_kind = None`
  - `"SpilloverStore"` with `file_pattern = "src/tests/**"` (definition outside filter) → zero-hit with `hint_kind = OutOfScopeDefinitionHint` and location hint in message
  - `"impl"` (keyword) → no promotion
  - `"Xyzzy123"` (nonexistent) → zero-hit with `hint_kind = None`
  - `"some function"` (multi-token) → no promotion (falls through to Task 8)

**Acceptance criteria:**
- [ ] Promoted result fires for in-scope single-identifier case; trace has `promoted = Some(...)` and `hint_kind = None`
- [ ] Out-of-scope case emits hint text + trace has `hint_kind = OutOfScopeDefinitionHint`
- [ ] Nonexistent-symbol case emits zero-hit + trace has `hint_kind = None` and `promoted = None`
- [ ] Keyword deny-list blocks promotion on all listed keywords
- [ ] Multi-token queries fall through without attempting promotion
- [ ] Formatter produces the specified header for `Promoted` kind
- [ ] Tests pass, committed

---

## Task 8: Multi-token content zero-hit informative hint

**Files:**
- Modify: `src/tools/search/mod.rs` formatter path (reachable from `execute_with_trace`)
- Test: `src/tests/tools/search/promotion_tests.rs` (extend)

**Depends on:** Task 6 (`HintKind`), Task 4 (`zero_hit_reason`).

**What to build:** When content search returns zero hits on a query with ≥ 2 whitespace-separated tokens AND no single-identifier promotion fired, return the structured hint message template from §3.7.

**Approach:**
- Fires after Task 7's single-identifier path returns "not applicable" (multi-token).
- Message template (verbatim from §3.7):
  ```
  0 content matches for "<query>" with file_pattern=<pattern>.

  Content search requires all tokens on the same line (under Tokens strategy) or the same file (under FileLevel strategy). Multi-token zero-hits usually mean:
  - Concept query → try: get_context(query="<query>")
  - Symbol lookup → try: fast_search(query="<single_token>", search_target="definitions")
  - Literal phrase → drop to 1-2 key tokens

  Tokens: [<token_1>, <token_2>, ...]
  Strategy used: <FileLevel|Tokens|Substring>
  Filters: file_pattern=<pattern>, language=<lang>, exclude_tests=<bool>
  Zero-hit reason: <from zero_hit_reason enum>
  ```
- Populate tokens via `CodeTokenizer` (same tokenizer the index uses) so the agent sees what the search actually looked for.
- Trace: set `hint_kind = MultiTokenHint`.

**Acceptance criteria:**
- [ ] Multi-token zero-hit produces the templated message including tokens, strategy, filters, zero_hit_reason
- [ ] Trace has `hint_kind = MultiTokenHint`
- [ ] Tokens in message match `CodeTokenizer` output for the query
- [ ] Single-token queries do NOT hit this path (they go through Task 7)
- [ ] Tests pass, committed

---

## Task 9: Drop the fake content-hit score

**Files:**
- Modify: `src/tools/search/execution.rs:138-141` (synthetic score assignment)
- Modify: `src/tools/search/trace.rs` — `SearchTrace::from_hits` (if it treats content score specially)
- Test: extend existing execution tests or add narrow test

**Depends on:** Task 6 (only because it touches trace.rs).

**What to build:** Stop populating `SearchHit.score` with `result_count - rank`. Set to `0.0` (or `f32::NAN` if downstream code tolerates it). Update `SearchTrace::from_hits` so content-hit summaries reflect the change.

**Approach:**
- Replace line 138-141 in `execute_content_search`:
  ```rust
  let workspace_total = result.matches.len().max(1) as f32;
  for (idx, line_match) in result.matches.into_iter().enumerate() {
      let score = workspace_total - idx as f32;
  ```
  with:
  ```rust
  for line_match in result.matches.into_iter() {
      let score = 0.0_f32;
  ```
- Document in a single code comment near the line that this is intentional (no per-line BM25 available yet; real scoring tracked in the deferred dashboard doc).
- Verify no downstream code panics or misbehaves on `0.0` content scores. Compare bench `expected_rank` logic (`src/dashboard/search_compare.rs`) uses position in result vector, not score, so should be unaffected. Confirm during the test run.

**Acceptance criteria:**
- [ ] Content hits return `score = 0.0` instead of synthetic `result_count - rank`
- [ ] Narrow test: run a content search returning 5 hits; assert all scores are 0.0 and none are distinct
- [ ] Dashboard still renders without panicking (`cargo xtask test changed` passes affected buckets)
- [ ] Tests pass, committed

---

## Task 10: Telemetry persistence of new trace fields

**Files:**
- Modify: `src/handler/search_telemetry.rs:8-35` (`fast_search_metadata`)
- Test: `src/tests/integration/search_telemetry_roundtrip_tests.rs` (new or extend existing telemetry tests)

**Depends on:** Tasks 4, 6, 7 (fields must exist on `SearchTrace` and `SearchExecutionResult`).

**What to build:** Extend `fast_search_metadata` to include `promoted`, `zero_hit_reason`, `hint_kind`, per-target result counts in the JSON blob persisted to `tool_calls.metadata`.

**Approach:**
- Read the existing `fast_search_metadata` function to understand the current JSON shape. Add new keys under `trace`:
  - `promoted`: full `PromotionInfo` if present, else absent. Serialize `requested_target`, `effective_target`, `requested_result_count`, `promotion_reason`.
  - `zero_hit_reason`: snake_case string if present
  - `hint_kind`: snake_case string if present
  - For promoted results, the existing `result_count` field already equals the effective (definition-side) count; the `Promoted` variant's `requested_result_count` = 0 is carried in `promoted.requested_result_count`. This gives dashboards the content-side zero + definition-side count they need for honest promotion counting without adding parallel fields.
- Narrow test: construct a mock `SearchExecutionResult` with each variant combination, call `fast_search_metadata`, assert the resulting JSON has the expected keys with the expected values.
- Round-trip test: persist the metadata, read it back from daemon.db, verify structured fields are recoverable via SQL (`json_extract(metadata, '$.trace.hint_kind')`).

**Acceptance criteria:**
- [ ] `fast_search_metadata` includes `trace.promoted` (with all `PromotionInfo` fields), `trace.zero_hit_reason`, `trace.hint_kind`
- [ ] Promoted results expose both side counts: `trace.result_count` (effective) and `trace.promoted.requested_result_count` (content side, = 0)
- [ ] Narrow test verifies each field survives JSON round-trip
- [ ] SQL query `SELECT COUNT(*) FROM tool_calls WHERE json_extract(metadata, '$.trace.hint_kind') IS NULL AND json_extract(metadata, '$.trace.promoted') IS NULL AND json_extract(metadata, '$.trace.result_count') = 0` returns the without-recourse count correctly on a synthetic fixture
- [ ] Tests pass, committed

---

## Task 11: Dashboard route compile-fix for `Promoted` variant

**Files:**
- Modify: `src/dashboard/routes/search.rs:209-231` (`normalize_dashboard_results`) — the exhaustive match on `SearchExecutionKind`
- Test: minimal smoke test that dashboard search executes without panicking on a Promoted result

**Depends on:** Task 6 (`Promoted` variant must exist).

**What to build:** Extend the exhaustive `match result.kind` to handle `Promoted { inner_definitions, .. }` by passing through using the existing definitions rendering path. Compile-fix only — no template work, no new badge.

**Approach:**
- The current match at line 220-228 handles `Definitions` and `Content`. Add a third arm for `Promoted`:
  - Pull `inner_definitions` out of the `Promoted` variant, reuse the definitions rendering branch for the hits.
  - The `strategy_id` string (e.g., `"fast_search_content_promoted"`) flows through the existing strategy display field.
- Do NOT add template changes. Do NOT add a visible badge. UI polish is explicitly deferred to the dashboard follow-up design doc.
- Narrow test: construct a mock `SearchExecutionResult` with `kind = Promoted { ... }`, pass through `normalize_dashboard_results`, assert no panic and hits array is populated from `inner_definitions`.

**Acceptance criteria:**
- [ ] `src/dashboard/routes/search.rs` compiles with the new `Promoted` variant
- [ ] Promoted result flows through `normalize_dashboard_results` without panic
- [ ] Hits in the rendered payload come from `inner_definitions`
- [ ] No template file is modified
- [ ] Tests pass, committed

---

## Task 12: Zero-hit replay fixture and acceptance validation

**Files:**
- Create: `fixtures/search-quality/zero-hit-replay.json` (48 captured zero-hit queries from 24h telemetry)
- Create: `src/tests/integration/zero_hit_replay_tests.rs` (new, ignored by default, run explicitly for validation)
- Update: `docs/plans/2026-04-21-search-quality-hardening-diagnosis.md` with final replay counts

**Depends on:** Tasks 1-11 all landed.

**What to build:** Extract the 48 zero-hit queries from `~/.julie/daemon.db` (persisted in the 24h telemetry window) into a JSON fixture. Write an integration test that replays them against the patched Julie and reports the raw zero-hit rate and the without-recourse rate.

**Approach:**
- SQL extract from daemon.db:
  ```
  SELECT metadata FROM tool_calls
  WHERE tool_name = 'fast_search'
    AND json_extract(metadata, '$.trace.result_count') = 0
    AND timestamp > <24h-ago-epoch>
  ```
- Fixture format: JSON array of objects `{ query, search_target, file_pattern, language, exclude_tests, workspace_id }`.
- Integration test (`#[ignore]` by default to keep `cargo xtask test dev` fast) loads the fixture, runs `FastSearchTool` against the current workspace for each entry, aggregates:
  - raw zero-hit rate: `count(result_count == 0) / 48`
  - without-recourse rate: `count(result_count == 0 && hint_kind IS NULL && promoted IS NULL) / 48`
- Test asserts raw ≤ 0.20 and without-recourse ≤ 0.08. Fails the build if acceptance targets are missed.
- Update the diagnosis report with pre-change vs post-change counts per `zero_hit_reason`.
- The test is explicitly runnable with `cargo nextest run --lib zero_hit_replay -- --ignored`.

**Acceptance criteria:**
- [ ] Fixture committed at `fixtures/search-quality/zero-hit-replay.json` with all 48 captured queries
- [ ] Integration test `zero_hit_replay_tests` runs under `--ignored`, asserts both metric targets
- [ ] Raw content zero-hit rate ≤ 20% on the fixture
- [ ] Without-recourse rate ≤ 8% on the fixture
- [ ] Diagnosis report updated with before/after counts per reason
- [ ] Tests pass, committed

---

## Task 13: Tool description and skill doc update

**Files:**
- Modify: `src/tools/search/mod.rs:45-83` — `FastSearchTool` struct's schemars doc attributes
- Modify: `.claude/skills/search-debug/SKILL.md`

**What to build:** Update `fast_search` agent-facing description (within MCP 2k instruction limit) to state the target-selection guidance. Update `search-debug` skill to reflect the new behaviors.

**Approach:**
- `fast_search` description additions (terse, within 2k):
  - Single identifier (no whitespace, symbol-shaped) → `search_target="definitions"`
  - Phrase / multi-token literal → `search_target="content"` with specific `file_pattern`
  - Concept query → use `get_context` instead
- `search-debug` skill updates:
  - New labeled auto-promotion: single-identifier content→definitions
  - Zero-hit hint format includes `zero_hit_reason`
  - `file_pattern` multi-pattern syntax: comma-separated or brace-expanded ONLY. Whitespace-separated globs are invalid and emit `CommaGlobHint`.
  - Out-of-scope definition hint format (location reference, not a hit)
- Verify total tool description size stays under 2000 chars (MCP limit; see `project_mcp_instruction_limit` memory).

**Acceptance criteria:**
- [ ] `fast_search` tool description mentions the three query shapes and their target choices
- [ ] Total MCP tool description size < 2000 chars (verify with script or manual count)
- [ ] `search-debug` skill covers all four new behaviors
- [ ] Plugin repo updated: copy `search-debug/SKILL.md` to `~/source/julie-plugin/skills/search-debug/SKILL.md`
- [ ] No stale examples referencing whitespace-separated globs as valid
- [ ] Committed

---

## Acceptance Criteria Rollup (from design §5)

Aggregated for final sign-off:

- [ ] Comma form: `fast_search(query="delete_orphaned_files_atomic", search_target="definitions", file_pattern="src/database/*.rs,src/database/**/*.rs")` returns the symbol
- [ ] Whitespace form `"src/database/*.rs src/database/**/*.rs"` returns zero with `hint_kind = CommaGlobHint`
- [ ] Literal-space regression test at `src/tests/integration/search_regression_tests.rs:253-260` green
- [ ] `CommaGlobHint` heuristic: `"a/** b/**"` fires; `"**/file name.rs"` and `"My Project/src/**"` do NOT
- [ ] Empty/whitespace `file_pattern` normalization at `execute_search` covers MCP, dashboard, compare bench (values `""`, `"   "`, `"\t"`, `"\n"`)
- [ ] `src/dashboard/routes/search.rs` compiles with `Promoted` variant; variant flows through without panic; visible-badge out of scope
- [ ] OR-fallback diagnosis report produced at `docs/plans/2026-04-21-search-quality-hardening-diagnosis.md`
- [ ] Fixture test: 3-token query, no single file contains all → ≥1 candidate with `relaxed=true`
- [ ] `LineModeSearchResult` exposes drop-stage reason; per-variant narrow tests
- [ ] Second-pass filter investigated: removed or commented with justification
- [ ] Auto-promote fires for single-identifier in-scope case; hint fires for out-of-scope; no fire for keyword
- [ ] `SearchExecutionKind::Promoted` carries requested + effective counts; persisted to telemetry
- [ ] Multi-token zero-hit response includes template hint; trace persists `hint_kind = MultiTokenHint`
- [ ] Out-of-scope definition hint persists `hint_kind = OutOfScopeDefinitionHint`
- [ ] Nonexistent identifier zero-hit persists `hint_kind = None`
- [ ] `hint_kind` survives JSON round-trip; without-recourse SQL metric computable
- [ ] `SearchHit.score` on content hits no longer synthetic rank-index
- [ ] `search-debug` skill + `fast_search` description updated
- [ ] 24h telemetry replay: raw ≤ 20%, without-recourse ≤ 8%
- [ ] `cargo nextest run --lib` passes for all new tests
- [ ] `cargo xtask test changed` green on affected buckets
- [ ] `cargo xtask test dev` green once at end of batch

---

## Risks and Rollback

Per design §7:
- **Auto-promote fires on wrong query** — strict firing rule + label + feature flag escape hatch. Mitigation: if dogfooding shows > 2% wrong-promotion rate, the promotion path becomes a no-op via the flag.
- **OR-fallback diagnosis finds nothing to fix** — acceptable outcome. Telemetry is the deliverable.
- **Content-score drop breaks consumers** — dashboard compare bench relies on position, not score, so should be unaffected. Verify during Task 9.

Rollback: every task commits atomically. Any single task can be reverted independently without breaking the others (tasks after Task 6 require the type system to be in place, but each subsequent task adds behavior on top of the type, not replaces it).

---

## Handoff

Execution skill: `razorback:team-driven-development` on Claude Code. File-ownership split for 3 teammates:
- **Teammate A**: Tasks 1, 2, 7 (exec), 9 — owns `query.rs`, `execution.rs`
- **Teammate B**: Tasks 3, 4, 5 — owns `line_mode.rs`, `search/index.rs` (instrumentation only)
- **Teammate C**: Tasks 6, 8, 11 — owns `trace.rs`, `mod.rs` (formatter), `routes/search.rs`

Lead owns: Tasks 10 (telemetry integration), 12 (replay validation), 13 (docs/skill).

Ordering: Teammate C starts with Task 6 as a blocker; Teammates A and B run their independent Tasks 1/2 and 3 in parallel. Once Task 6 lands, Teammates A and B pick up their dependent tasks.

Test command contract: workers use `cargo nextest run --lib <narrow_test_name>` only. Lead runs `cargo xtask test changed` between batches and `cargo xtask test dev` once at end.
