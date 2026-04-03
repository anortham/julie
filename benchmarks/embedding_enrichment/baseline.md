# Embedding Enrichment Benchmark: Baseline

**Date:** 2026-04-03
**Commit:** f1bf9ac0
**Model:** CodeRankEmbed (768d, sidecar)
**Workspaces:** Julie (Rust), Zod (TypeScript), Flask (Python), Cobra (Go)

---

## 1. Exact Symbol Lookups (Control Group)

These should work well with keyword search and serve as a regression check.

### Julie (Rust)

**`fast_search("hybrid_search", definitions, limit=5)`**
- #1: `src/search/hybrid.rs:174` (function, public) -- CORRECT
- Also found: 2 import refs, 2 fixture file matches
- Assessment: **Good.** Real definition is #1.

**`fast_search("prepare_batch_for_embedding", definitions, limit=5)`**
- #1: `src/embeddings/metadata.rs:154` (function, public) -- CORRECT
- Also found: pipeline call site, 3 test call sites
- Assessment: **Good.** Real definition is #1.

### Zod (TypeScript)

**`fast_search("ZodType", definitions, limit=5)`**
- #1: `packages/zod/src/v4/classic/schemas.ts:20` (interface)
- #2: `packages/zod/src/v4/classic/tests/prototypes.test.ts:5` (interface)
- #3-4: `packages/zod/src/v3/types.ts:4952` (export, duplicate entries)
- #5: `packages/zod/src/v4/classic/compat.ts:57` (export)
- Assessment: **OK.** Found interface def, but test file is #2 (should be lower).

**`fast_search("parse", definitions, limit=5)`**
- Found 5 parse methods across bench, v3, v4/classic, v4/core, v4/mini
- Assessment: **Good.** All legitimate definitions of parse across versions.

### Flask (Python)

**`fast_search("Flask", definitions, limit=5)`**
- #1: `src/flask/app.py:109` (class, public) -- CORRECT
- #2: `tests/test_config.py:202` (class) -- test subclass
- #3: `README.md:3` (module) -- doc
- #4: `src/flask/globals.py:9` (import)
- #5: `pyproject.toml:83` (property)
- Assessment: **Good.** Real class is #1.

**`fast_search("route", definitions, limit=5)`**
- #1: `src/flask/sansio/scaffold.py:336` (method, public) -- CORRECT
- Also found: 4 content matches in test files
- Assessment: **Good.** Real definition is #1.

### Cobra (Go)

**`fast_search("Command", definitions, limit=5)`**
- #1: `command.go:54` (class/struct, public) -- CORRECT
- #2-5: Documentation markdown modules
- Assessment: **Good.** Real struct is #1, but docs pollute results.

**`fast_search("Execute", definitions, limit=5)`**
- #1: `command.go:1070` (method, public) -- CORRECT
- #2: `command.go:905` (method, private) -- internal execute
- Also: test file content match, ExecuteContext, ExecuteC
- Assessment: **Good.** Both public and private Execute found.

---

## 2. Conceptual / Natural Language Queries

This is where semantic search should help but currently doesn't.

### Julie (Rust)

**`fast_search("error handling and retry logic", limit=10)`**
- ALL 10 results from `crates/julie-extractors/src/tests/javascript/error_handling.rs`
- Matched on keyword terms ("error", "handling", "retry logic") in test file comments
- No actual error handling implementation code surfaced
- Assessment: **Poor.** Keyword-only matching. Hit a test fixture file, not real error handling code.

**`fast_search("search scoring and ranking", limit=10)`**
- Results split between `docs/TESTING_GUIDE.md` and `src/search/expansion.rs`
- Mostly doc content about test tiers, not actual scoring implementation
- Assessment: **Poor.** Docs and tangential code, not the scoring logic itself (which lives in `src/search/scoring.rs`, `src/tools/get_context/scoring.rs`).

**`fast_search("symbol extraction from source code", limit=10)`**
- ALL 10 results from `.julieignore` file
- Matched on keyword terms in comment text
- No actual extractor code surfaced
- Assessment: **Very poor.** A config file's comments, not the extractors.

### Zod (TypeScript)

**`fast_search("input validation and type checking", limit=10)`**
- ALL 10 results from `packages/docs-v3/README_ZH.md` (Chinese README)
- Matched on keyword terms in documentation
- No actual validation code surfaced
- Assessment: **Very poor.** Chinese docs, not validation logic.

### Flask (Python)

**`fast_search("request routing and middleware", limit=10)`**
- ALL 10 results from `docs/design.rst`
- Matched on keyword terms in design documentation
- No actual routing code surfaced (e.g., `scaffold.py`, `app.py`)
- Assessment: **Very poor.** Design docs, not implementation.

### Cobra (Go)

**`fast_search("command line argument parsing", limit=10)`**
- ALL 10 results from `site/content/completions/_index.md`
- Matched on keyword terms in documentation
- No actual arg parsing code surfaced (e.g., `args.go`, `command.go`)
- Assessment: **Very poor.** Static site docs, not implementation.

---

## 3. Similar Symbols (deep_dive)

### Julie (Rust)

**`deep_dive("hybrid_search", depth="context")`**
- No "Similar symbols" section in output
- Shows callers (12), callees (5), test locations (10)
- Assessment: **No semantic similarity data.** Either no embeddings match above 0.5 threshold, or symbol not embedded.

**`deep_dive("format_symbol_metadata", depth="context")`**
- No "Similar symbols" section in output
- Shows callers (16), callees (4), test locations (10)
- Assessment: **No semantic similarity data.**

**`deep_dive("SymbolDatabase", depth="full")`**
- No "Similar symbols" section in output
- Shows struct definition with conn and file_path fields
- Assessment: **No semantic similarity data.** Struct with very minimal metadata embedding ("struct SymbolDatabase") -- too generic to match anything above 0.5.

### Zod (TypeScript)

**`deep_dive("ZodType", workspace="zod")`**
- Returned 7 definitions, asked for disambiguation via context_file
- Assessment: N/A (disambiguation needed)

### Flask (Python)

**`deep_dive("Flask", workspace="flask")`**
- Returned 3 definitions (README module, app.py class, test_config.py subclass)
- No "Similar symbols" section on any
- Assessment: **No semantic similarity data.**

---

## 4. get_context Orientation

### Julie (Rust)

**`get_context("embedding pipeline and vector search")`**
- Pivots: `spawn_workspace_embedding`, `embed_query`
- Neighbors: 12 symbols (embedding-related: device_info, initialize_embedding_provider, run_embedding_pipeline_cancellable, etc.)
- Assessment: **Decent.** Found embedding infrastructure, but pivoted on the spawn/init layer rather than the core pipeline logic (prepare_batch, format_symbol_metadata). An agent would need follow-up calls.

**`get_context("how does search scoring work")`**
- Pivots: `calculate_score` (path_relevance), `ScoringConfig`, `get_reference_scores`
- Neighbors: 11 symbols
- Assessment: **Mixed.** `calculate_score` is path relevance scoring, not search result scoring. `get_reference_scores` is relevant but is the DB query, not the scoring logic. The actual `select_pivots` and `weighted_rrf_merge` scoring functions are missing from pivots.

### Zod (TypeScript)

**`get_context("type validation", workspace="zod")`**
- Pivots: `Result` (standard-schema, 2 versions), `ZodError`
- Neighbors: 18 symbols (error-related: processError, ZodIssue types, handleResult, etc.)
- Assessment: **OK.** Found error/validation result types, but missed the core schema validation logic (parse, _parse, safeParse). Focused on error reporting side.

### Flask (Python)

**`get_context("HTTP request handling", workspace="flask")`**
- Pivots: `full_dispatch_request`, `finalize_request`, `Request`
- Neighbors: 8 symbols (dispatch_request, handle_user_exception, wsgi_app, etc.)
- Assessment: **Good.** This is actually the correct request handling pipeline. An agent would get a solid understanding of Flask's request lifecycle from this.

---

## Summary

| Category | Quality | Notes |
|----------|---------|-------|
| Exact symbol lookup | **Good** | Keyword search handles this well. No change expected. |
| Conceptual search | **Very poor** | ALL conceptual queries returned docs/comments/config, not code. Semantic search is not contributing. |
| Similar symbols | **Absent** | No similar symbols shown for any tested symbol. Threshold too high or embedding metadata too thin. |
| get_context | **Mixed** | Flask was good. Julie queries picked adjacent but wrong symbols. Pivot selection is keyword-driven. |
