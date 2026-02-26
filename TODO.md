# TODO

## Open Items

- [ ] **Filewatcher validation**: Validate that the filewatcher is keeping the index fresh since we made all the changes with adding tantivy and removing the embeddings pipeline

- [ ] **Evaluate token optimization across tools**: The `get_context` pipeline now has `truncate_to_token_budget` (head-biased truncation, 2/3 top + 1/3 bottom) and adaptive token allocation (`pivot_tokens` / `neighbor_tokens` / `summary_tokens` split). Evaluate whether any of this should be ported to other tools:
  - `deep_dive` — currently returns full code bodies at `context`/`full` depth; could benefit from per-symbol budget enforcement when multiple symbols are returned
  - `get_symbols` — already has its own truncation logic in `mode="minimal"`/`mode="full"`; check if the head-biased approach would be better than its current strategy
  - `fast_search` — content mode returns matching lines; probably fine as-is (already line-limited)
  - General question: should `TokenEstimator`-based budgets be a shared utility pattern rather than per-tool implementations?

- [ ] **Search-layer relevance for natural-language queries**: `get_context` scoring/fallback works well when the search index returns a mix of code and non-code, but can't help when the index returns *only* docs/tests. Natural-language queries like "workspace routing" or "symbol extraction" match markdown headings and test function names better than production code identifiers. Potential approaches:
  - Query expansion / synonym mapping (e.g., "workspace routing" → also search `workspace`, `route`, `router`)
  - Index-time boost for `src/` paths vs docs/tests
  - Semantic search layer (embeddings) for concept-level matching
  - Observed during live testing of v3.3.2 get_context improvements
