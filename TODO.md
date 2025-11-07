# Julie TODO

## 🎯 Current Status (2025-11-07)

### 🏆 RAG POC COMPLETE - v1.1.0 Release

**Status**: ✅ Production Ready
**Achievement**: 88.9% average token reduction (83-94% range)
**Tests**: All 20 markdown extractor tests passing

### 📦 FILES CHANGED (Ready to Commit)
```
Modified:
- src/extractors/markdown/mod.rs               # Enhanced content extraction (all types)
- src/tests/extractors/markdown/mod.rs         # Added RAG validation tests
- src/tools/workspace/indexing/index.rs        # Removed knowledge_embeddings refs
- src/tests/integration/documentation_indexing.rs # Updated comments
- docs/RAG_POC_PROGRESS.md                     # Marked 100% complete
- docs/RAG_TRANSFORMATION.md                   # Updated with results
- TODO.md                                       # This file (cleaned up)
```

### ✅ COMPLETED THIS SESSION

1. **Architecture Cleanup** - Removed knowledge_embeddings complexity
   - Removed all 3 references to knowledge_embeddings tables
   - Verified content_type field already in symbols table
   - Simplified to proven infrastructure
   - Zero breaking changes

2. **Markdown Enhancement** - Capture full section bodies for RAG
   - **Before**: Only captured paragraph nodes (~50 chars)
   - **After**: Captures ALL content (lists, code, blockquotes, tables) (~280+ chars)
   - Added `is_content_node()` helper for comprehensive extraction
   - Stores rich content in `doc_comment` field for embeddings

3. **POC Validation** - Demonstrated token reduction
   - **Test 1**: CASCADE query → 83.5% reduction (2,151 → 355 tokens)
   - **Test 2**: Semantic Tantivy query → 94.3% reduction (9,220 → 525 tokens)
   - **Average**: 88.9% reduction (target was >85%) ✅
   - Both FTS5 (text) and HNSW (semantic) searches working
   - Content quality maintained (complete explanations with context)

### 🎯 NEXT STEPS

**Priority 1: Ship v1.1.0 Release** 🚀
- Commit all changes with comprehensive message
- Tag release as v1.1.0
- Push to origin/main
- Celebrate milestone!

**Priority 2: Production Rollout** (Future)
- Agent onboarding optimization
- Documentation search integration
- Cross-reference linking (code ↔ docs)
- Query suggestion improvements

**Priority 3: Additional Documentation Support** (Future)
- Consider PDF support for external docs
- DOCX support for Word documents
- Plain text files (.txt)
- Org-mode files if needed

### 📝 KEY LEARNINGS

1. **Simpler is better** - Existing symbols table worked perfectly, no need for knowledge_embeddings complexity
2. **Content extraction is key** - Full section bodies (not just headings) enable true RAG token reduction
3. **SQLite FTS5 is sufficient** - <5ms searches with BM25 ranking, no Tantivy needed for now
4. **Test-driven validation works** - Measured token reduction proves the value proposition

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
  If we add Tantivy back, how do we fit it into our architecture without causing problems? What functionality does it bring to the system that we can leverage above and beyond simple text search?

  we should look at fixing this: ⏺ julie - fast_search (MCP)(query: "mod knowledge tests/mod.rs", search_method: "text", limit: 10, search_target: "content", file_pattern: "src/tests/mod.rs")
  ⎿  Error: Tool execution failed: fts5: syntax error near "/"

  The TOP TIER GOAL of the RAG updates has got to be token reduction. We need the agent to be able to find code with better accuracy and fewer tokens. The agent needs to be able to consume documentation with fewer tokens used. This project has grown and so has the documentation. It's become difficult to get any decent sized chunk of work done without running our of context because of all the code that needs to be understood and documenation that needs to be read. This has to be prioritiy one for the RAG changes.

  Are we missing any other tree-sitter parsers we need to add more documentation to our RAG? What about other file types that could have important info in them to add to the RAG? PDFs? docx? what all can we support? If we REALLY want to build a RAG out of the codebase we need to support as many types as we can.

Should we add a semantic fallback results for text search results that return 0 results? 

Should we add a new tool specifically for semantic searching instead of it being a "mode" in fast_search? What other tools we already have could benefit from a semantic search "layer"?


we should add YAML treesitter support next

Our symbols.db-wal file is staying a larger filesize than I would have expected, we should invetigate.

