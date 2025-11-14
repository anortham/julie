# Semantic Search Review

_Last updated: 2025-11-14_

## Overview
This document captures the current state of Julie's semantic search pipeline (embedding generation, index construction, and query execution) and highlights the most important risks and improvement opportunities discovered during the audit. References to code paths use the repository layout so they can be opened directly in VS Code.

## Pipeline Summary
- **Data extraction** – Tree-sitter extractors populate the `symbols` table with per-language metadata. Embedding text is derived from symbol name/kind/signature/doc-comment plus specialized handling for `.memories/` JSON (`src/embeddings/mod.rs`).
- **Embedding generation** – `generate_embeddings_from_sqlite` batches all symbols returned by `SymbolDatabase::get_symbols_without_embeddings()` and persists vectors to SQLite before rebuilding HNSW (`src/tools/workspace/indexing/embeddings.rs`).
- **Index creation** – `VectorStore::build_hnsw_index` loads every embedding into memory and writes `hnsw_index.*` files to `.julie/indexes/<workspace>/vectors/` for lazy loading (`src/embeddings/vector_store.rs`).
- **Semantic search execution** – `fast_search` routes `mode="semantic"` queries to `semantic_search_impl`, which lazily loads the vector store, embeds the user query, executes `search_similar_hnsw`, re-ranks with heuristic boosts, applies optional filters, and falls back to SQLite when needed (`src/tools/search/semantic_search.rs`).

## Key Findings & Recommendations

### 1. Semantic updates require explicit reindexing (major accuracy gap)
- **Evidence**: File watcher handlers update SQLite but never persist embeddings or touch the HNSW index. The spawned task in `src/watcher/handlers.rs` calls `engine.embed_symbols_batch(&symbols_for_embedding)` and discards the result, so neither `embedding_vectors` nor `hnsw_index.*` change.
- **Impact**: After the initial `manage_workspace index`, semantic search results remain frozen. Any code edits processed by the watcher become visible to FTS5 but not to HNSW, so "semantic" queries silently hit stale vectors (or miss functions that no longer exist).
- **Recommendation**:
  1. Persist per-file embeddings inside the watcher flow (call `bulk_store_embeddings` for new symbols and `delete_embeddings_for_symbol` for removed ones).
  2. Either (a) insert the resulting vectors into the in-memory `VectorStore` via the existing but unused `insert_batch`/`add_vector_to_hnsw` APIs, or (b) schedule a lightweight `generate_embeddings_for_files(paths)` task that rebuilds only the affected slice before updating the on-disk HNSW dump.

### 2. In-memory vector store is never refreshed after rebuilds
- **Evidence**: `handler.ensure_vector_store()` initializes `workspace.vector_store` once and early-returns thereafter (`src/handler.rs`). The background job in `src/tools/workspace/indexing/embeddings.rs` builds a brand-new HNSW graph and saves it to disk but never swaps it into the live workspace.
- **Impact**: Even if a manual reindex regenerates embeddings and writes a fresh `hnsw_index`, any running MCP session keeps serving queries from the stale in-memory graph until Julie is restarted. Users effectively need to restart the process after each reindex to see updated semantic results.
- **Recommendation**: Introduce a versioned vector-store handle (e.g., `ArcSwap<Option<VectorStore>>`) so the background task can atomically replace the live instance once the new graph is saved. Expose an invalidation hook that semantic search can check before each query to reload if the on-disk timestamp is newer.

### 3. `get_symbols_without_embeddings` keeps queuing un-embeddable symbols
- **Evidence**: `EmbeddingEngine::build_embedding_text` deliberately returns `String::new()` for markdown headings with empty doc comments and for `.memories/` symbols other than `description` (see `src/embeddings/mod.rs`). Those symbols never receive vectors, yet `SymbolDatabase::get_symbols_without_embeddings()` (used at the start of `generate_embeddings_from_sqlite`) still returns them because it simply left-joins `embeddings` (`src/database/symbols/search.rs`).
- **Impact**: Every reindex spins through the same set of "skip" symbols, bloating logs and wasting GPU/CPU time. More importantly, `index_workspace_files` sees a non-zero `symbols_needing_embeddings` count on every run, so it spawns the expensive background job even when no embeddable symbols changed.
- **Recommendation**: Add an `embeddings_skipped` flag (or derived view) so `get_symbols_without_embeddings()` filters out symbols where `build_embedding_text` would be empty. Short term, filter them directly in SQL (e.g., ignore `.memories/` unless `name='description'` and ignore markdown headings with blank docs).

### 4. HNSW rebuild scales poorly for large workspaces
- **Evidence**: `build_and_save_hnsw_index()` loads every embedding row into a `HashMap<String, Vec<f32>>` before calling `VectorStore::build_hnsw_index` (`src/tools/workspace/indexing/embeddings.rs`). With 100k symbols the temporary allocation exceeds several gigabytes, and the entire process runs single-threaded while holding the embedding engine lock.
- **Impact**: Larger monorepos will run out of memory or spend minutes reconstructing the graph, defeating the "background" promise. Because the vector store has no incremental insert path in production, users must endure the full rebuild each time they run `manage_workspace index`.
- **Recommendation**: Stream embeddings directly into the HNSW builder (avoid duplicating them in a `HashMap`), and start using `VectorStore::insert_batch` for incremental maintenance so that a full rebuild is only required when the dimensionality or model changes.

### 5. Semantic query filtering can under-deliver results
- **Evidence**: `semantic_search_impl` always asks HNSW for `(limit * 5).min(200)` candidates and then applies language/glob filters (`src/tools/search/semantic_search.rs`). If the user filters by `language="rust"` but most near neighbors are other languages, the result vector shrinks below `limit` even though many more rust hits exist deeper in the pool.
- **Recommendation**: Either dynamically widen `search_limit` when filters discard too many candidates or re-query with stricter constraints (e.g., run a second HNSW search limited to rust symbols by maintaining per-language indexes). At minimum, surface an insight in the response when filters reduce the result count so the user knows why fewer matches appear.

## Additional Observations
- Only the symbol name, kind, signature, and doc comments feed into embeddings. Bodies are never embedded, so undocumented helper functions remain hard to match semantically. Consider optionally including a truncated body (e.g., first 20 lines) guarded by token budget heuristics.
- `generate_embeddings_from_sqlite` shares a single `EmbeddingEngine` instance across all workspaces via a global `RwLock`. Concurrent reindexes for different workspaces serialize on that lock, so heavy reference workspaces slow down the primary. A per-workspace engine pool would isolate workloads.
- `vector_store.insert_batch`/`add_vector_to_hnsw` are implemented but unused, reinforcing that incremental semantic maintenance was planned but not wired up. Leaning on those APIs would simultaneously fix Findings #1 and #4.

## Next Steps
1. Decide whether to prioritize incremental semantic updates or to gate semantic search behind an explicit "last refreshed" indicator so users know when results are stale.
2. Add instrumentation (timers + counters) around embedding generation and HNSW build phases so regressions are visible when future changes land.
3. Once incremental updates exist, add regression tests covering "modify file → semantic search returns new name" to prevent future regressions.
