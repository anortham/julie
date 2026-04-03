# Embedding Enrichment: Behavioral Fingerprints for Semantic Intelligence

**Date:** 2026-04-03
**Status:** Draft
**Scope:** Embedding layer enrichment + query classification + tool consumer tuning + workflow updates

## Problem

Julie's embeddings are built from thin metadata: `kind + name + signature first line + doc excerpt`. This produces strings like:

```
function process_payment(amount: f64) -> Result<Receipt>
```

Two functions with identical signatures but completely different logic get similar embeddings. Two functions that do conceptually the same thing with different parameter names are distant. The semantic side of hybrid search, the similar-symbols section of deep_dive, and the fast_refs semantic fallback all underperform because the embedding text captures *what code looks like*, not *what code does*.

We now have rich tree-sitter data (callees, type relationships, field accesses, parent/child structures) that isn't being leveraged for embeddings.

## Goal

Enrich embedding input text with behavioral context from the tree-sitter graph, tune how existing tools consume semantic results, and update workflow guidance. No new MCP tools.

**Evaluation lens:** Every change must reduce the total tokens an agent needs to acquire the understanding required to complete a task. Julie's core value proposition is intelligence per token.

## Non-Goals

- Changing the embedding model (CodeRankEmbed stays)
- Changing the storage layer (sqlite-vec stays)
- Adding new MCP tools or endpoints
- Reranking layers or cross-encoder pipelines
- GraphRAG or new graph traversal capabilities

## Design

### 1. Benchmark Baseline (Before)

Run a fixed set of Julie tool calls against 4 indexed workspaces, capture results to a file.

**Test workspaces:**
- Julie (Rust) -- primary workspace, sanity check
- Zod (TypeScript) -- variable-heavy, arrow functions as variables
- Flask (Python) -- decorator-heavy, docstring-rich
- Cobra (Go) -- no class hierarchy, different project structure

**Query categories:**
1. **Exact symbol lookup** -- `fast_search(query="<symbol>", search_target="definitions")` -- control group, should already work well
2. **Conceptual search** -- `fast_search(query="<natural language description>")` -- where semantic improvement should be visible
3. **Similar symbols** -- `deep_dive(symbol="<well-known symbol>")` -- capture the "similar symbols" section
4. **Context orientation** -- `get_context(query="<domain concept>")` -- measure pivot relevance

**Output:** Markdown file with raw results, saved to `benchmarks/embedding_enrichment/`. Run before and after changes for comparison. No scoring framework; results are self-evidently better or worse.

### 2. Embedding Text Enrichment

Core change to `format_symbol_metadata` in `src/embeddings/metadata.rs` and `prepare_batch_for_embedding`.

**Current format:**
```
function process_payment(amount: f64) -> Result<Receipt>
```

**New format:**
```
function process_payment(amount: f64) -> Result<Receipt>
calls: validate_amount, stripe_api::charge, log_transaction
in: billing/processor.rs
```

**Enrichment by symbol kind:**

| Symbol Kind | Current | Added |
|---|---|---|
| Function/Method | kind + name + sig + doc | + callee names (from `callees_by_symbol`), + relative file path |
| Class/Struct | kind + name + child method names | + child field signatures (captures type info, e.g., `name: String`), + implemented traits/interfaces, + file path |
| Trait/Interface | kind + name + child method names | + known implementors (from relationships), + file path |
| Enum | kind + name + variant names | + file path |
| Module/Namespace | kind + name | + exported symbol names/kinds, + file path |
| Variable (budgeted) | kind + name + sig + doc | + assigned value type if available, + file path |

**Constraints:**
- Total embedding text stays under ~500 chars to keep signal dense
- Callee lists capped at ~8 names
- File path is relative, Unix-style (already how Julie stores paths)
- No raw code bodies; behavioral fingerprints only

**Data availability:** `callees_by_symbol` is already passed to `prepare_batch_for_embedding`. File path is on the `Symbol` struct (`file_path` field). Implemented traits/interfaces come from the relationships table (Implements/Extends edges). Child field signatures come from Property/Field child symbols' `signature` field. Parent/child relationships are already resolved in `prepare_batch_for_embedding` for child method enrichment; extending to field signatures and trait implementations follows the same pattern.

**Files changed:**
- `src/embeddings/metadata.rs` -- `format_symbol_metadata`, `prepare_batch_for_embedding`
- `src/embeddings/pipeline.rs` -- pass additional data (relationships map) to the metadata formatter
- `src/tests/core/embedding_metadata.rs` -- update tests for new format
- `src/tests/core/embedding_metadata_enrichment.rs` -- update enrichment tests

### 3. Query Classification for Hybrid Search

Add a lightweight query classifier that adjusts `SearchWeightProfile` dynamically based on query characteristics.

**Classification heuristic (pattern matching, no ML):**

| Signal | Classification |
|---|---|
| Query matches known symbol patterns (snake_case, CamelCase, ::, .) | `SymbolLookup` |
| Query is 4+ words, no code-like tokens | `Conceptual` |
| Mixed signals | `Mixed` |

**Weight profiles per classification:**

| Classification | keyword_weight | semantic_weight |
|---|---|---|
| SymbolLookup | 1.0 | 0.3 |
| Conceptual | 0.5 | 1.0 |
| Mixed | 0.8 | 0.8 |

Only applies when `hybrid_search` is called without an explicit weight profile override. Tools can still pass their own profiles.

**Implementation:** New function `classify_query(query: &str) -> QueryIntent` in `src/search/weights.rs`. Called at the top of `hybrid_search` when no explicit profile is provided.

**Files changed:**
- `src/search/weights.rs` -- `QueryIntent` enum, `classify_query` function, profile mappings
- `src/search/hybrid.rs` -- use classification when no explicit profile is passed
- New tests in `src/tests/tools/` for query classification

### 4. Tool Consumer Improvements

**`get_context` pivot selection:**
Add embedding similarity to the query as a tiebreaker when selecting pivots. When two candidates have similar keyword scores, prefer the one semantically closer to the query. This improves pivot relevance for conceptual queries.

**Files changed:**
- `src/tools/get_context/pipeline.rs` -- incorporate embedding similarity in pivot ranking

**`deep_dive` similar symbols:**
No code changes. Benefits from richer embeddings producing more meaningful similarity results.

**`fast_refs` semantic fallback:**
`QUERY_SIMILARITY_THRESHOLD` (currently 0.2) may need re-tuning after embedding enrichment. Evaluate during benchmark comparison; adjust if needed.

**`hybrid_search`:**
Query classification integration (Section 3 above).

### 5. Workflow & Instruction Updates

**MCP tool descriptions:**
Update `fast_search` description to indicate that conceptual/natural language queries are supported and effective. Currently the description only mentions "multi-word queries with AND/OR logic."

**SessionStart hook:**
Add guidance for when to use conceptual search vs exact symbol lookup.

**Julie plugin skills:**
Update `explore-area`, `logic-flow`, and `call-trace` skills to suggest conceptual `fast_search` as an entry point when the user describes behavior rather than naming a symbol.

**What stays the same:**
The core workflow (search -> deep_dive -> implement) is unchanged. The "search" step becomes more powerful for natural language queries.

## Implementation Order

1. **Benchmark baseline** -- capture before-state across 4 workspaces
2. **Embedding text enrichment** -- the foundational change (TDD: write tests for new format first)
3. **Re-index test workspaces** -- rebuild embeddings with enriched text
4. **Benchmark comparison** -- capture after-state, compare
5. **Query classification** -- implement and test the heuristic
6. **get_context tiebreaker** -- add embedding similarity to pivot selection
7. **Second benchmark comparison** -- validate the full stack improvement
8. **Threshold tuning** -- adjust fast_refs fallback threshold if needed
9. **Workflow updates** -- tool descriptions, SessionStart hook, skills
10. **Final benchmark** -- confirm end-to-end improvement

## Risks

- **Richer text could dilute signal:** If callee names are noisy or irrelevant, they could push embeddings away from the right neighborhood. Mitigated by capping at ~8 callees and the 500-char budget.
- **Query classification heuristic could misfire:** A query like "Config" could be classified as SymbolLookup when the user meant conceptual. Mitigated by keeping the heuristic conservative (only classify as Conceptual when strong NL signals are present).
- **Threshold shifts:** Enriched embeddings will change the similarity score distribution. The 0.5 (symbol-to-symbol) and 0.2 (query-to-symbol) thresholds may need adjustment. Mitigated by the benchmark comparison step.
- **Re-indexing cost:** All workspaces need full re-embedding after the metadata change. This is a one-time cost per workspace, handled by the existing pipeline with `force: true`.
