# Phase 1: Search Quality & Cleanup

**Date:** 2026-02-24
**Status:** Design Approved
**Depends on:** Nothing
**Enables:** Phase 2 (Graph Centrality), Phase 3 (get_context tool)

## Context

Competitive analysis against [vexp.dev](https://vexp.dev) revealed that Julie's search quality has a significant gap: when strict AND-per-term queries return zero results, there is no fallback. Previously, semantic embeddings served as a safety net — if text search missed, embeddings could catch conceptual matches. Since embeddings were removed (Feb 2026), zero-result queries are a dead end.

Additionally, 26 stale `todo!()` test stubs exist in `src/tests/integration/intelligence_tools.rs` for tools that were either already removed or never built. These violate TDD methodology and must be cleaned up.

## Problem Statement

### Zero-Result Searches

Julie's `build_symbol_query` in `src/search/query.rs` uses `Occur::Must` for every tokenized term. This is strict AND — a document must contain ALL terms to match.

**Example failures from a real session:**
- `"ranking score boost centrality"` → 4 terms, all required → zero results
- `"zero results fallback OR relaxed query"` → 6 terms (OR is literal), all required → zero results
- `"logic_flow trace execution path"` → zero results
- `"cross_repo multi_repo openapi"` → zero results

These queries had relevant content in the codebase but no single symbol/file contained every token. Without a fallback mechanism, the agent wastes round-trips retrying with different queries.

### Stale Test Stubs

`src/tests/integration/intelligence_tools.rs` contains 26 tests across 6 modules, all with `todo!()` bodies:

| Module | Tests | Originally For |
|--------|-------|---------------|
| `explore_overview_tests` | 6 | ExploreOverviewTool (removed as `fast_explore`) |
| `trace_execution_tests` | 5 | TraceExecutionTool (removed as `trace_call_path`) |
| `get_minimal_context_tests` | 4 | GetMinimalContextTool (never built) |
| `find_business_logic_tests` | 4 | FindBusinessLogicTool (never built) |
| `score_criticality_tests` | 5 | ScoreCriticalityTool (never built) |
| `integration_tests` | 2 | Pipeline integration (never built) |

These are NOT TDD — they're forward-declared stubs that panic immediately.

## Design

### 1. Delete Stale Tests

Delete `src/tests/integration/intelligence_tools.rs` entirely. Remove its `mod` declaration from `src/tests/integration/mod.rs` (or equivalent). Clean compile, clean test run.

### 2. OR-Fallback for Zero-Result Searches

**Location:** `src/search/index.rs` → `search_symbols()` and `src/search/query.rs`

**Strategy:** When AND-per-term returns zero results, automatically retry with OR semantics.

```
search_symbols(query, filter, limit):
  1. tokens = filter_compound_tokens(tokenize_query(query))
  2. query = build_symbol_query(tokens, ..., mode=AND)
  3. results = searcher.search(query, limit)
  4. IF results.is_empty() AND tokens.len() > 1:
     a. query = build_symbol_query(tokens, ..., mode=OR)
     b. results = searcher.search(query, limit)
     c. mark results as relaxed_match = true
  5. apply_important_patterns_boost(results)
  6. return results
```

**Implementation in `build_symbol_query`:**
- Add a `require_all_terms: bool` parameter (or an enum `MatchMode { And, Or }`)
- When `require_all_terms = true` (default): current behavior, `Occur::Must` per term
- When `require_all_terms = false`: use `Occur::Should` per term
- BM25 naturally ranks documents matching more terms higher
- Documents matching 3/4 terms rank above documents matching 1/4

**Output hint:** When OR fallback is used, prepend a note to the tool output:
```
⚡ Relaxed search (partial matches) — no results matched all terms
```
This tells the agent the results are broader than requested.

### 3. Search Quality Audit

After implementing OR fallback, verify with real-world queries:

**Must still work (AND precision):**
- `"UserService"` → exact match, definition promoted
- `"process_payment"` → exact match
- `"build_symbol_query"` → exact match

**Must now work (OR fallback):**
- `"ranking score boost centrality"` → finds symbols related to scoring/ranking
- `"graph centrality"` → finds related code
- `"token budget limit"` → finds TokenEstimator and related code

**Must not regress:**
- Single-token queries unchanged
- Definition search (`search_target="definitions"`) unchanged
- Language/file filtering unchanged

## Implementation Steps

### Step 1: Delete stale tests
1. Delete `src/tests/integration/intelligence_tools.rs`
2. Remove mod declaration from parent module
3. Run `cargo test 2>&1 | tail -20` — verify clean

### Step 2: Add MatchMode to query building
1. Write failing test: search with 4 unrelated terms returns zero AND results but non-zero OR results
2. Add `require_all_terms` parameter to `build_symbol_query`
3. Implement OR variant (change `Occur::Must` to `Occur::Should` for term clauses)
4. Pass test

### Step 3: Integrate OR fallback into search_symbols
1. Write failing test: `search_symbols` with a multi-term query that has no AND matches returns OR results
2. Modify `search_symbols` to retry with OR on zero results
3. Add relaxed-match indicator to output
4. Pass test

### Step 4: Integration testing with content search
1. Verify `build_content_query` in `src/search/query.rs` — does it have the same AND-strictness problem?
2. If yes, apply similar OR fallback
3. Write tests for content search fallback

### Step 5: Quality audit
1. Build debug binary
2. Test against Julie's own codebase with the query suite above
3. Verify no regressions in existing search behavior
4. Tune if needed

## Success Criteria

- [ ] Zero stale `todo!()` tests in codebase
- [ ] Multi-term queries that previously returned zero results now return relevant partial matches
- [ ] Single-term and exact-match queries unchanged in behavior
- [ ] All existing search tests pass
- [ ] Agent can distinguish AND matches from OR fallback matches in output

## Risk Assessment

**Low risk.** OR fallback only triggers on the zero-result path — existing behavior is completely unchanged when AND produces results. The stale test deletion is pure cleanup.

**One concern:** OR fallback could return noisy results for very broad queries (many common tokens). Mitigation: only fall back when `tokens.len() > 1` (single-token queries already work fine), and consider a minimum match threshold (e.g., must match at least 2 of N terms).
