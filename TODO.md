# TODO

## Open Items

- [ ] **Evaluate token optimization across tools**: The `get_context` pipeline now has `truncate_to_token_budget` (head-biased truncation, 2/3 top + 1/3 bottom) and adaptive token allocation (`pivot_tokens` / `neighbor_tokens` / `summary_tokens` split). Evaluate whether any of this should be ported to other tools:
  - `deep_dive` — currently returns full code bodies at `context`/`full` depth; could benefit from per-symbol budget enforcement when multiple symbols are returned
  - `get_symbols` — already has its own truncation logic in `mode="minimal"`/`mode="full"`; check if the head-biased approach would be better than its current strategy
  - `fast_search` — content mode returns matching lines; probably fine as-is (already line-limited)
  - General question: should `TokenEstimator`-based budgets be a shared utility pattern rather than per-tool implementations?

- [ ] **NL query recall for C# classes with indirect naming** (Medium)
  - `get_context("LuceneIndexService")` returns correct pivots (ref=9.1), but `get_context("Lucene search implementation")` misses it
  - BM25 term overlap: query ["lucen","search","implement"] vs symbol ["lucen","index","servic"] = only 1/3 match
  - File content has "search" many times but spread across method-level symbols in 1200-line file, diluting per-symbol BM25
  - Potential fixes: NL synonym expansion ("search" ↔ "index"), semantic search fallback, or name-boosted BM25 scoring
  - `TextSearchTool` also invisible (ref=0) — only referenced by test classes; production code uses DI resolution

- [ ] **`fast_refs` classifies Python imports as "Definitions"** (Low/UX)
  - `EmbeddingManager` in miller shows "Definitions (123)" but 122 are imports, only 1 is the real class
  - Technically correct (imports create local bindings) but misleading for users expecting 1 definition
  - Consider a separate "Imports" category or only showing the actual definition under "Definitions"

## Recently Completed

- [x] **C# centrality propagation: interface → implementation** (2026-02-28)
  - Added Step 2 to `compute_reference_scores()`: propagates 70% of interface/base class centrality to implementations
  - `LuceneIndexService`: 0 → 9.1 (from `ILuceneIndexService` ref=13), `PathResolutionService`: 0 → 21.7 (from `IPathResolutionService` ref=31)
  - TDD: 2 new tests for propagation behavior, all existing tests pass

- [x] **C# cross-file inheritance extraction via PendingRelationship** (2026-02-28)
  - `extract_inheritance_relationships` only resolved against same-file symbols, silently dropping cross-file interfaces (nearly ALL C# inheritance)
  - Added `else` branch creating `PendingRelationship` with `is_interface_name()` heuristic for Implements vs Extends
  - Phase 1/Phase 2 restructure to satisfy borrow checker (collect data → create relationships)
  - coa relationships: 2,067 → 2,088 (+21 new cross-file inheritance)
  - TDD: 3 new tests (cross-file interface, cross-file base class, same-file still works)

- [x] **`get_context` NL path prior made language-agnostic** (2026-02-28)
  - `is_test_path` now handles C# `.Tests` dirs, Go `_test.go`, JS/TS `.test.ts`/`.spec.ts`, Python `test_*.py`, Ruby `spec/`, generic `test`/`tests`/`__tests__` segments
  - `is_docs_path` and `is_fixture_path` similarly generic
  - Tests cover C#, Python, Java, Go, JS/TS, Ruby project layouts
  - Fixed: coa NL queries no longer return test files/stubs; Program.cs no longer gravitates as pivot

- [x] **`get_context` Program.cs gravity eliminated** (2026-02-28)
  - Verified: "how does text search work", "Lucene search implementation", "circuit breaker error handling", "symbol extraction pipeline" — none return Program.cs as pivot
  - Root cause was NL path prior being a no-op on C# layouts (now fixed)

- [x] **`get_context` off-topic `estimate_words` for "search scoring"** (2026-02-28)
  - "how does search scoring work" now returns `calculate_score` (path_relevance) and `calculate_search_confidence` (search scoring) — both relevant

- [x] **`get_context` empty Lua test pivot for "symbol extraction pipeline"** (2026-02-28)
  - Now returns `spawn_workspace_embedding`, `extract_symbols`, `process_files_optimized` — all relevant, no empty tests

- [x] **`get_context` compact format dumping massive code bodies** (2026-02-28)
  - Was caused by Program.cs gravity (fixed) — low-value pivots no longer consume entire token budget

- [x] **Definition search over-fetch + kind-aware promotion** (2026-02-28)
  - Over-fetch floor bumped from 50 to 200; three-tier promotion (definition kinds → non-definition → rest)
  - Removed premature `.take(limit)` truncation before promotion in file_pattern code path
  - `LuceneIndexService` definition search with limit=5 now promotes correctly
  - `EmbeddingManager` on miller now shows class definition as first result

- [x] **Deduplicated `is_nl_like_query`** (2026-02-28)
  - Deleted weaker private copy in `expansion.rs`, replaced with import of canonical version from `scoring.rs`

- [x] **C# field/property type relationships** (2026-02-28)
  - Added `field_declaration` and `property_declaration` relationship extraction
  - Shared helpers: `extract_type_name_from_node`, `find_containing_class`
  - 7 new tests, all passing

- [x] **File delete events**: Handle when a delete event occurs in the filewatcher but the file isn't deleted, just edited (atomic save pattern). Fixed with `path.exists()` guard + `should_process_deletion()` for real deletions. (2026-02-28)
- [x] **Filewatcher validation**: Validated watcher keeps index fresh with Tantivy + embeddings sidecar. Incremental pipeline confirmed (52 new / 19,899 skipped on restart). (2026-02-28)
- [x] **CPU usage at idle**: Fixed — 0.0% CPU at idle with new sidecar setup. (2026-02-28)
- [x] **Search-layer relevance for natural-language queries**: Shipped deterministic NL query expansion (original/alias/normalized groups), weighted query builders, and conservative NL-only `src/` path prior with regression coverage for identifier-query stability.
