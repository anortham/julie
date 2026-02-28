# TODO

## Open Items

- [ ] **Filewatcher validation**: Validate that the filewatcher is keeping the index fresh since we made all the changes with adding tantivy and removing the embeddings pipeline

- [ ] **CPU USAGE IS TOO HIGH**: when it should be at idle with no usage, it's too high, what is going on?

- [ ] **Evaluate token optimization across tools**: The `get_context` pipeline now has `truncate_to_token_budget` (head-biased truncation, 2/3 top + 1/3 bottom) and adaptive token allocation (`pivot_tokens` / `neighbor_tokens` / `summary_tokens` split). Evaluate whether any of this should be ported to other tools:
  - `deep_dive` — currently returns full code bodies at `context`/`full` depth; could benefit from per-symbol budget enforcement when multiple symbols are returned
  - `get_symbols` — already has its own truncation logic in `mode="minimal"`/`mode="full"`; check if the head-biased approach would be better than its current strategy
  - `fast_search` — content mode returns matching lines; probably fine as-is (already line-limited)
  - General question: should `TokenEstimator`-based budgets be a shared utility pattern rather than per-tool implementations?

- [x] **Search-layer relevance for natural-language queries**: shipped deterministic NL query expansion (original/alias/normalized groups), weighted query builders, and conservative NL-only `src/` path prior with regression coverage for identifier-query stability.
  - Remaining gap: phrase alias coverage is intentionally small/curated and may need expansion as we collect more real dogfooding queries.

## Dogfood Issues (2026-02-26)

Cross-workspace dogfood testing across primary (julie/Rust), coa-codesearch-mcp (.NET), and miller (Python+Rust) revealed the following issues:

### `get_context` — Reference Workspace Relevance

- [ ] **`get_context` on coa returns test files and stubs for NL queries** (Major)
  - "how does text search work" → 3 test methods all named `ExecuteAsync_Should_Return_Error_When_File_Does_Not_Exist`
  - "Lucene search implementation" → 2 stub methods that literally return empty lists
  - "circuit breaker error handling" → entire `LuceneIndexService` class (1232 lines) + entire `Program` class
  - "symbol extraction pipeline" (compact) → `Main` method + `Program` class from Program.cs
  - Meanwhile "database query symbols" and "how does indexing work" returned good results
  - **Root cause confirmed (double whammy):**
    1. NL path prior is a **no-op** on coa — hardcoded paths (`src/tests/`, `src/`, `docs/`, `fixtures/`) don't match C# layout (`COA.CodeSearch.McpServer.Tests/`, `COA.CodeSearch.McpServer/`). See `apply_nl_path_prior()` in `src/search/scoring.rs:127-147`.
    2. Centrality is **too weak to compensate** — coa max ref_score is 168 (vs julie 3,795, miller 396) and top symbols are generic methods (`GetFileName`, `ToString`, `Equals`), not important implementation classes.
    3. Miller survives despite same path-prior gap because it has strong centrality (EmbeddingManager: 318, StorageManager: 378).
  - **Fix: make NL path prior language-agnostic** — detect test dirs generically (path contains `/test/`, `/tests/`, `.Tests/`, `.Test/`, `test_`, `_test`) instead of hardcoded `src/tests/`.

- [ ] **`get_context` on coa gravitates toward `Program.cs`** (Medium)
  - Multiple unrelated queries return `Program` class (656-line DI entry point) as a pivot
  - `Program.cs` references everything → matches all queries → gets selected despite low relevance
  - **Root cause:** Program.cs is a DI container that references every service, so it tokenizes to include terms for everything. Without centrality to counterbalance (ref_score: 0), it matches every NL query.
  - **Fix options:** (a) penalize entry-point/startup files (Program.cs, main.py, index.ts), (b) investigate why important C# classes like `LuceneIndexService` get ref_score 0 — likely the C# relationship extractor doesn't capture DI constructor injection patterns, (c) add a "references too many things" anti-signal (files that call everything are rarely what you want to understand a concept)

- [ ] **`get_context` on primary returned off-topic `estimate_words` for "how does search scoring work"** (Minor)
  - `ScoringConfig` was relevant but `estimate_words` (token estimation) is unrelated to search scoring
  - Should have surfaced `apply_centrality_boost`, `apply_nl_path_prior`, etc.

- [ ] **`get_context` compact format on coa dumps massive code bodies** (Medium)
  - Token budget system should prevent massive bodies but low-value pivots (`Program`, `Main`) consume the entire budget
  - Related to the `Program.cs` gravity issue above

- [ ] **`get_context` on primary "symbol extraction pipeline" returned empty Lua test as first pivot** (Minor)
  - `test_table_extraction_basic` is a placeholder test with only a comment — very low value pivot
  - Second pivot (`process_files_optimized`) was excellent
  - Test files should be penalized more heavily in pivot selection

### `fast_search` — Definition Promotion on Reference Workspaces

- [ ] **Definition promotion fails when limit is too small — definition ranks below cutoff** (Medium)
  - `fast_search(query="LuceneIndexService", search_target="definitions", limit=5)` → "5 matches for" (no promotion)
  - `fast_search(query="LuceneIndexService", search_target="definitions", limit=10)` → "Definition found:" (correctly promoted!)
  - **Root cause:** The actual definition file (`LuceneIndexService.cs`) ranks below position 5 in Tantivy because files that *reference* the class have more keyword hits than the definition file itself. Definition promotion happens in the formatting step and can only promote what's already in the results. With small limits, the definition falls outside the result set entirely.
  - **Fix:** When `search_target=definitions`, **over-fetch** candidates (e.g., 3x-5x the limit), find and promote definitions first, then apply the display limit. See `format_definition_search_results()` in `src/tools/search/formatting.rs:98`.

- [ ] **Definition search on miller promotes imports over actual class definition** (Medium)
  - `fast_search(query="EmbeddingManager", search_target="definitions")` returned 4 imports and 1 variable assignment
  - The actual class definition at `python/miller/embeddings/manager.py:79` was NOT in the top 5
  - **Root cause:** Same over-fetch issue — imports that mention `EmbeddingManager` rank higher than the class definition. Also, the formatting partitions by `s.name == query` (exact match) — if the definition IS in results, promotion works, but it needs to be fetched first.
  - **Fix:** Same over-fetch fix as above. Additionally, when doing definition search, could pre-filter by `SymbolKind::Class`/`Struct`/`Interface` to boost actual definitions over imports.

### `fast_refs` — Classification

- [ ] **`fast_refs` classifies Python imports as "Definitions"** (Low/UX)
  - `EmbeddingManager` in miller shows "Definitions (123)" but 122 are imports, only 1 is the real class
  - Technically correct (imports create local bindings) but misleading for users expecting 1 definition
  - Consider a separate "Imports" category or only showing the actual definition under "Definitions"

### Root Cause Summary

Three systemic root causes explain most of the issues above:

1. **NL path prior is Julie-specific** (`src/search/scoring.rs:127-147`)
   - Hardcoded `src/tests/`, `src/`, `docs/`, `fixtures/` patterns don't match C# or Python project layouts
   - Makes the prior a no-op for ALL non-Julie workspaces
   - Affects: coa test-file surfacing, coa `Program.cs` gravity, primary Lua test pivot
   - Miller survives because strong centrality compensates

2. **C# relationship extraction doesn't capture DI patterns**
   - Important C# classes like `LuceneIndexService`, `TextSearchTool`, `FileIndexingService` all have ref_score: 0
   - Top coa symbols are generic methods (`GetFileName: 168`, `ToString: 117`, `Equals: 81`)
   - Only 1,786 relationships extracted vs 11,236 for julie — likely missing constructor injection, interface implementation, and attribute relationships
   - Centrality data: coa max 168, miller max 396, julie max 3,795

3. **Definition search doesn't over-fetch** (`src/tools/search/text_search.rs`)
   - When `search_target=definitions`, the search uses the user's limit directly
   - Definitions rank low in Tantivy (defined once vs referenced many times)
   - Small limits (5) exclude the actual definition entirely
   - Fix: over-fetch 3-5x, find definitions, promote, then trim to limit

### What Worked Well

- `deep_dive` — Excellent across all 3 workspaces (Rust structs, C# classes, Python classes)
- `fast_refs` — Correct and comprehensive reference tracking (8 refs for SearchIndex, 230 for EmbeddingManager)
- `get_symbols` — Perfect file structure extraction (61 symbols from 1232-line C# file)
- `fast_search` content mode — Multi-word queries returned relevant results across all workspaces
- `get_context` on miller — Consistently excellent (EmbeddingManager, StreamingPipeline, _extract_worker)
- `get_context` on julie — Generally good (compact format for "symbol extraction pipeline" was great)
