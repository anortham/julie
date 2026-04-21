# Search Quality Hardening Design

**Date:** 2026-04-21
**Status:** Proposed — revision 8 (self-audit pass; awaiting user approval)
**Scope:** Tier 2 (multi-day search hardening)
**Follow-up:** Separate dashboard-metrics fix doc once search is solid
**Revision history:**
- rev 1: initial draft
- rev 2: corrected the live content pipeline target after Codex review identified dead-code targeting; tightened promotion design; reset zero-hit goal to a defensible number; expanded zero-hit-reason telemetry
- rev 3: multi-pattern `file_pattern` now comma/brace only (preserves literal spaces in paths); corrected OR-fallback test to match file-level AND semantics; added `hint_kind` persisted field so the ≤8% without-recourse metric is measurable
- rev 4: dropped unused `KeywordShadowed` variant; tightened `CommaGlobHint` heuristic so it only fires when every whitespace-split piece has glob meta; fixed exclusion-only glob regression (`!docs/**` must still match everything except docs); reconciled §3.10 skill-doc text with §3.1 (whitespace-separated patterns are invalid; commas are the multi-pattern syntax)
- rev 5: normalized empty/whitespace-only `file_pattern` to `None` at the tool boundary so the "both empty" match-semantics branch is unreachable by construction (fixes a silent semantic change from rev 4 where `file_pattern=""` would have flipped from match-nothing to match-everything)
- rev 6: moved empty/whitespace `file_pattern` normalization down to `execute_search` so all callers (FastSearchTool MCP path, dashboard route, compare bench) share the guarantee — not only the MCP tool; added `src/dashboard/routes/search.rs` to the file map and specified rendering policy for the new `SearchExecutionKind::Promoted` variant (its exhaustive match would otherwise refuse to compile)
- rev 7: narrowed two rev-6 claims that overreached into dashboard UI territory. The `file_pattern` normalization guarantee is now execution-only (the page-chip display fix is moved to the dashboard follow-up doc). The `Promoted` variant still gets the compile-fix in `src/dashboard/routes/search.rs` (required, non-optional), but the "visible badge" rendering requirement is moved to the dashboard follow-up doc — rev 7 scope stops at "the exhaustive match compiles and the variant flows through."
- rev 8: self-audit pass (no external review). Fixed stale §4.5 cross-reference in §2 (now §3.7). Removed contradictory `zero_hit_reason` list in §3.4 that excluded filter-stage reasons which §3.6 explicitly handles. Added boundary-normalization change to the `src/tools/search/execution.rs` file-map entry.

---

## 1. Context

### What the 24h telemetry showed

The search observability system shipped 2026-04-20 captured 192 `fast_search` calls across 16 sessions in its first 24 hours. Raw numbers:

| Metric | Value |
|---|---|
| Total fast_search calls (24h) | 192 |
| `fast_search_definitions` zero-hit rate | 8% (4/47) |
| `fast_search_content` zero-hit rate | 30% (44/145) |
| Episodes | 70 |
| Episodes flagged "stalled" by dashboard | 40% |
| Proportion of "stalled" episode searches that actually returned hits | 81% |

The dashboard headlines look alarming. The analysis shows most of the alarm comes from measurement artifacts, but the underlying **content-search zero-hit rate of 30% is a real signal** worth addressing.

### Root-cause breakdown of the 48 zero-hit queries

**Content zero-hits (44), three classes:**

1. **Natural-language queries on content search (~25).** Examples: `blast radius callers grouped by risk`, `token budget allocation truncation compact format deep_dive overview context full`, `alias table OR public API alias OR fast_refs`. Content search requires tokens on one line (or one file, depending on strategy), and these multi-token concept queries have no matching literal lines anywhere.

2. **Single-identifier symbol queries on content search (~10).** Examples: `SpilloverStore`, `linked_tests`, `snapshot_file_hashes_tx`, `run_compare`, `KIND_WEIGHTS`, `identifier_incoming_edges`. Every one of these returns hits under `search_target="definitions"`. The agent chose the wrong strategy; Julie didn't steer.

3. **Tokenizer edge cases (~5).** Examples: `[ ]` (markdown checkboxes, brackets stripped), `sdl-mcp`, `TODO.md`, `from_option(value: Option<&str>) -> Self`. Hyphens, dots, brackets, and Rust-syntax punctuation get split or dropped in ways the agent doesn't anticipate.

**Definitions zero-hits (4):**

- **Silent `file_pattern` bug (1):** `file_pattern = "src/database/*.rs src/database/**/*.rs"` returns 0 for a symbol that exists at `src/database/workspace.rs:12`. The parser treats space-separated globs as a single literal pattern. Verified by rerunning the query with `file_pattern = "src/database/**/*.rs"` → symbol found.
- **Symbol does not exist (1):** `with_store_for_workspace_target` is not in the codebase; zero is correct.
- **Indexing race (2):** `run_pipeline_with_options` and `impact-analysis` both return hits now with the same filters.

### The live content-search pipeline

Critical for this design: the live `fast_search(search_target="content")` path goes through `line_mode_matches`, not the `text_search_impl("content")` branch. Verified by reading source:

- `src/tools/search/execution.rs:50` — `execute_search` routes `search_target="content"` to `execute_content_search`
- `src/tools/search/execution.rs:125` — `execute_content_search` calls `line_mode::line_mode_matches`
- `src/tools/search/line_mode.rs:127,209` — `line_mode_matches` calls `SearchIndex::search_content` on the Tantivy index directly and consumes `relaxed` from the Tantivy result
- `src/tools/search/line_mode.rs:146,152,158` — filters candidate files by `matches_glob_pattern`, `file_matches_language`, `is_test_path`
- `src/tools/search/line_mode.rs:164-172,245-253` — reads file content via `db.get_file_content`, extracts lines with `collect_line_matches` + `line_matches` using the strategy from `line_match_strategy(query)`

The `text_search::content_search_with_index` and its post-verifier at `text_search.rs:427-449` are only exercised in tests today. `text_search_impl` is only called with `target="definitions"` in production (`execute_definition_search` at `execution.rs:71`). All design work in this doc targets the `line_mode_matches` pipeline, not the dead content branch of `text_search_impl`. See §11 for the follow-up investigation.

### Additional real bug

The content-hit "score" is synthetic. `src/tools/search/execution.rs:138-141`:

```rust
let workspace_total = result.matches.len().max(1) as f32;
for (idx, line_match) in result.matches.into_iter().enumerate() {
    let score = workspace_total - idx as f32;
```

For every content hit, `score = result_count - rank`. It's rank-index, not a relevance signal. Confirmed in telemetry: every content search has `top_hit_score == result_count`.

---

## 2. Goals and non-goals

### Goals

1. Reduce `fast_search_content` zero-hit rate on the 24h telemetry replay fixture. Target: **raw zero-hit rate ≤20%**, down from 30%.
2. Introduce and track a secondary metric — **zero-hit-without-recourse rate** — that excludes zero-hit responses carrying a specific, actionable hint or a labeled target promotion. Target: ≤8% on the same fixture. Measured directly from the new persisted `hint_kind` field (see §3.5) so the metric is computable from `tool_calls.metadata` without parsing response text.
3. Eliminate the silent `file_pattern` parsing failure on space-separated globs.
4. Diagnose and either fix or document why `relaxed=true` fires on only 3 of 145 content searches. The diagnosis itself is the goal; the fix may be a no-op if Tantivy behaves correctly and the low rate is explained by query shape.
5. Make content-search "score" meaningful or stop calling it a score.
6. Teach agents the right calling pattern via labeled auto-promote (format-preserving cases only) and specific error hints (format-changing cases).
7. Add a `zero_hit_reason` field to search telemetry covering every drop point in the live content pipeline.

### Why the target changed from ≤10% to ≤20% (rev 2)

Math: 25 of the 44 content zero-hits are multi-token concept queries. Under the asymmetric forgiveness design (§3.7), those stay as labeled zero-hit responses with a hint to `get_context`. They remain in the raw-zero-hit denominator. That puts a floor of ~17.2% on the raw rate before any other work lands. Claiming ≤10% raw would require either auto-routing concept queries (rejected: format-changing promotion is surprising) or changing the denominator. Neither is clean. Instead we hold the raw target at a defensible ≤20% and introduce the secondary "without-recourse" metric to measure the quality of the hint experience: a zero-hit response that redirects the agent to a specific alternative is a success of a different kind, and we measure it as one.

### Non-goals

- Dashboard metric or template changes. Tracked separately; hide or recolor the existing dashboard in the interim.
- New search modes (`search_target="auto"`, semantic-first, LLM rewrite).
- Tokenizer overhaul. We hint on hyphen/dot behavior; we do not change index-time tokenization in this pass.
- Cross-language tokenizer audits.
- Cleanup of the dead `text_search::content_search_with_index` branch. Flagged in §11; handled separately.

---

## 3. Design

### 3.1 Multi-pattern `file_pattern` with literal-space preservation

`src/tools/search/query.rs::matches_glob_pattern` takes a single pattern string and passes it to `globset::Glob::new`. Agents frequently pass shell-style whitespace-joined patterns (`"a/** b/**"`) expecting OR semantics, and get silent zero matches instead. But whitespace alone is not a safe separator: Windows and macOS paths can contain literal spaces, and the pinned regression test `matches_glob_pattern("\\\\?\\C:\\source\\My Project\\src\\file name.rs", "**/file name.rs")` at `src/tests/integration/search_regression_tests.rs:253-260` must keep passing.

**Fix:** support unambiguous multi-pattern forms that do not conflict with literal spaces.

**Accepted input forms:**
- `"a/**"` → single pattern (unchanged)
- `"a/**,b/**"` → OR of `a/**` and `b/**` (comma-separated)
- `"{a/**,b/**}"` → same, via brace expansion (already supported by globset)
- `"!docs/**,src/**"` → exclusion of `docs/**` AND inclusion of `src/**`
- `"**/file name.rs"` → single glob containing a literal space (unchanged behavior; preserved)

**NOT accepted as multi-pattern:**
- `"a/** b/**"` (whitespace-separated) — kept as a single literal pattern; matches nothing, same as today. But the response changes: on zero-hit with this shape, the trace emits `hint_kind = CommaGlobHint` and the message includes a targeted note: `did you mean a comma-separated multi-pattern glob? try "a/**,b/**"`.
- **Detection heuristic (tightened after Codex rev-3 review):** split `file_pattern` on whitespace; fire `CommaGlobHint` only if (a) the split produces ≥ 2 non-empty pieces AND (b) **every** piece contains at least one glob meta-character (`*`, `?`, `[`, `{`). Ordinary paths with literal spaces (e.g., `"**/file name.rs"` splits to `["**/file", "name.rs"]`; piece 2 has no glob meta, so no hint fires) are correctly ignored. A pattern like `"My Project/src/**"` (Windows-style path with a space) splits to `["My", "Project/src/**"]`; piece 1 has no glob meta, no hint fires. Only patterns where every split piece is itself glob-meta-bearing get flagged.

**Internal API:** `pub fn matches_glob_pattern(file_path: &str, pattern: &str) -> bool` stays at the call site. Introduce a private `compile_patterns(pattern: &str) -> CompiledPatterns` that splits on top-level commas only (brace-aware: do not split commas inside `{...}`). Returns two lists (inclusions, exclusions).

**Match semantics (important edge case — from Codex rev-3 review):**
- If inclusions is non-empty AND exclusions is non-empty: `any(inclusions).matches(path) && !any(exclusions).matches(path)`
- If inclusions is non-empty AND exclusions is empty: `any(inclusions).matches(path)`
- If inclusions is empty AND exclusions is non-empty: treat as implicit include-all. Logic is `!any(exclusions).matches(path)`. This preserves the current meaning of `!docs/**` = "match everything except docs".
- Both empty is unreachable by construction (see boundary normalization below), so `compile_patterns` can panic or return an explicit error for diagnostic clarity.

**Boundary normalization (from Codex rev-4 / rev-5 reviews):** empty or whitespace-only `file_pattern` normalizes to `None` before any filter compilation. The normalization lives at the **shared search entry point in `src/tools/search/execution.rs::execute_search`**, not at individual callers. This covers every caller of `execute_search`:
- `FastSearchTool::call_tool` (`src/tools/search/mod.rs`) — MCP tool path
- Dashboard route (`src/dashboard/routes/search.rs:186`) — currently uses `(!file_pattern.is_empty()).then(...)`, which lets whitespace-only strings through as `Some("   ")`. After rev 6 normalization in `execute_search`, the whitespace-only case is handled regardless of how the caller constructed `file_pattern`.
- Compare bench (`src/dashboard/search_compare.rs`) — same shared path.
- Any future caller of `execute_search`.

Today, `matches_glob_pattern("", path)` reaches the simple-filename branch at `query.rs:36-46` and returns `false` for every file. Under rev-6 normalization, `Some("")` or `Some("   ")` becomes `None`, which bypasses the filter entirely (same as no filter). This is the right behavior — an empty filter string is not a meaningful "match nothing" request; it is user error. The normalization avoids a silent semantic flip and keeps `compile_patterns`'s "both empty" branch unreachable by construction.

**Acceptance tests (shared-boundary coverage, execution semantics only):**
- `FastSearchTool { file_pattern: Some("".to_string()) }` returns the same result set as `file_pattern: None` (MCP path, search execution).
- Dashboard search execution with `file_pattern=""` or `file_pattern="   "` returns the same result set as omitting the parameter (dashboard path, search execution).
- Compare bench search execution with `file_pattern=""` similarly normalized.

**Out of scope for this doc (moved to dashboard follow-up):** dashboard-page display consistency when the URL contains `?file_pattern=%20%20`. The chip renderer at `dashboard/templates/partials/search_results.html:9-15` currently shows a blank `pattern:` chip in that case. That is a display-layer fix, not a search-semantics fix; it does not affect what the search engine returns. Tracked in the dashboard fix doc.

**Backwards compatibility:** comma-separated inputs were not a valid single globset pattern before (comma has no glob meaning), so existing valid single patterns are unaffected. Windows/Unix paths with literal spaces work as before. The `**/file name.rs` regression test stays green.

**Validation:** add these test cases:
- `"src/database/*.rs,src/database/**/*.rs"` matches `src/database/workspace.rs` (new OR form)
- `"**/file name.rs"` matches a path with a literal space (regression test pinned)
- `"!docs/**,src/**"` matches `src/lib.rs` and excludes `docs/README.md`
- `"{src/**,tests/**}"` matches either tree
- `"a/** b/**"` (whitespace-separated) returns false with the same semantics as today (literal pattern, no match)

### 3.2 OR-fallback diagnosis on the live content pipeline

Before adding new forgiveness layers, verify that `line_mode_matches` actually exercises OR-fallback when AND returns 0.

**Investigation tasks:**

1. Instrument `SearchIndex::search_content` (the call site in `line_mode.rs:127` and `line_mode.rs:209`) to record:
   - Whether the AND query returned 0 candidates
   - Whether OR fallback was attempted
   - Candidate count returned by OR fallback
   - Whether the query form was eligible for OR (single-token queries may not be)
2. Replay the 44 content zero-hits from the telemetry against the instrumented build.
3. Classify each zero-hit by the last stage that reduced the result set to zero (using the expanded `zero_hit_reason` enum defined in §3.8).
4. If the classification shows a class where OR could have fired but didn't, find the gate in `SearchIndex::search_content` and fix it.
5. If the classification shows a class where OR fired but downstream line-extraction or filtering killed the candidates, address that drop point explicitly (see §3.3).

**Target state:** `relaxed=true` on the trace accurately reflects "AND returned 0 and OR was used to find candidates."

Important Tantivy semantics: `SearchIndex::search_content` (`src/search/index.rs:534-568`) runs AND across the whole file content field, not the line. So if all query tokens appear anywhere in a single file, AND succeeds and `relaxed` stays false. OR fallback only engages when **no file** contains all tokens.

**Narrow test (corrected):** construct a fixture with:
- File A containing tokens `{x, y}` (no `z`)
- File B containing tokens `{y, z}` (no `x`)
- No file containing all three of `{x, y, z}`

Query `"x y z"` under the `FileLevel` strategy. Under AND, Tantivy returns zero file candidates. OR fallback should then return both files. Expected outcome: `result_count > 0` with `relaxed=true`.

**Alternative assertion surface:** once the instrumentation in §3.2 is in place, assert directly on per-stage candidate counts (`and_candidate_count == 0 && or_candidate_count > 0`) regardless of what the line-extraction does afterwards. This makes the test resilient to future changes in `collect_line_matches`.

### 3.3 Instrument line-mode filter stages

The live content path filters candidates at multiple stages inside `line_mode_matches`. Each stage can drop candidates to zero; today the telemetry cannot distinguish which stage did it.

**Stages in `line_mode.rs` (primary and target workspace branches both):**
1. Tantivy `search_content` returned 0 candidate files
2. Candidate files filtered by `file_pattern` via `matches_glob_pattern` (line 146, 228)
3. Candidate files filtered by `language` via `file_matches_language` (line 152, 233)
4. Candidate files filtered by `exclude_tests` via `is_test_path` (line 158, 239)
5. Candidate files had no content available from DB (line 164, 245 — the `if let Some(content) = ...` guard)
6. Files had content but no line matched the strategy in `collect_line_matches` + `line_matches` (line 165, 246)
7. Post-filter at line 263-281 that reapplies language/file_pattern/test filters to line matches (this is a redundant second pass; investigate whether it can be removed)

**Fix:** add a lightweight counter alongside the `matches` accumulator in `line_mode_matches`. Each stage decrements candidate or match counts and records the stage. On zero-result, the `LineModeSearchResult` carries a `zero_hit_reason` enum value indicating the last stage that dropped the set to zero (or `tantivy_no_candidates` if stage 1 already hit zero).

**Second-pass filter decision:** the filter at `line_mode.rs:263-281` reapplies filters that were already applied in the per-file loop. This is defensive but redundant on the happy path. The investigation in §3.2 should confirm whether any match can reach this filter without having passed the per-file loop filters. If not, remove the redundant pass and let candidates fall out at stage 2/3/4 with accurate stage attribution.

### 3.4 Single-identifier content → definitions auto-promote (labeled)

When the live content pipeline returns zero hits (reason: any) AND the query is a single identifier-shaped token, run `execute_definition_search` internally with the same filters and return those results through a new composite result kind.

**Firing rule (strict):**
- Query after trim has no whitespace
- Query matches one of:
  - `^[A-Za-z_][A-Za-z0-9_]*$` (plain word, snake_case)
  - `^[A-Z][A-Za-z0-9_]*$` (CamelCase / PascalCase)
  - `^[A-Za-z][A-Za-z0-9_]*(-[A-Za-z][A-Za-z0-9_]*)+$` (hyphen-joined identifier like `impact-analysis`)
- Length ≥ 3 characters
- Not in a cross-language keyword deny-list: `impl, fn, class, def, async, public, private, static, const, let, var, function, method, struct, enum, trait, type, module, mod, use, import, from, as, return, if, else, for, while, loop, match, case, switch, break, continue, true, false, null, none, void, self, this, super` (seed list; grow with telemetry)
- The live content pipeline returned `result_count = 0` (any `zero_hit_reason` value qualifies — see §3.6 for how filter-stage reasons are handled via the scope-free second query)
- The §3.6 decision tree determines whether the promotion fires (in-scope definition found), a hint fires (out-of-scope definition found), or nothing fires (symbol doesn't exist anywhere)

### 3.5 Composite result type for promoted searches

Codex correctly flagged that the current `SearchExecutionResult` model cannot represent "content miss, definition hits." Changes:

**`src/tools/search/trace.rs`:**

```rust
pub enum SearchExecutionKind {
    Definitions,
    Content { workspace_label: Option<String>, file_level: bool },
    Promoted {
        requested_target: String,      // "content"
        effective_target: String,      // "definitions"
        requested_result_count: usize, // 0
        effective_result_count: usize, // count of definition hits
        promotion_reason: String,      // e.g., "single_identifier_content_zero_hit"
        inner_content: Box<SearchExecutionKind>,      // original Content { ... }
        inner_definitions: Box<SearchExecutionKind>,  // Definitions
    },
}

pub struct SearchTrace {
    pub strategy_id: String,            // reports "fast_search_content_promoted"
    pub result_count: usize,            // effective_result_count (so caller sees N > 0)
    pub top_hits: Vec<SearchHitSummary>,// top definition hits
    pub promoted: Option<PromotionInfo>,// None on non-promoted searches
    pub zero_hit_reason: Option<String>,// populated when result_count = 0
    pub hint_kind: Option<HintKind>,    // populated when zero-hit response carries actionable recourse
}

pub struct PromotionInfo {
    pub requested_target: String,
    pub effective_target: String,
    pub requested_result_count: usize,
    pub promotion_reason: String,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HintKind {
    MultiTokenHint,               // §3.7 — multi-token zero-hit with get_context / single-token alternatives
    OutOfScopeDefinitionHint,     // §3.6 — promotion suppressed because definition lives outside file_pattern
    CommaGlobHint,                // §3.1 — zero-hit with whitespace-joined multi-glob; suggest comma form
}

// Paths that intentionally leave `hint_kind = None`:
// - Single identifier with no definition anywhere in the workspace (symbol doesn't exist; nothing to hint at)
// - Content hit with result_count > 0 (not a zero-hit at all)
// - Searches that found their target via normal path (not a zero-hit)
// The ≤8% zero-hit-without-recourse metric treats `hint_kind = None` AND `promoted = None` AND `result_count = 0`
// as "no recourse offered"; this is a legitimate outcome when the symbol truly doesn't exist.
```

**`src/tools/search/mod.rs` formatter changes:**

- Add a third branch in the output formatter for `SearchExecutionKind::Promoted`. It prepends a fixed header:
  ```
  (promoted from content → definitions because your query looks like a symbol name;
   0 literal content matches for "<query>" with file_pattern=<pattern>)
  ```
- Followed by the normal definitions output (same renderer as the `Definitions` branch).
- The `Content` and `Definitions` formatters are untouched.

**Dashboard route changes (`src/dashboard/routes/search.rs:220-228`) — compile-fix only:**

The existing exhaustive `match result.kind` over `Definitions` and `Content` variants will refuse to compile when `Promoted` is added. Required change in this doc: extend the match to handle `Promoted { ... }` by cloning the variant through to the rendered payload (using the existing definitions template fallback for now). This keeps the dashboard compiling; the result still renders using the existing definitions template plus the strategy_id string (which becomes `fast_search_content_promoted` and is visible in the existing strategy display).

**Out of scope for this doc (moved to dashboard follow-up):**
- A dedicated "promoted" badge or header on the dashboard search playground.
- Custom rendering for the `Promoted` variant in the compare page beyond the strategy_id string.
- Template changes to `dashboard/templates/partials/search_results.html` or `search_compare_results.html`.

These are pure UI polish. Rev 7 stops at "the type system is happy and the variant flows through the route." The MCP tool output (which is where the auto-promote label matters for agents) gets the fully labeled header from `src/tools/search/mod.rs` as described above.

**Telemetry:**
- `handler/search_telemetry.rs` persists `trace.promoted`, `trace.zero_hit_reason`, and `trace.hint_kind` into `tool_calls.metadata` alongside the existing fields. Every persisted field is a structured value — no parsing response text at the dashboard layer.
- Separate counts: `content_result_count` and `definition_result_count` both present in the trace so the dashboard can count promotions honestly.
- The `hint_kind` field is what makes the ≤8% without-recourse metric measurable from captured metadata. See §2.

**Metric definition:**
- Raw zero-hit rate: `count(result_count == 0) / count(total content searches)`
- Zero-hit-without-recourse rate: `count(result_count == 0 AND hint_kind IS NULL AND promoted IS NULL) / count(total content searches)`

A zero-hit response with any `hint_kind` value is considered "with recourse" — the caller received a labeled, actionable redirect.

### 3.6 Auto-promote respects `file_pattern` (revised from rev 1)

Codex pushed back on the rev-1 stance of "include a promoted definition even if `file_pattern` excludes it." Codex is right: a scoped search says something about caller intent. Mutating the result set to escape the scope is a false positive.

**New rule:**
- Auto-promote runs `execute_definition_search` with the caller's `file_pattern` unchanged.
- If definitions search returns ≥ 1 hit inside the filter, the promotion fires. Trace: `hint_kind = None` (this is not a hint — it is a fully promoted result), `promoted = PromotionInfo { ... }`.
- If definitions search returns zero inside the filter, run a second, scope-free definitions query to check whether the symbol exists elsewhere:
  - If a definition exists **outside** the filter: include it in the response as advisory hint text (location reference only, not injected into `hits[]`). Trace: `hint_kind = OutOfScopeDefinitionHint`, `zero_hit_reason = <whatever stage dropped the in-scope result>`.
  - If no definition exists anywhere: return the content zero-hit response as-is. Trace: `hint_kind = None`, `zero_hit_reason = <stage>`. This is a legitimate "no recourse" outcome; the symbol the caller searched for does not exist in the workspace.
- If the query had ≥ 2 tokens and no single-identifier promotion was attempted, fall through to the multi-token hint path in §3.7. Trace: `hint_kind = MultiTokenHint`.

Advisory hint message shape when `OutOfScopeDefinitionHint` fires:
  ```
  0 content matches for "SpilloverStore" with file_pattern="src/tests/**".
  Definition exists outside your filter: src/tools/spillover/store.rs:12.
  Try: fast_search(query="SpilloverStore", search_target="definitions")
  ```
- The out-of-scope location is a **hint**, not a hit. It does not appear in `hits[]`; it lives in the message text only. The agent reads it, decides whether to broaden the filter.

### 3.7 Multi-token content zero-hit → informative hint (no auto-route)

When content search returns zero on a query with ≥ 2 whitespace-separated tokens AND no single-identifier promotion fired, return a structured message instead of the existing terse "No lines found" string.

**Message template:**

```
0 content matches for "<query>" with file_pattern=<pattern>.

Content search requires all tokens on the same line (under Tokens strategy) or the same file (under FileLevel strategy). Multi-token zero-hits usually mean:
- Concept query → try: get_context(query="<query>")
- Symbol lookup → try: fast_search(query="<single_token>", search_target="definitions")
- Literal phrase → drop to 1-2 key tokens

Tokens: [<token_1>, <token_2>, ...]
Strategy used: <FileLevel|Tokens|Substring>
Filters: file_pattern=<pattern>, language=<lang>, exclude_tests=<bool>
Zero-hit reason: <tantivy_no_candidates | file_pattern_filtered | language_filtered | test_filtered | file_content_unavailable | line_match_miss>
```

The message is verbose on purpose. It runs only on zero-hit responses. When this path fires, the trace emits `hint_kind = MultiTokenHint` alongside the applicable `zero_hit_reason`.

### 3.8 Expanded `zero_hit_reason` telemetry

The rev-1 enum did not match the real drop points. Revised enum, pegged to the live pipeline stages in §3.3:

- `tantivy_no_candidates` — Tantivy AND and OR both returned 0 candidate files
- `file_pattern_filtered` — candidates existed but all dropped by `matches_glob_pattern`
- `language_filtered` — candidates existed but all dropped by `file_matches_language`
- `test_filtered` — candidates existed but all dropped by `is_test_path`
- `file_content_unavailable` — candidates passed filters but none had content in DB
- `line_match_miss` — files had content, no line matched the strategy
- `promoted` — content returned zero; promoted result path took over; `result_count > 0`

Every zero-result (or promoted-from-zero) response populates exactly one value. `handler/search_telemetry.rs` persists into `tool_calls.metadata`. Future dashboard work will group by reason.

### 3.9 Content hit "score" — drop

The current `score = result_count - rank` is not a relevance score. Two options considered; recommended is the simpler.

**Recommended: drop content scores.** Stop populating `SearchHit.score` for content hits in `execution.rs:141` (set to `0.0` or `f32::NAN`; downstream treats missing as missing). Update `SearchTrace::from_hits` to omit `score` on content hits. Update dashboard `median_top_score` and `low_score` flag to apply only to definitions strategies (tracked in the dashboard fix doc).

**Alternative (deferred):** real line-level BM25 scoring. Defer — this is its own project, and the dashboard fixes get the same win from dropping the fake score.

### 3.10 Agent-facing tool description and skill doc pass

The `fast_search` tool description on the MCP side determines whether agents pick the right `search_target` by default. Update to explicitly state, within MCP 2k instruction limit:

- Single identifier query (no whitespace, looks like a symbol) → `search_target="definitions"`
- Phrase / multi-token with literal intent (error message, doc comment, log string) → `search_target="content"` with specific file_pattern
- Concept query (describing a feature or behavior) → not `fast_search`; use `get_context`

Update `.claude/skills/search-debug/SKILL.md` to reflect:
- The new labeled auto-promotion behavior (single-identifier content → definitions)
- The new zero-hit hint format with `zero_hit_reason`
- The multi-pattern `file_pattern` syntax: **comma-separated or brace-expanded only**. Whitespace-separated globs (e.g., `"a/** b/**"`) are NOT a multi-pattern form — they are a single literal pattern that matches nothing, and the tool now emits a `CommaGlobHint` pointing at the comma form. Document the comma form as the official way to OR-combine globs.
- The strict `file_pattern` behavior on promotion (definition outside filter is a hint, not a hit)

---

## 4. File map

Updated after Codex review. Bold entries are changed from rev 1.

| File | Change | Section |
|---|---|---|
| **`src/tools/search/line_mode.rs`** | OR-fallback diagnosis instrumentation; per-stage `zero_hit_reason` attribution; second-pass filter investigation | §3.2, §3.3, §3.8 |
| **`src/search/index.rs`** | If OR-fallback gate in `SearchIndex::search_content` needs fixing | §3.2 |
| `src/tools/search/query.rs` | Multi-pattern `file_pattern` parser | §3.1 |
| `src/tools/search/execution.rs` | Boundary normalization of empty/whitespace `file_pattern` to `None` inside `execute_search`; auto-promote logic (calls `execute_definition_search` from content zero-hit); composite `Promoted` kind construction; drop content score | §3.1, §3.4, §3.5, §3.9 |
| `src/tools/search/trace.rs` | New `SearchExecutionKind::Promoted`; `PromotionInfo`; `zero_hit_reason` on `SearchTrace`; score handling | §3.5, §3.8, §3.9 |
| `src/tools/search/mod.rs` | Promoted-kind formatter branch; multi-token hint builder; single-identifier hint builder for out-of-scope case | §3.4, §3.5, §3.6, §3.7 |
| `src/handler/search_telemetry.rs` | Persist `promoted`, `zero_hit_reason`, `hint_kind`, per-target result counts into `tool_calls.metadata` | §3.5, §3.8 |
| **`src/dashboard/routes/search.rs`** | Extend the exhaustive `SearchExecutionKind` match at line 220-228 to handle the new `Promoted` variant; define rendering policy (see below) | §3.5 |
| `.claude/skills/search-debug/SKILL.md` | Skill doc update | §3.10 |
| **`src/tests/tools/search/line_mode_*.rs`** | Narrow tests per §3.2, §3.3, §3.8 | testing |
| `src/tests/tools/search/promotion_tests.rs` (new) | Narrow tests per §3.4, §3.5, §3.6 | testing |
| `src/tests/tools/search/file_pattern_tests.rs` (new) | Narrow tests per §3.1 | testing |
| `fixtures/search-quality/zero-hit-replay.json` (new) | 48 captured zero-hit queries from 24h telemetry | testing |

**Files NOT changed in this pass (explicitly):**
- `src/tools/search/text_search.rs` — dead `content_search_with_index` branch stays as-is; cleanup tracked separately (§11).
- Dashboard templates, `src/dashboard/search_analysis.rs`, `src/dashboard/search_compare.rs` — deferred to a separate dashboard design doc.

---

## 5. Acceptance criteria

Implementer completes the work when all of these hold:

- [ ] `fast_search(query="delete_orphaned_files_atomic", search_target="definitions", file_pattern="src/database/*.rs,src/database/**/*.rs")` returns the symbol under the new comma form. (§3.1)
- [ ] The whitespace form `"src/database/*.rs src/database/**/*.rs"` returns zero hits with `hint_kind = CommaGlobHint` and the message suggesting the comma alternative. (§3.1)
- [ ] Existing regression test `matches_glob_pattern("\\\\?\\C:\\source\\My Project\\src\\file name.rs", "**/file name.rs") == true` still passes — literal-space globs are preserved. (§3.1)
- [ ] Narrow test covers comma-separated OR, brace-expansion `{a,b}`, mixed include/exclude (e.g., `"!docs/**,src/**"`), and **exclusion-only** patterns (e.g., `"!docs/**"` matches `src/lib.rs` and does NOT match `docs/README.md`). (§3.1)
- [ ] Narrow test for `CommaGlobHint` heuristic: `"a/** b/**"` fires the hint; `"**/file name.rs"` and `"My Project/src/**"` do NOT fire the hint. (§3.1)
- [ ] Empty/whitespace `file_pattern` normalization lives in `execute_search` and covers every caller (execution semantics only). Narrow tests:
  - `FastSearchTool { file_pattern: Some("".to_string()) }` returns identical result set to `file_pattern: None`
  - Dashboard search execution with `file_pattern=""` returns identical result set to omission
  - Dashboard search execution with `file_pattern="   "` returns identical result set to omission
  - Values tested: `""`, `"   "`, `"\t"`, `"\n"`
  (§3.1)
- [ ] `src/dashboard/routes/search.rs` exhaustive match on `SearchExecutionKind` compiles with the new `Promoted` variant; the variant flows through the route without panicking. Visible-badge rendering is explicitly out of scope (deferred to dashboard follow-up doc). (§3.5)
- [ ] OR-fallback diagnosis report produced in `docs/plans/2026-04-21-search-quality-hardening-diagnosis.md`: classification of all 44 zero-hit content queries by `zero_hit_reason`, plus counts per reason. (§3.2, §3.8)
- [ ] Narrow test: fixture with three tokens where no single file contains all three (File A: `{x,y}`, File B: `{y,z}`) — a 3-token query returns ≥ 1 candidate with `relaxed=true`. (§3.2)
- [ ] `LineModeSearchResult` exposes the drop-stage reason when `matches.is_empty()`; narrow test for each reason value. (§3.3, §3.8)
- [ ] Redundant second-pass filter at `line_mode.rs:263-281` is investigated: either shown to be redundant and removed, or shown to be necessary with a code comment explaining why. (§3.3)
- [ ] Auto-promote fires for `fast_search(query="SpilloverStore", search_target="content", file_pattern="src/tests/tools/blast_radius_tests.rs")`: if `SpilloverStore` is defined inside `src/tests/tools/blast_radius_tests.rs`, the promotion returns it; if it is defined elsewhere, the response is a hint that references the out-of-scope location. (§3.4, §3.6)
- [ ] Auto-promote does NOT fire for `fast_search(query="impl", search_target="content")` or other keyword-like queries (keyword deny-list check). (§3.4)
- [ ] `SearchExecutionKind::Promoted` carries both `requested_result_count` and `effective_result_count`; trace persists both into `tool_calls.metadata`. (§3.5)
- [ ] Multi-token zero-hit response includes templated hint with tokens, filters, strategy, and `zero_hit_reason`, AND the trace persists `hint_kind = MultiTokenHint`. (§3.7)
- [ ] Out-of-scope definition hint path persists `hint_kind = OutOfScopeDefinitionHint` in the trace. (§3.6)
- [ ] Single-identifier zero-hit when the symbol exists nowhere persists `hint_kind = None` (not a placeholder variant); narrow test with a non-existent identifier confirms this. (§3.5, §3.6)
- [ ] `hint_kind` is persisted into `tool_calls.metadata` by `handler/search_telemetry.rs`; narrow test confirms it survives the round-trip. (§3.5)
- [ ] Zero-hit-without-recourse metric is computable directly from `tool_calls.metadata` via `hint_kind IS NULL AND promoted IS NULL`. Narrow test builds a synthetic set of calls and verifies the SQL. (§3.5)
- [ ] `SearchHit.score` on content hits is not the fake rank-index. (§3.9)
- [ ] `search-debug` skill and `fast_search` tool description reflect the new behaviors. (§3.10)
- [ ] Replay of the 24h telemetry fixture shows:
  - Raw content zero-hit rate ≤ 20% (target; measure actual)
  - Zero-hit-without-recourse rate ≤ 8% (zero-hits minus queries that carry a specific hint or promotion)
- [ ] `cargo nextest run --lib` passes for new and existing tests.
- [ ] `cargo xtask test changed` is green for the affected buckets.

---

## 6. Validation plan

1. **Narrow tests first** (TDD per project convention) for each of §3.1, §3.2, §3.3, §3.4, §3.5, §3.7, §3.8.
2. **Replay fixture**: build `fixtures/search-quality/zero-hit-replay.json` from the 48 captured zero-hit queries in the last 24h. Run replay under the pre-change and post-change builds; diff the zero-hit rates and categorize by `zero_hit_reason`.
3. **OR-fallback diagnosis report**: before implementing §3.3 post-handling changes, produce a short report with counts per `zero_hit_reason` from the instrumented build. This report determines whether any gate in `SearchIndex::search_content` actually needs fixing. The report lives at `docs/plans/2026-04-21-search-quality-hardening-diagnosis.md` and gets committed with the implementation.
4. **`cargo xtask test changed`** after each section lands; `cargo xtask test dev` once at the end of the batch.
5. **Dogfood**: run an agent session issuing the top 10 failure-pattern queries from the telemetry and verify the new behaviors fire as labeled.

---

## 7. Risks

- **Auto-promote fires on the wrong query.** The firing rule is strict: single identifier, keyword deny-list, content AND and OR both zero, definitions has hits inside the same filter. Still, there's a tail risk — a caller intentionally searched content for the string `Foo` to see all mentions and got a silently-labeled definition promotion instead. Mitigation: the label is always shown; telemetry counts promotions; easy to disable with a feature flag if the rate of wrong promotions exceeds 2% in dogfooding.
- **OR-fallback diagnosis finds nothing to fix.** If the 41/44 `relaxed=false` zero-hits are all explained by `tantivy_no_candidates` (AND and OR both legitimately empty), §3.2's fix is a no-op and the zero-hit-reason telemetry is the deliverable. Acceptable outcome.
- **`file_pattern` multi-glob is additive, not breaking.** Comma and brace forms are new and were not previously valid single globs. Whitespace-joined patterns keep the same matching semantics as today (fail-to-match) but gain a `hint_kind = CommaGlobHint` on zero-hit responses. Literal-space paths (e.g., `"**/file name.rs"`) are unaffected. No regressions expected.
- **Content-score drop breaks downstream consumers.** Compare bench and dashboard summary read the score. Mitigation: dashboard changes are a separate doc; compare bench is already documented as unreliable for line mode in the 24h analysis. The drop is the right behavior; downstream code adapts.
- **Dead `content_search_with_index` path keeps diverging.** Tests on the dead branch may pass while the live `line_mode_matches` branch diverges in behavior. This doc does not clean up the dead code; that follow-up is in §11.

---

## 8. Open questions for reviewer

1. **Keyword deny-list scope.** §3.4 seeds with ~30 keywords common to 5+ languages. Is that right, or should we treat this as per-language (use `SymbolKind` from the definitions result as a gate — "if top definition hit is a function/struct, promote; otherwise don't")? The latter is more robust but requires one extra lookup.
2. **Second-pass filter in `line_mode_matches:263-281`.** Investigation in §3.3. If it turns out to be necessary (not redundant), the `zero_hit_reason` logic needs to account for it as a distinct stage. Accept this as a check during implementation rather than a pre-implementation decision.
3. **Score drop breaking changes.** §3.9 sets content `SearchHit.score` to 0 or NaN. Compare bench `expected_rank` logic in `src/dashboard/search_compare.rs` relies on score ordering. Do we want to fix compare bench in the same PR or in the deferred dashboard doc? My lean is deferred, since the dashboard doc already exists as a follow-up and touching compare bench here extends scope.

---

## 9. Out of scope (explicit)

- Dashboard template/color/metric changes (separate design doc to follow).
- `search_target="auto"` or any new search mode.
- Tokenizer overhaul (hyphen/dot preservation at index time).
- Semantic-search integration into `fast_search` by default.
- Cross-workspace search changes.
- Performance tuning for replay-bench.
- Cleanup of dead `text_search::content_search_with_index` path (see §11).

---

## 10. Testing the design against the Codex findings

Codex rev-1 review (2026-04-21) produced five findings. Mapping to rev 2:

| Codex finding | Rev-2 response |
|---|---|
| F1 (critical): plan targeted `text_search::content_search_with_index`, not the live `line_mode_matches` path | §3.2, §3.3, §3.8 rewritten around `line_mode_matches` and `SearchIndex::search_content`. File map updated. Explicit live-pipeline walkthrough added in §1. |
| F2 (high): promotion doesn't fit `SearchExecutionResult` model | §3.5 adds `SearchExecutionKind::Promoted`, `PromotionInfo`, per-target result counts, and a new formatter branch. Trace fields are explicit. |
| F3 (high): ≤10% target unattainable under hint-only design for concept queries | §2 changes target to raw ≤20% and introduces secondary `zero-hit-without-recourse ≤8%` metric. Explicit arithmetic argument in §2. |
| F4 (high): auto-promote escapes `file_pattern` | §3.6 revises the rev-1 stance. `file_pattern` stays strict; out-of-scope definitions are advisory hint text only, not injected into hits. |
| F5 (high): `zero_hit_reason` misses live drop points | §3.8 enum expanded to match each stage in `line_mode_matches`: `tantivy_no_candidates`, `file_pattern_filtered`, `language_filtered`, `test_filtered`, `file_content_unavailable`, `line_match_miss`, `promoted`. |

Codex rev-2 review (2026-04-21) produced three more findings. Mapping to rev 3:

| Codex finding | Rev-3 response |
|---|---|
| F1 (high): rev-2 §3.1 space-splitting breaks literal-space patterns like `"**/file name.rs"`; pinned regression test at `src/tests/integration/search_regression_tests.rs:253-260` | §3.1 switched to comma/brace-only multi-pattern syntax. Literal-space globs remain single patterns, preserving the existing regression test. Whitespace-joined patterns with glob meta now emit `hint_kind = CommaGlobHint` pointing at the comma form. Regression test added to §5 as an acceptance item. |
| F2 (high): rev-2 §3.2 acceptance test used wrong Tantivy semantics (file-level AND, not line-level) | §3.2 target-state test rewritten around a fixture where no file contains all tokens. Alternative assertion surface (instrumented AND/OR candidate counts) added. |
| F3 (medium): rev-2 secondary metric not measurable from persisted fields | §3.5 added `hint_kind: Option<HintKind>` with four variants; `handler/search_telemetry.rs` persists it. Metric definition in §3.5 is now a concrete SQL-shape expression over `tool_calls.metadata`. |

Codex rev-3 review (2026-04-21) produced three more findings. Mapping to rev 4 (verified in rev-4 pass):

| Codex finding | Rev-4 response |
|---|---|
| F1 (high): `HintKind` not exhaustive — single-identifier zero-hit with no definition anywhere has no variant; `KeywordShadowed` defined but never fires | §3.5 drops the unused `KeywordShadowed` variant. §3.6 adds explicit treatment of the "symbol exists nowhere" case: `hint_kind = None` is the correct value (truly no recourse to offer). §3.5 adds a comment block enumerating which paths intentionally set `hint_kind = None`. §5 adds a narrow test for the non-existent-identifier case. |
| F2 (high): `compile_patterns` regresses exclusion-only globs like `!docs/**` | §3.1 "Match semantics" subsection added with explicit case split: inclusions empty + exclusions non-empty → implicit include-all, so `!docs/**` means "match everything except docs" (preserves current behavior). §5 adds an acceptance test for the exclusion-only case. |
| F3 (medium): `CommaGlobHint` heuristic over-broad + §3.10 contradicts §3.1 | §3.1 heuristic tightened: fire only if every whitespace-split piece contains glob meta. `"**/file name.rs"` → no hint; `"My Project/src/**"` → no hint; `"a/** b/**"` → hint fires. §3.10 skill-doc text corrected to match §3.1: whitespace-separated globs are not a valid multi-pattern form; comma is the supported syntax. §5 adds a narrow test for the tightened heuristic. |

Codex rev-4 review (2026-04-21) produced one more finding. Mapping to rev 5:

| Codex finding | Rev-5 response |
|---|---|
| F1 (medium): `"both empty"` branch in §3.1's match-semantics case split is reachable via `file_pattern = ""`; rev-4 would have silently flipped `file_pattern=""` from match-nothing to match-all | §3.1 adds a boundary-normalization rule in `FastSearchTool::call_tool`: empty or whitespace-only `file_pattern` normalizes to `None` before any filter compilation. This makes the `"both empty"` branch unreachable by construction (no silent semantic change). §3.1 adds an acceptance test that `Some("")` behaves identically to `None`. |

Codex rev-5 review (2026-04-21) produced two more findings. Mapping to rev 6:

| Codex finding | Rev-6 response |
|---|---|
| F1 (high): rev-5 normalization at `FastSearchTool::call_tool` does not cover the dashboard route (`src/dashboard/routes/search.rs:186`) which still uses `(!file_pattern.is_empty()).then(...)`, letting whitespace-only strings through | §3.1 moves normalization down to `execute_search` (the shared entry point). Every caller — MCP tool, dashboard route, compare bench — gets the guarantee. §5 acceptance criteria expanded to cover each caller path. |
| F2 (medium): `src/dashboard/routes/search.rs:220-228` has an exhaustive `SearchExecutionKind` match that will refuse to compile with the new `Promoted` variant | §4 file map adds `src/dashboard/routes/search.rs` as a required change. §3.5 adds explicit dashboard rendering policy for `Promoted` (definitions template with a "promoted" badge). §5 adds a compilation-success acceptance item. |

Codex rev-6 review (2026-04-21) produced two more findings. Mapping to rev 7:

| Codex finding | Rev-7 response |
|---|---|
| F1 (medium): `file_pattern` normalization claim overreached into display semantics; dashboard chip at `partials/search_results.html:9-15` still shows whitespace value | §3.1 narrows the claim to **execution semantics only**. §5 acceptance criteria rewritten to "returns identical result set" (not "behaves identically" which includes UI). Chip-display fix moved to dashboard follow-up doc with explicit reference. |
| F2 (medium): "visible promoted badge" in §3.5 contradicts "dashboard templates deferred" in §9 | §3.5 drops the badge requirement and narrows dashboard scope to "compile-fix only." `Promoted` still flows through `src/dashboard/routes/search.rs` to keep the type system happy, but template changes and visible badge are explicitly in the dashboard follow-up doc. §5 acceptance item rewritten to "compiles and flows through without panicking." |

---

## 11. Follow-up: dead code in `text_search.rs`

`text_search::content_search_with_index` and the `content` branch of `text_search_impl` are not reachable from live `fast_search`. `fast_refs` confirms:
- `text_search_impl` has one production caller: `execute_definition_search` at `execution.rs:71`, which only passes `target="definitions"`.
- `content_search_with_index` is called only from within `text_search_impl` when `target="content"`.
- Tests in `src/tests/tools/text_search_tantivy.rs` and `src/tests/tools/search/primary_workspace_bug.rs` still exercise the `content` branch via `text_search_impl`.

**Question for follow-up (not in this doc's scope):**
- Is the `content` branch historical (pre-`line_mode`) dead code, or does something still need it?
- If dead: delete the branch, delete or retarget the tests, and remove `content_search_with_index`. Reduces maintenance surface and prevents the two implementations from drifting.
- If retained for a reason: document the reason in a code comment near the branch.

This cleanup is deferred. Tracked as a separate follow-up issue.
