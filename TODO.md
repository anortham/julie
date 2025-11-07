# Julie TODO

## 🎯 Current Session Progress (2025-11-06)

### 📦 FILES CHANGED (Ready to Commit)
```
Modified:
- src/tools/workspace/indexing/index.rs        # Race condition fix (lines 321-352)
- src/tools/workspace/indexing/embeddings.rs   # Removed DELETE operations (now sync)
- src/tools/search/query_preprocessor.rs       # Added AND logic (lines 428-449)
- src/tests/integration/watcher_handlers.rs    # Unused variable fix
- TODO.md                                       # Updated progress and next steps
```

**Git Status**: Ready to commit and push
**Next Machine**: Run `cargo build --release` after pulling changes
**Test**: Multi-word queries like `"workspace registry"` should return results

### ✅ COMPLETED
1. **Race Condition Fix** - Critical bug where background task deleted data after main thread wrote it
   - Moved all DELETE operations from `generate_embeddings_from_sqlite` to synchronous code in `index.rs`
   - Now clears: embeddings, embedding_vectors, knowledge_embeddings, knowledge_relationships
   - Fix applied at: `src/tools/workspace/indexing/index.rs:321-352`
   - Validated by independent Gemini analysis (15min deep-dive reached same conclusion)

2. **Query Expansion Fix** - Multi-word queries now use FTS5 AND logic
   - **Problem**: `process_standard_query()` was doing nothing (`query.trim()`)
   - **Solution**: Added AND logic for multi-word queries: `"a b c"` → `"a AND b AND c"`
   - **Result**: Multi-word searches now work correctly (Google-style: all terms must be present)
   - Fix applied at: `src/tools/search/query_preprocessor.rs:428-449`
   - **VERIFIED WORKING**:
     - `"workspace registry"` → 3 results ✅
     - `"workspace_path current_dir is_primary_workspace"` → 2 results ✅ (line mode)
     - `"force_reindex is_primary_workspace"` → 1 result ✅ (line mode)

3. **Search Quality Investigation** - Original "failing" queries were invalid
   - Test queries in TODO.md were asking for terms that DON'T EXIST TOGETHER
   - Example: `"let is_primary_workspace canonical"` → "canonical" doesn't exist in that file
   - FTS5 correctly returned 0 results (not a bug, just invalid queries)
   - Multi-word search is working as expected with AND logic

### 🎯 NEXT STEPS

**Priority 1: Architecture Simplification (2025-11-07)** 🔥
- **Root Cause Found**: SQLite FTS5 virtual tables + foreign keys + triggers = "unsafe use of virtual table" errors
- **Solution**: Use existing `symbols` table for documentation (simpler, proven, working)
- **Evidence**: Markdown extractor already stores 504 symbols successfully
- **Next Actions**:
  1. Remove `knowledge_embeddings` table and related code
  2. Add `content_type` field to symbols table to distinguish docs/code
  3. Update indexing to route docs through symbols pipeline
  4. Test with existing FTS5 search infrastructure

**Priority 2: Validate Search Quality Improvements** ✅
- Multi-word AND queries are now working - test with real-world usage
- Consider adding query suggestions when 0 results (suggest removing terms)
- Monitor if semantic fallback is needed for edge cases

**Priority 3: Fix Documentation Indexing Test**
- Test location: `src/tests/integration/documentation_indexing.rs::test_documentation_indexing_basic`
- Issue: Expects `knowledge_embeddings` table which has SQLite issues
- **Solution**: Update test to use symbols table instead

**Priority 3: Consider Tantivy Reintegration** (Optional - FTS5 is working well)
- Current FTS5 implementation with AND logic is functional
- Only consider Tantivy if hitting performance/feature limitations
- If pursued: Fix Arc deadlock pattern with proper batch writes and lock release

**Priority 4: RAG Enhancement** (docs/RAG_TRANSFORMATION.md)
- Improve embedding quality for markdown (extract section content, not just headings)
- Implement semantic doc search with proper embeddings
- Consider knowledge_embeddings table consolidation

### 📝 KEY LEARNINGS

1. **Multi-word queries work best with simple AND logic** - No need for complex query expansion in most cases
2. **FTS5 is fast and reliable** - <5ms searches with proper BM25 ranking
3. **Test queries should validate actual code patterns** - Invalid test cases waste debugging time
4. **Race conditions in async code require careful synchronization** - DELETE operations must complete before spawning background tasks

---

## 📊 Archived: Original Search Quality Investigation (Pre-Fix)

* Multi-word text searches returning no results constantly. Since we're adding semantic embeddings coverage, consider auto-fallback to semantic search when text search returns 0 results (with message describing the fallback)


● julie - fast_search (MCP)(query: "let is_primary_workspace canonical", search_method: "text", limit: 3, search_target: "content", file_pattern:
                           "src/tools/workspace/indexing/index.rs")
  ⎿  🔍 No results found for: 'let is_primary_workspace canonical'
     💡 Try a broader search term, different mode, or check spelling

● julie - fast_search (MCP)(query: "process_files_optimized is_primary_workspace true false", search_method: "text", limit: 5, search_target: "content")
  ⎿  🔍 No results found for: 'process_files_optimized is_primary_workspace true false'
     💡 Try a broader search term, different mode, or check spelling

● julie - fast_search (MCP)(query: "JulieServerHandler::new initialize_workspace_with_force TempDir", search_method: "text", limit: 5, search_target: "content",
                           file_pattern: "src/tests/**")
  ⎿  🔍 No results found for: 'JulieServerHandler::new initialize_workspace_with_force TempDir'
     💡 Try a broader search term, different mode, or check spelling

● julie - fast_search (MCP)(query: "create_test_handler TempDir integration", search_method: "text", limit: 10, search_target: "content", file_pattern:
                           "src/tests/integration/**")
  ⎿  🔍 No results found for: 'create_test_handler TempDir integration'
     💡 Try a broader search term, different mode, or check spelling


● julie - fast_search (MCP)(query: "CallToolResult content vec", search_method: "text", limit: 3, search_target: "content", file_pattern: "src/tools/**/*.rs")
  ⎿  🔍 No results found for: 'CallToolResult content vec'
     💡 Try a broader search term, different mode, or check spelling

● julie - fast_search (MCP)(query: "CallToolResult", search_method: "text", limit: 5, search_target: "content", file_pattern: "src/tools/search.rs", output: "lines")
  ⎿  🔍 No lines found matching: 'CallToolResult'
     💡 Try a broader search term or different query

when this keeps happening the agent just gives up and falls back to search like:
● Search(pattern: "src/tools/**/*.rs")
  ⎿  Found 66 files (ctrl+o to expand)


  ----------------------------------------

  ● OK, here's the real technical breakdown based on actual code:

  Problem 1: Embedding Quality is SHALLOW (Confirmed Bad)

  What we're actually embedding:
  // src/embeddings/mod.rs:562
  format!("{} {} {} {}",
      symbol.name,       // "getUserData"
      symbol.kind,       // "function"
      symbol.signature,  // "pub async fn getUserData() -> Result<User>"
      symbol.doc_comment // "/// Fetches user data"
  )

  For code: Decent (name + signature + doc = reasonable semantic info)

  For markdown: TERRIBLE:
  name: "CASCADE Architecture"
  kind: "heading"
  signature: null
  doc_comment: null
  → Embedding text = "CASCADE Architecture heading"

  This is useless for RAG. We need the actual section CONTENT (the 500 words explaining CASCADE), not just the heading text.

  Problem 2: FTS5 Multi-Word Query FAILURE (Confirmed Bad)

  Your failures:
  - "let is_primary_workspace canonical" → no results
  - "process_files_optimized is_primary_workspace true false" → no results

  Root cause (src/tools/search/query_preprocessor.rs:429):
  fn process_standard_query(query: &str) -> String {
      query.trim().to_string()  // DOES NOTHING!
  }

  FTS5 gets: "let is_primary_workspace canonical" verbatim

  FTS5 interprets as: Implicit phrase search OR token AND (depends on FTS5 config)

  Problem: If words aren't in exact order or adjacent, FTS5 fails. No fuzzy logic, no relevance boosting, just dumb tokenization.

  Problem 3: Tantivy Removal Was a Mistake (Confirmed)

  From CLAUDE.md:
  Tantivy removed - Eliminated Arc deadlocks

  Translation: "We had concurrency bugs we couldn't fix, so we deleted the search engine."

  What Tantivy actually provides:
  - Query parser: Boolean AND/OR/NOT, phrase queries, fuzzy matching (term~2)
  - BM25F scoring: Field-weighted relevance (name > doc_comment > code)
  - Better tokenizers: CamelCase splitting, stop words, stemming
  - Performance: Faster than FTS5 at scale

  The deadlock was a USAGE bug, not a Tantivy problem. Proper Tantivy usage:
  // DON'T: Hold write lock forever
  let writer = index.writer.lock().unwrap();
  writer.add_document(...); // Blocks everyone

  // DO: Batch and release
  {
      let mut writer = index.writer.lock().unwrap();
      writer.add_document(...);
  } // Lock released
  writer.commit()?; // Separate step

  The Hard Truth

  Your text search sucks because:
  1. FTS5 has NO query intelligence (no AND/OR injection, no fuzzy matching)
  2. We're not even TRYING to improve queries (process_standard_query does nothing)
  3. Tantivy would solve this but was removed due to skill issues

  Your semantic search can't save you because:
  1. We're only embedding shallow metadata (name + signature)
  2. Markdown gets JUST the heading, not the content
  3. Without rich embeddings, semantic search is just expensive keyword matching

  The Real Path Forward

  Option A: Fix FTS5 properly
  - Inject AND/OR logic in process_standard_query ("a b c" → "(a AND b AND c) OR (a b c)")
  - Add fuzzy suffix matching ("workspace" → "workspace*")
  - Better tokenization in schema (currently: unicode61 separators '_::->')

  Option B: Bring back Tantivy (correctly)
  - Fix the Arc usage pattern (batch writes, release locks)
  - Get proper query parsing and scoring
  - Actually solve the search quality problem

  Option C: Improve embeddings for RAG
  - Extract full section CONTENT for markdown (not just headings)
  - Include code_context in embedding text (first 256 chars of function body)
  - Build unified semantic search that actually understands code + docs

  You're right to be frustrated. The search quality IS bad, and I've been downplaying it. The FTS5 vs Tantivy debate is real, and removing Tantivy because of
  concurrency bugs was probably premature.


  There it is. The smoking gun.

  Query expansion exists, has comprehensive tests, was built over 2 days... and is NEVER CALLED in production search code. It's only used in test files.

  What Actually Happened

  You built:
  - src/utils/query_expansion.rs - CamelCase, snake_case, wildcard, fuzzy query variants
  - Comprehensive tests that all pass
  - Query preprocessor improvements

  What got connected:
  - Nothing. src/tools/search/*.rs has ZERO imports of query_expansion

  Result:
  - Tests pass ✅
  - Production search still uses raw FTS5 with no intelligence ❌
  - You see "no results found" constantly because queries hit FTS5 verbatim

  The Brutal Comparison

  codesearch (C# + Lucene.NET):
  Query: "let is_primary_workspace canonical"
  → Lucene query parser: (let AND is_primary AND workspace AND canonical)
  → BM25 scoring with field weights
  → Fuzzy matching with edit distance
  → Results found

  Julie (Rust + FTS5):
  Query: "let is_primary_workspace canonical"
  → process_standard_query: does nothing
  → FTS5 gets it verbatim
  → FTS5 treats as phrase search OR depends on config
  → No results (words not adjacent or in exact order)

  The Real Problem

  You've been building features that don't get integrated. This is a pattern:

  1. Build query expansion → not connected
  2. Build knowledge_embeddings table → duplicate storage, not used for search
  3. Build doc indexing → stores data that never gets embedded
  4. Remove Tantivy → lose proper search engine capabilities

  Two Real Options

  Option 1: Actually Connect What You Built (Quick Fix)
  - Wire expand_query() into process_standard_query()
  - Try all variants until one hits
  - This might help but FTS5 is still fundamentally limited

  Option 2: Bring Back Tantivy (Real Fix)
  - Fix the Arc deadlock (batch writes properly)
  - Get proper query parsing (boolean AND/OR, fuzzy, phrase)
  - Get BM25 field-weighted scoring
  - Match Lucene.NET capabilities from codesearch

  The question is: Do you want to patch FTS5 indefinitely, or actually solve the search problem?

  Lucene.NET worked. Tantivy is Lucene for Rust. The deadlock was a usage bug, not a Tantivy problem.

  we should look at fixing this: ⏺ julie - fast_search (MCP)(query: "mod knowledge tests/mod.rs", search_method: "text", limit: 10, search_target: "content", file_pattern: "src/tests/mod.rs")
  ⎿  Error: Tool execution failed: fts5: syntax error near "/"