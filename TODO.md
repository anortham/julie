# TODO

 • Agent Takeaways
   - Codex agent surfaced operational gaps: semantic search ignoring requested workspace (src/tools/search.rs), FTS fallback returning non-actionable pseudo-symbols,
   line-mode search bypassing filters, and missing telemetry.
   - Claude agent emphasized structural search tooling, richer symbol filtering, scoped queries, and enriching result context to boost developer ergonomics.
   - Gemini agent reinforced the need for structured query syntax, improved ranking signals, disambiguation UX, search history, and stronger natural-language handling.
   - Cloud and Qwen agents failed (missing credentials); no input gathered from them.

   **VALIDATION RESULTS (2025-10-17):**

   ✅ FIXED (2025-10-17): "FTS fallback returning non-actionable pseudo-symbols"
      ✓ sqlite_fts_search() now parses file content to find precise line numbers
      ✓ Creates proper symbols with real line locations (start_line, end_line)
      ✓ Match content in code_context field with actual line content
      → See: src/tools/search/text_search.rs:184-278

   ✅ FIXED (2025-10-17): "Line-mode search bypassing filters"
      ✓ line_mode_search() now accepts language and file_pattern parameters
      ✓ Language filtering by extension (rust, typescript, python, etc.)
      ✓ File pattern filtering using glob patterns (src/**, tests/**)
      ✓ Test coverage: test_fast_search_line_mode_language_filter (passing)
      → See: src/tools/search/line_mode.rs:19-26, src/tests/tools/search/line_mode.rs:382-491

   ⏳ DEFERRED: "Missing telemetry" - NO PERFORMANCE TRACKING
      ✗ No timing instrumentation in any search implementation (text/semantic/hybrid/line)
      ✗ No latency measurements, no fallback counters, no confidence tracking
      → REASON: User prioritized fixing line-mode filtering and FTS precision first
      → Future work: Add structured timing metrics and performance counters

   Remaining Roadmap (from Integrated Plan)
   - Expand Filtering & Scoping: Extend FastSearchTool to accept symbol kind, visibility, documentation, include/exclude glob lists, and "within symbol" scoping so
   results stay code-specific.
   - Introduce Structural Search: Ship a dedicated structural/relationship search tool leveraging relationships data for call graphs, implementations, and dependency
   exploration; plan phased AST/regex pattern search on stored code_context as follow-up.
   - Elevate Ranking Signals: Incorporate file importance, symbol kind boosts, and recency into ranking, and expose mode-level telemetry (latency, fallback counts,
   confidence) for future tuning.
   - Enhance Result UX: Enrich results with hierarchy breadcrumbs, reference counts, configurable context windows, and mode-aware next actions; add interactive
   disambiguation when multiple definitions surface.
   - Improve Ergonomics & Memory: Provide structured query syntax, optional search history, and quick suggestions drawn from prior queries and workspace metadata;
   document the syntax for users.
   - Natural Language Evolution: Evaluate upgrading semantic tier with better embeddings or fine-tuned models to close Gemini's NL gap once telemetry validates baseline
   stability.

/e


#FAST_SEARCH
   ⏺ julie - fast_search (MCP)(query: "get_symbols", mode: "text", limit: 20)
  ⎿  ⚠ Large MCP response (~14.0k tokens), this can fill up context quickly
  ⎿  {
       "confidence": 1,
       "insights": "Mostly Methods (14 of 20)",
     … +723 lines (ctrl+o to expand)

     we fixed the context issues with fast_search, I think we need to spend some time find tuning our results. The better the results are, the fewer we need to return by default.

# missing implementations
  - look for "coming soon", "TODO", "Stub" , etc


# database logging
2025-10-17T16:31:50.259135Z  INFO julie::database::embeddings: src/database/embeddings.rs:166: ✅ Bulk embedding storage complete! 100 embeddings in 2ms (34167 embeddings/sec)
2025-10-17T16:31:50.259155Z  INFO julie::tools::workspace::indexing::embeddings: src/tools/workspace/indexing/embeddings.rs:92: 🔄 Processing embedding batch 65/67 (100 symbols)
2025-10-17T16:31:50.684274Z  INFO julie::database::embeddings: src/database/embeddings.rs:166: ✅ Bulk embedding storage complete! 100 embeddings in 2ms (39845 embeddings/sec)
2025-10-17T16:31:50.684294Z  INFO julie::tools::workspace::indexing::embeddings: src/tools/workspace/indexing/embeddings.rs:92: 🔄 Processing embedding batch 66/67 (100 symbols)
2025-10-17T16:31:53.401834Z  INFO julie::database::embeddings: src/database/embeddings.rs:166: ✅ Bulk embedding storage complete! 100 embeddings in 2ms (43612 embeddings/sec)
2025-10-17T16:31:53.401855Z  INFO julie::tools::workspace::indexing::embeddings: src/tools/workspace/indexing/embeddings.rs:92: 🔄 Processing embedding batch 67/67 (36 symbols)

we should make the src/database/embeddings.rs line DEBUG instead of info, too much clutter in the logs.

2025-10-17T16:35:10.194188Z  INFO julie::tools::symbols: src/tools/symbols.rs:82: 📋 Getting symbols for file: src/tools/search/semantic_search.rs (depth: 1)
2025-10-17T16:35:10.194583Z  INFO julie::handler: src/handler.rs:499: ✅ Tool executed successfully
2025-10-17T16:36:24.599409Z  INFO julie::tools::symbols: src/tools/symbols.rs:82: 📋 Getting symbols for file: src/tools/search/hybrid_search.rs (depth: 1)
2025-10-17T16:36:24.599654Z  INFO julie::handler: src/handler.rs:499: ✅ Tool executed successfully
2025-10-17T16:37:07.089766Z  INFO julie::handler: src/handler.rs:499: ✅ Tool executed successfully
2025-10-17T16:37:14.959127Z  INFO julie::tools::symbols: src/tools/symbols.rs:82: 📋 Getting symbols for file: src/handler.rs (depth: 1)
2025-10-17T16:37:14.959934Z  INFO julie::handler: src/handler.rs:499: ✅ Tool executed successfully

we also don't need "Tool executed successfully" to be INFO, change that to DEBUG too


# VECTOR_STORE ARCHITECTURE - REDUNDANT MEMORY USAGE

**Problem**: VectorStore currently loads ALL embeddings from database into memory (HashMap<String, Vec<f32>>) when loading HNSW index for reference workspaces. This is wasteful and redundant.

**Current Flow** (semantic_search.rs for reference workspaces):
```rust
// 1. Load ALL embeddings from database into HashMap
let embeddings = db_lock.load_all_embeddings("bge-small")?;
for (symbol_id, vector) in embeddings {
    store.store_vector(symbol_id, vector)?;  // Stores in HashMap
}

// 2. Load HNSW index (which ALREADY contains the vector data)
store.load_hnsw_index(&vectors_dir)?;
```

**Why This Happens**:
- load_hnsw_index() needs vectors.keys() to rebuild id_mapping (line 399-402 in vector_store.rs)
- search_similar_hnsw() needs vectors HashMap for cosine similarity calculation (line 285-294)
- HNSW files (.graph + .data) already contain all vector data, but VectorStore keeps a second copy

**Impact**:
- For large codebases with millions of symbols, this means loading gigabytes of embeddings twice
- Once in HNSW internal storage (loaded from .data file)
- Once in VectorStore's HashMap (loaded from SQLite database)

**Potential Solutions**:
1. **Lazy Loading**: Only load embeddings from DB for the specific symbol IDs returned by HNSW search
2. **HNSW-First Architecture**: Store id_mapping in HNSW files, eliminate need for vectors HashMap entirely
3. **Hybrid**: Keep HashMap for primary workspace (built in memory), use lazy loading for reference workspaces

**Related Code**:
- src/embeddings/vector_store.rs (load_hnsw_index, search_similar_hnsw)
- src/tools/search/semantic_search.rs (lines 62-116 - reference workspace loading)
- src/workspace/mod.rs (initialize_vector_store - primary workspace pattern)

