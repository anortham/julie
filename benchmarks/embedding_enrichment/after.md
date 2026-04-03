# Embedding Enrichment Benchmark: After

**Date:** 2026-04-03
**Commit:** b7cd8375
**Model:** CodeRankEmbed (768d, sidecar)
**Changes:** File path in metadata, implementor enrichment, field signature enrichment, query classification
**Workspaces:** Julie (4947 vectors), Zod (6361), Flask (617), Cobra (404)

**IMPORTANT NOTE:** Conceptual queries require `search_target="definitions"` to route through hybrid search where query classification + semantic weighting apply. The default `content` mode is line-level Tantivy only and doesn't use embeddings. This is a workflow instruction concern (Task 10).

---

## 1. Exact Symbol Lookups (Control Group)

No change expected or observed. All exact lookups return the same #1 results as baseline. No regressions.

---

## 2. Conceptual / Natural Language Queries (search_target="definitions")

### Julie (Rust)

**`fast_search("error handling and retry logic", definitions, limit=10)`**
- Results: query_classification_tests.rs, error_handling.rs fixture, baseline.md, plan.md, **user-dashboard.tsx fixture** (retry), **sidecar protocol.py** (error handling)
- Assessment: **Mixed.** Still keyword-heavy, but now includes code files (sidecar protocol, fixture with retry). Improvement over pure doc/comment hits.

**`fast_search("search scoring and ranking", definitions, limit=10)`**
- Results: **get_context/pipeline.rs**, **text_search.rs**, **migrations.rs** (reference_score), **line_mode.rs**, **search/quality.rs**, **exact_match_boost.rs**, **tantivy_index_tests.rs**, CLAUDE.md
- Assessment: **DRAMATICALLY BETTER.** Baseline returned TESTING_GUIDE.md and expansion.rs. Now returns the actual search scoring implementation files.

**`fast_search("symbol extraction from source code", definitions, limit=10)`**
- Results: **factory.rs** (extractor factory), **body_extraction.rs**, **swift/mod.rs**, **java/mod.rs**, **go/mod.rs**, **base/types.rs** (Symbol struct), **Identifier struct**, **cpp/identifiers.rs**, **handler.rs**, **csharp/di_relationships.rs**
- Assessment: **DRAMATICALLY BETTER.** Baseline returned .julieignore comments. Now returns the actual extractor code across 6+ files.

### Zod (TypeScript)

**`fast_search("input validation and type checking", definitions, limit=10)`**
- Results: **parseUtil.ts**, **core/schemas.ts** (2x), **standard-schema.ts** (Result type), **ZodError.ts**, **types.ts** (multiple: objectInputType, objectOutputType, parse, ~validate)
- Assessment: **DRAMATICALLY BETTER.** Baseline returned Chinese README. Now returns actual validation logic, parse utilities, and error types.

### Flask (Python)

**`fast_search("request routing and middleware", definitions, limit=10)`**
- Results: **app.py:109** (Flask class), **app.py:992** (full_dispatch_request), **app.py:1501** (request_context), **app.py:966** (dispatch_request), **app.py:562**, **app.py:1366** (preprocess_request), **ctx.py** (2x), **globals.py**, **tutorial example**
- Assessment: **DRAMATICALLY BETTER.** Baseline returned docs/design.rst. Now returns the actual Flask request handling pipeline.

### Cobra (Go)

**`fast_search("command line argument parsing", definitions, limit=10)`**
- Results: **command.go:54** (Command struct), **command.go:821** (Traverse), **command.go:750** (args), **command.go:1868** (ParseFlags), **command.go:788** (suggestions), **completions.go**, completions/_index.md, **args.go** (2x), **doc/yaml_docs.go**
- Assessment: **DRAMATICALLY BETTER.** Baseline returned completions docs only. Now returns Command struct, Traverse, ParseFlags, args.go.

---

## 3. Similar Symbols (deep_dive)

### Julie (Rust)

**`deep_dive("hybrid_search", depth="context")`**
- No "Similar symbols" section
- Assessment: **No change.** Threshold (0.5) still too aggressive for CodeRankEmbed.

**`deep_dive("format_symbol_metadata", depth="context")`**
- No "Similar symbols" section
- Assessment: **No change.**

**`deep_dive("SymbolDatabase", depth="full")`**
- No "Similar symbols" section
- Assessment: **No change.** Struct embedding ("struct SymbolDatabase\nin: src/database/mod.rs") still too sparse.

### Verdict on Similar Symbols
Similar symbols remain absent. The 0.5 threshold is likely too high for CodeRankEmbed's cosine distance distribution. Task 9 should investigate lowering to 0.4 or 0.35.

---

## 4. get_context Orientation

### Julie (Rust)

**`get_context("embedding pipeline and vector search")`**
- Pivots: `spawn_workspace_embedding`, `embed_query` (same as baseline)
- Neighbors: 12 symbols (same set)
- Assessment: **No change.** The embedding pipeline query lands on the same infrastructure pivots.

**`get_context("how does search scoring work")`**
- Pivots: `calculate_score` (path relevance), **`search_symbols`** (Tantivy search!), **`apply_centrality_boost`** (reference-score reranking!)
- Neighbors: 17 symbols including hybrid_search, build_symbol_query_weighted, expand_query_terms, apply_nl_path_prior, definition_search_with_index
- Assessment: **SIGNIFICANTLY BETTER.** Baseline had ScoringConfig and get_reference_scores (DB query). Now shows the actual search engine + centrality boost + query building. An agent gets the complete scoring pipeline in one call.

### Zod (TypeScript)

**`get_context("type validation", workspace="zod")`**
- Pivots: `Result` (standard-schema), **`input`** (type alias), `ZodError` (same as baseline)
- Neighbors: 25 symbols (expanded from 18), including encode/decode/safeParse/refine/prefault
- Assessment: **Improved.** Added `input` type as pivot, expanded neighbor set by 40%. More complete picture of the validation API surface.

### Flask (Python)

**`get_context("HTTP request handling", workspace="flask")`**
- Pivots: **Flask** (class, new!), `full_dispatch_request`, `finalize_request`
- Neighbors: 11 symbols including wsgi_app, process_response, dispatch_request, create_app examples
- Assessment: **Improved.** Added Flask class itself as a pivot (baseline had Request wrapper instead). Agent gets Flask class with __call__ -> wsgi_app entry point visible.

---

## Comparison Summary

| Category | Baseline | After | Verdict |
|----------|----------|-------|---------|
| Exact symbol lookup | Good | Good | **No regression** |
| Conceptual search (definitions) | Very poor (100% doc/comment hits) | Good (actual code symbols) | **Dramatic improvement** |
| Similar symbols | Absent | Absent | **No change** (threshold issue) |
| get_context Julie | Mixed | Significantly better pivots | **Improved** |
| get_context Zod | OK | Richer (25 vs 18 neighbors) | **Improved** |
| get_context Flask | Good | Better (Flask class as pivot) | **Improved** |

### Key Insight
The biggest win is query classification routing conceptual queries to semantic-heavy weights. Combined with enriched embeddings (file paths, callee names, field signatures, implementors), the semantic search now surfaces actual implementation code instead of documentation. This directly reduces the tokens an agent needs to spend: one `fast_search` with `search_target="definitions"` now gives relevant code on the first try for conceptual queries.

### Remaining Gap
1. **Similar symbols still absent** at 0.5 threshold. Worth investigating lower threshold.
2. **Content-mode search** (the default) doesn't benefit from semantic improvements. Agents need guidance to use `search_target="definitions"` for conceptual queries.
3. **Embedding pipeline query** in get_context didn't change, suggesting some queries are already well-served by keyword matching and the semantic additions don't shift the ranking.
