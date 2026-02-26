# Phase 2: Hybrid Search Integration Design

**Date:** 2026-02-26
**Status:** Approved
**Depends on:** Phase 1 (semantic embedding layer) — merged at `2add886`

## Goal

Integrate semantic embeddings into `get_context` and `fast_search` tools using Reciprocal Rank Fusion (RRF) to merge Tantivy keyword results with sqlite-vec KNN results.

**Exit criteria:** `get_context(query="how does text search work")` ranks `text_search_impl`, `TextSearchTool`, `search_symbols` as top pivots instead of test methods or docs.

## Key Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| First tool | `get_context` | Highest value — NL queries are its primary use case |
| Merge strategy | RRF (k=60) | Rank-based, no score normalization needed, well-studied |
| Activation | Always-on | RRF naturally handles both identifier and NL queries |
| Integration point | Pipeline entry (Approach A) | Minimal disruption — same output type, downstream unchanged |

## Architecture

### New Module: `src/search/hybrid.rs`

Self-contained RRF merger. Takes two ranked lists, produces unified `SymbolSearchResults`.

```
hybrid_search(query, db, search_index, embedding_provider, filter, limit)
  → Tantivy: search_symbols(query, filter, limit * 2)      // over-fetch for merge pool
  → KNN:     embed_query(query) → knn_search(vector, limit * 2)
  → Convert KNN symbol_ids to SymbolSearchResult via DB batch lookup
  → RRF merge (k=60) → top `limit` results
  → Return SymbolSearchResults (same type as before)
```

**Graceful degradation:** If embedding provider is `None` or KNN returns empty (embeddings not indexed yet), fall through to Tantivy-only — identical to current behavior.

### RRF Formula

```
RRF(d) = Σ 1 / (k + rank_i(d))
```

Where `k = 60` (standard constant) and `rank_i(d)` is the rank of document `d` in ranked list `i`. Documents appearing in only one list get a single term. Higher RRF score = better.

### Changes to `get_context` Pipeline

**`run_pipeline`:** Step 1 calls `hybrid_search()` instead of `search_index.search_symbols()`. Function signature gains `Option<&dyn EmbeddingProvider>` and `&SymbolDatabase` for KNN. Everything downstream (pivot selection, graph expansion, token allocation, formatting) is unchanged.

**`run` (async wrapper):** Passes workspace's `embedding_provider` through to `run_pipeline` inside `spawn_blocking`. For reference workspaces, embeddings won't be available — graceful degradation handles this.

### Changes to `fast_search`

After `get_context` is working: when `is_nl_like_query` is true AND keyword results are sparse (< 3 results), run KNN fallback and merge via RRF. Simpler case — no pivot/graph logic needed.

### KNN → SymbolSearchResult Conversion

KNN returns `(symbol_id, distance)` pairs. To produce `SymbolSearchResult` objects compatible with the existing pipeline:

1. Batch-fetch symbol metadata from SQLite by ID
2. Construct `SymbolSearchResult` with:
   - Symbol fields from DB (name, kind, file_path, line, signature, etc.)
   - `score` derived from distance (1.0 - normalized_distance or similar)
   - `matched_field` set to a sentinel like `"semantic"` for debugging

## Testing Strategy

- **Unit tests** for RRF merger: deterministic inputs, verify ranking math, edge cases (empty lists, disjoint sets, single-list fallback)
- **Integration tests**: embed a small set of symbols, run hybrid search, verify semantic results appear alongside keyword results
- **Dogfood tests**: `get_context(query="how does text search work")` on Julie codebase — top pivots should be `text_search_impl`, `TextSearchTool`, `search_symbols`

## Non-Goals

- Changing Tantivy scoring or centrality boosting (those stay as-is)
- Adding semantic search to `deep_dive` or `fast_refs` (Phase 3)
- Tuning RRF k-constant or adding per-query-type weighting (future optimization)
- Reference workspace embeddings (primary workspace only for now)
