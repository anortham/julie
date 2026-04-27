# Search Zero-Hit Reduction Plan

**Date:** 2026-04-27
**Metrics basis:** 1,876 fast_search calls with enriched telemetry (824 before file mode, 1,052 after)
**Current true zero-hit rate:** 8.0% (excluding file_pattern_filtered: 15.2% before, 8.0% after)
**Current raw zero-hit rate:** 19.2% (masked by file_pattern_filtered spike)

---

## F1: file_pattern_filtered fallback (58% of remaining zero-hits)

### Problem

When Tantivy finds candidates but `file_pattern` eliminates all of them, the search returns zero results with a text hint ("try broadening that scope"). The hint is good, but returning empty is expensive: the agent burns a tool call, gets nothing, and either retries or gives up.

118 of 202 post-file-mode zero-hits are `FilePatternFiltered` with `NoInScopeCandidates`. These are cases where the search engine found what the agent was looking for, but the file scope was wrong.

### Fix: labeled scope-rescue on FilePatternFiltered + NoInScopeCandidates

Rescue results must be visually and structurally distinct from in-scope hits. Agents ignore text notes when the result list looks normal, so the output format must signal "these are outside your requested scope."

**Where:** `line_mode_matches` in `src/tools/search/line_mode.rs`

**What:**
1. Extract the `spawn_blocking` + `run_line_mode_fetch_loop` block into a helper that takes `file_pattern: Option<&str>` as a parameter (avoids duplicating the closure-heavy block)
2. After the first pass, check: `zero_hit_reason == FilePatternFiltered && file_pattern_diagnostic == NoInScopeCandidates && file_pattern.is_some()`
3. If true, re-run the helper with `file_pattern: None`
4. Add structured fields to `LineModeSearchResult`:
   - `scope_relaxed: bool` (true when rescue fired)
   - `original_file_pattern: Option<String>` (the pattern that produced zero results)
   - `original_zero_hit_reason: Option<ZeroHitReason>` (preserves the root cause)
5. Propagate these through `SearchExecutionResult` / `SearchTrace` so telemetry, dashboard, and MCP output all see them
6. In the formatted output, prepend a labeled header: `"NOTE: 0 matches within file_pattern={original}. Showing {N} results from the full codebase (outside requested scope)."`
7. Do NOT clear `zero_hit_reason` on the trace; set a separate `scope_rescue_count` field so metrics can distinguish "rescued" from "found in scope"

**Cost:** One extra Tantivy query on the ~15% of content calls that currently zero-hit due to file_pattern. P50 cost is ~12ms, but P95/P99 on larger workspaces could be higher since the fallback runs after the initial search + widened probe have already completed. Monitor P95 after shipping.

**Caveat:** Not every `NoInScopeCandidates` means the scope was wrong. Sometimes the answer is genuinely "no." Exact file paths with no glob characters are the strongest signal that the user intended a specific scope. The rescue should still fire (agents benefit from seeing where the term *does* exist), but the output label should be unambiguous that these results are outside the requested scope.

**Sample skew note:** 113/118 of the current `file_pattern_filtered` zeros are on the Julie workspace during two development days (April 22-23). Cross-workspace evidence is thin (5 non-Julie zeros). Treat F1 as a UX improvement and monitor post-fix to validate impact on external workspaces.

**Why not retry at `execute_content_search`:** The retry logic needs access to the workspace snapshot and closure machinery that's already set up inside `line_mode_matches`. Doing it there avoids crossing the execution/line_mode boundary twice.

**Test plan:**
- RED: test that sends a query with a too-narrow `file_pattern` and asserts zero results (existing `live_zero_hit_attributes_file_pattern_when_pattern_drops_every_candidate` test)
- GREEN: modify to assert non-zero results with `scope_relaxed == true` and `original_file_pattern` set
- New test: rescue is NOT triggered for `CandidateStarvation` (those have in-scope candidates, just outside the fetch window)
- New test: rescue is NOT triggered when `file_pattern` matches but results are zero for other reasons (e.g., `LineMatchMiss`)
- New test: rescue results are structurally labeled (trace fields present, output header present)

### Steps

1. Add `scope_relaxed: bool`, `original_file_pattern: Option<String>`, `original_zero_hit_reason: Option<ZeroHitReason>` to `LineModeSearchResult`
2. Extract the workspace-specific search block from `line_mode_matches` into a helper (one for Primary, one for Target, or a unified closure-builder)
3. Add rescue logic: on `FilePatternFiltered` + `NoInScopeCandidates`, re-invoke helper with `file_pattern: None`
4. Propagate structured fields through `SearchExecutionResult` / `SearchTrace`
5. Add/update hint in `hint_formatter.rs` to render the labeled rescue header
6. Update telemetry in `search_telemetry.rs` to record `scope_relaxed: true` and `scope_rescue_count` so future metrics can track this separately from in-scope hits
7. Write tests (see test plan above)

---

## F2: Agent file_pattern guidance (instruction layer)

### Problem

Agents sometimes use `file_pattern="src/handler.rs"` for content searches on a single file. When the search returns zero results, the agent has burned a tool call. Using `get_symbols(file_path=...)` would be more effective for exploring a specific file's structure, while `file_pattern` with a single file path is valid for grep-within-one-file workflows.

The goal is guidance, not prohibition. Single-file `file_pattern` is not inherently wrong; it's suboptimal when the agent wants symbol structure rather than line-level text matches.

### Fix: two-layer nudge

**Layer 1: hint text (in F1 rescue results)**
When rescue fires and the original `file_pattern` has no glob characters (`*`, `?`, `[`), append: `"Hint: for symbol structure within a specific file, use get_symbols(file_path=...). file_pattern is valid for text search within a known file."`

**Layer 2: agent instructions**
Update `JULIE_AGENT_INSTRUCTIONS.md` to add guidance under the `fast_search` tool description:
```
file_pattern scopes searches to matching paths (e.g., "src/**/*.rs", "tests/**", or a specific file).
For symbol structure within a specific file, prefer get_symbols(file_path=...) over file_pattern.
```

**No PreToolUse hook change.** Hooks that reject valid tool calls create friction. The rescue approach (F1) handles the failure case, and better instructions handle the guidance case.

### Steps

1. Add the glob-detection check to the rescue hint in `hint_formatter.rs`
2. Update `JULIE_AGENT_INSTRUCTIONS.md` with the file_pattern guidance
3. Copy updated instructions to `~/source/julie-plugin` for distribution

---

## F4: line_match_miss for punctuated/operator queries (26% of remaining zero-hits)

### Problem

Queries like `INSERT OR REPLACE symbols`, `logging.basicConfig`, `\.julie/logs`, `format("%Y-%m-%d %H:%M")` contain punctuation, operators, and escape sequences that code-aware tokenizers strip or split. Tantivy finds candidate files (the terms partially match), but the line-level matcher can't find a line containing the full query because the tokenized search doesn't preserve punctuation boundaries.

52 of 202 zero-hits are `LineMatchMiss`. These are "correct" from the search engine's perspective (the tokenized terms don't match any single line), but frustrating for agents who are searching for exact text.

### Investigation results

Deep-dived the full pipeline: `line_match_strategy` (query classification) -> `line_matches` (dispatch) -> `line_matches_literal` / `tokenize_text_for_line_match` + `term_matches_tokens` (matching). The 52 line_match_miss queries break into three root causes:

**Root cause A: Boolean operators treated as literal text (12 queries, 23%)**

`line_match_strategy` checks for ` OR ` / ` AND ` / ` NOT ` and routes to `Substring(entire_query_lowered)`. So `logging.basicConfig OR logging.Formatter OR datefmt` becomes a literal substring search for `"logging.basicconfig or logging.formatter or datefmt"`. No line will ever contain that.

Meanwhile, Tantivy's `search_content` builds a `BooleanQuery` from tokenized terms. The line matcher treats the operators as literal text while the index treats them as structure. These two layers disagree on what the query means.

**This is a design gap, but not a blanket bug.** Some queries with `OR`/`AND`/`NOT` are legitimate literals:
- SQL: `INSERT OR REPLACE`, `IS NOT NULL`, `SELECT ... AND ...`
- Comments/docs: `DO NOT EDIT`, `SHOULD NOT be modified`
- Config expressions with boolean-looking keywords

Blanket parsing of `OR`/`AND`/`NOT` as boolean operators would break these cases. The fix must be narrow and high-confidence.

**Root cause B: Separator normalization mismatch (9 queries, 17%)**

Queries like `security-signals` (hyphenated) don't match code containing `security_signals` (underscored). Tantivy normalizes both to `security` + `signals` and finds candidate files, but the Substring line matcher looks for the literal `security-signals` which doesn't appear.

Similarly `\.julie/logs` (escaped dot) doesn't match `.julie/logs` because the `\` is treated as literal.

**Fix:** In `line_matches_literal`, when the literal substring match fails and the query contains hyphens or escape characters, try a normalized variant (hyphen<->underscore, strip leading backslashes).

**Root cause C: Correct misses (20 queries, 38%) + quote/paren queries (11, 21%)**

Simple terms like `file_mode`, `fast_search`, `async_trait` that Tantivy finds in candidate files but no line contains the literal text. These are often from workspaces where the index is stale relative to file content, or the term exists only in multi-line constructs (spread across lines). The quote/paren queries (`search_target="files"`, `text_search_impl(`) are similar: the line matcher is correct; the text genuinely doesn't appear on a single line.

These are not fixable at the search level (they're correct behavior) and don't warrant changes.

### Fix plan

**Phase 1: Narrow high-confidence OR detection in line_match_strategy**

The ` OR ` / ` AND ` / ` NOT ` early return in `line_match_strategy` routes all such queries to `Substring`, which is wrong for boolean-intent queries but correct for literal-intent queries. The fix must distinguish the two cases.

**Detection heuristic:** Treat `OR` as boolean only when the query is a clean disjunction of identifier-shaped terms:
- Every branch (split on ` OR `) is a single identifier or dotted path (no spaces within branches)
- Examples that pass: `logging.basicConfig OR datefmt OR asctime`, `Command::Search OR Command::Refs`
- Examples that fail (stay Substring): `INSERT OR REPLACE symbols` (multi-word branch), `IS NOT NULL` (not OR-shaped), `DO NOT EDIT` (natural language)

Leave `AND` and `NOT` alone for now. Current behavior (Substring) is wrong for some AND/NOT queries too, but the false-positive risk is higher (e.g., `IS NOT NULL`, `DO NOT EDIT`). The existing `-term` exclusion syntax already handles the NOT case for agents who know about it.

```rust
// Pseudocode for the heuristic:
fn is_clean_or_disjunction(query: &str) -> Option<Vec<String>> {
    if !query.contains(" OR ") { return None; }
    let branches: Vec<&str> = query.split(" OR ").collect();
    // Each branch must be a single token (no internal whitespace after trim)
    if branches.iter().all(|b| b.trim().split_whitespace().count() == 1) {
        Some(branches.iter().map(|b| b.trim().to_lowercase()).collect())
    } else {
        None // Multi-word branches -> stay on Substring
    }
}
```

When the heuristic fires, route to `FileLevel { terms }` (match lines containing ANY term). When it doesn't, fall through to the existing Substring path.

**Where:** `src/tools/search/query.rs` (line_match_strategy function)

**Test plan:**
- `test_clean_or_disjunction_produces_file_level`: `"logging.basicConfig OR datefmt"` -> `FileLevel { terms: ["logging.basicconfig", "datefmt"] }`
- `test_multi_word_or_stays_substring`: `"INSERT OR REPLACE symbols"` -> `Substring` (multi-word branch "REPLACE symbols")
- `test_sql_not_null_stays_substring`: `"IS NOT NULL"` -> `Substring`
- `test_do_not_edit_stays_substring`: `"DO NOT EDIT"` -> `Substring`
- `test_qualified_or_produces_file_level`: `"Command::Search OR Command::Refs OR Command::Tool"` -> `FileLevel`
- `test_quoted_phrase_still_substring`: `'"INSERT OR REPLACE"'` -> `Substring` (quote wrapping signals literal intent)
- `test_single_branch_or_stays_substring`: `"INSERT OR REPLACE"` -> `Substring` (only 2 branches, each single-word, but this is known SQL syntax; could go either way; test documents the decision)

**Phase 2 (enhancement): Separator-normalized fallback in line_matches_literal**

When the literal substring match fails and the query contains hyphens or backslashes, try:
1. Replace `-` with `_` (and vice versa) and retry
2. Strip leading `\` before non-metacharacters and retry

**Where:** `src/tools/search/query.rs` (line_matches_literal function)

**Test plan:**
- `test_hyphen_underscore_normalization`: `"security-signals"` matches line containing `security_signals`
- `test_backslash_stripping`: `"\.julie"` matches line containing `.julie`

### Expected impact

The narrow OR heuristic covers queries like `logging.basicConfig OR datefmt OR asctime` and `Command::Search OR Command::Refs` but not `INSERT OR REPLACE` (multi-word branch). Of the 12 boolean-operator queries, approximately 7-8 are clean disjunctions that the heuristic would catch. Phase 2 (separator normalization) covers ~5-9 more. Together: ~12-17 fewer line_match_miss zeros, reducing the true zero-hit rate from 8.0% to approximately 6.5%.

### Steps

1. Write failing tests for clean OR disjunction queries (RED)
2. Add `is_clean_or_disjunction` detection to `line_match_strategy`, route to `FileLevel` (GREEN)
3. Write failing tests for separator normalization (RED)
4. Add normalized-fallback in `line_matches_literal` (GREEN)
5. Run `cargo xtask test changed` to check for regressions
6. Update `search_telemetry.rs` to track `or_disjunction_detected: true` in trace metadata

---

## Priority and ordering

| Finding | Impact | Effort | Priority |
|---------|--------|--------|----------|
| F1: file_pattern scope rescue | 118/202 raw zeros (58%) | Medium (refactor + retry logic) | **P0** |
| F2: Agent instructions | Prevents future F1 occurrences | Low (text changes) | **P1** (do alongside F1) |
| F3: Latency | ~~False alarm~~ | None | **Closed** |
| F4 Phase 1: Narrow OR disjunction detection | ~7-8/52 line_match_miss | Low (heuristic + routing) | **P2** |
| F4 Phase 2: Separator normalization | ~5-9/52 line_match_miss | Low (fallback in literal matcher) | **P3** |

### Impact projections (corrected)

F1 and F4 operate on different metrics. F1 reduces **raw** zero-hit rate (it rescues `file_pattern_filtered` zeros, which are excluded from the "true" rate by definition). F4 reduces **true** zero-hit rate (it fixes `line_match_miss` zeros, which are included).

- **Raw zero-hit rate:** 19.2% -> ~8-9% (F1 rescues most of the 118 `file_pattern_filtered` zeros)
- **True zero-hit rate:** 8.0% -> ~6.5% (F4 Phase 1+2 eliminate ~12-17 `line_match_miss` zeros out of 1,052 total calls)
- **Precision caveat:** lower zero-hits only count as progress if rescued/broadened results are useful. F1 adds labeled out-of-scope results, which may or may not help the agent. Monitor whether agents act on rescued results or ignore them.

**Implementation order:** F1 is the highest-impact single change (raw rate). F4 Phase 1 is the highest-impact change on true rate. F2 is text-only and ships alongside F1. F4 Phase 2 is a small enhancement once Phase 1 is in.

Recommended batching: F1+F2 as one PR, F4 Phase 1 as a second PR, F4 Phase 2 as a third.
