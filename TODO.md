# Julie TODO

## 🎯 Current Status (2025-11-07 Evening)

### ✅ RECENT COMPLETIONS (2025-11-07 Evening Session)

**Bug Fix**: Path canonicalization for reference workspaces
- Fixed: `src/extractors/base/extractor.rs` lines 45-64
- Issue: Relative paths (e.g., "COA.CodeSearch.McpServer/Services/FileIndexingService.cs") were failing canonicalization
- Solution: Join relative paths to workspace_root BEFORE canonicalizing
- Test: `test_relative_path_canonicalization()` in `src/tests/extractors/base.rs`
- Status: ✅ Complete - no more "Failed to canonicalize path" warnings

**Validated Complete (from Previous Work)**:
- ✅ Enhanced markdown embeddings with full section content (not just headings)
  - Location: `src/extractors/markdown/mod.rs` lines 72-90, 128-129
  - Now embeds section_content as doc_comment for rich RAG
- ✅ Rich embeddings including code_context
  - Location: `src/embeddings/mod.rs` lines 574-579
  - Embeddings now include: name + kind + signature + doc_comment + code_context
- ✅ Semantic search fallback when text search returns 0 results
  - Location: `src/tools/search/mod.rs` lines 279-363
  - Automatic fallback with clear user messaging
- ✅ Reference workspace support operational
  - Tested: C# codesearch codebase (281 files, 39,225 symbols)
  - Both text and semantic search working across workspaces

**Still Outstanding (Technical Debt)**:
- ❌ Query expansion NOT wired up (exists in `src/utils/query_expansion.rs` but never called in production)
- ❌ Multi-word queries still failing (FTS5 limitations without query intelligence)

### 🏆 Language #31 Complete - YAML Support Added ✅

**Status**: ✅ Production - 31 Languages Supported
**Latest Feature**: YAML extractor (CI/CD configs, Kubernetes, Docker Compose)
**Latest Release**: v1.1.3 (2025-11-07)
**Tests**: All 14 YAML extractor tests passing

### 🏆 RAG POC COMPLETE - v1.1.1 Released ✅

**Achievement**: 88.9% average token reduction (83-94% range)
**Tests**: All 20 markdown extractor tests passing

### 📦 RELEASES

**v1.1.1 (2025-11-07)** - Database Robustness and RAG Quality
- ✅ WAL checkpoint on shutdown for clean state
- ✅ WAL checkpoint method prevents unbounded growth
- ✅ Enhanced RAG embeddings with code_context (3 lines before/after)
- ✅ Semantic search fallback when text search returns 0 results
- ✅ Documentation token reduction (77% in CLAUDE.md)

**v1.1.0 (2025-11-07)** - RAG POC Complete
- ✅ 88.9% token reduction validated (83-94% range)
- ✅ Enhanced markdown content extraction (all node types)
- ✅ Simplified architecture (symbols table, no knowledge_embeddings)
- ✅ Both FTS5 (text) and HNSW (semantic) search operational

### ✅ COMPLETED - RAG INFRASTRUCTURE

1. **Architecture** ✅
   - Unified symbols table with content_type field
   - No knowledge_embeddings complexity needed
   - Proven infrastructure handles docs as first-class symbols
   - FTS5 full-text search operational (<5ms)
   - HNSW semantic search operational (<50ms)

2. **Markdown Extraction** ✅
   - Enhanced content extraction (lists, code, blockquotes, tables)
   - Rich embeddings with complete section bodies
   - Tree-sitter based extraction for consistency

3. **Token Reduction** ✅
   - **Validated**: 88.9% average reduction
   - **Test 1**: CASCADE query → 83.5% reduction
   - **Test 2**: Semantic query → 94.3% reduction
   - **Quality**: Complete explanations with context preserved

4. **Search Quality** ✅
   - Semantic fallback when text search fails (UX improvement)
   - Code_context embeddings (3 lines before/after for richer semantic understanding)
   - Both search modes working reliably

### 🎯 NEXT PRIORITIES

**Priority 1: Language Support Expansion**
- ✅ Markdown (#28) - Complete
- ✅ JSON (#29) - Complete
- ✅ TOML (#30) - Complete
- ✅ YAML (#31) - Complete (CI/CD configs: GitHub Actions, Kubernetes, Docker Compose)
- ⏸️ Dockerfile - Blocked on tree-sitter-dockerfile 0.25+ compatibility (crate uses tree-sitter 0.20)
- Consider: Plain text (.txt), CSV for data files

**Priority 2: Production Optimization** (Future)
- Agent onboarding flow improvements
- Cross-reference discovery (code ↔ docs linking)
- Query suggestion system
- Additional document formats (PDF, DOCX) if needed

**Priority 3: Search Quality Improvements** (Backlog)
- Consider Tantivy reintroduction (if FTS5 becomes limiting)
- Advanced query expansion beyond current fallback
- Search result ranking improvements

### 📝 KEY LEARNINGS

1. **Simpler is better** - Existing symbols table worked perfectly, no need for knowledge_embeddings complexity
2. **Content extraction is key** - Full section bodies (not just headings) enable true RAG token reduction
3. **SQLite FTS5 is sufficient** - <5ms searches with BM25 ranking, no Tantivy needed for now
4. **Test-driven validation works** - Measured token reduction proves the value proposition

---


  ----------------------------------------

  ● OK, here's the real technical breakdown based on actual code:

  Problem 1: Embedding Quality is SHALLOW ~~(Confirmed Bad)~~ ✅ FIXED (2025-11-07)

  **OLD (What we WERE embedding)**:
  // src/embeddings/mod.rs:562 (OLD)
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

  **NEW (What we NOW embed)** - ✅ FIXED:
  // src/embeddings/mod.rs:570-579 (CURRENT)
  - name + kind + signature (unchanged)
  - doc_comment (NOW includes full section content for markdown!)
  - code_context (3 lines before + symbol + 3 lines after)

  For markdown NOW:
  name: "CASCADE Architecture"
  kind: "module"
  doc_comment: "The 2-tier CASCADE architecture uses SQLite FTS5... [500+ words of actual content]"
  → Embedding text = Rich semantic content for RAG ✅

  **Status**: ✅ RESOLVED - Embeddings now have rich content for proper RAG token reduction

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

  Option A: Fix FTS5 properly - PARTIAL (1/3 complete)
  - ❌ Inject AND/OR logic in process_standard_query ("a b c" → "(a AND b AND c) OR (a b c)")
    - Query expansion EXISTS (src/utils/query_expansion.rs) but NOT CALLED
  - ❌ Add fuzzy suffix matching ("workspace" → "workspace*")
    - Not implemented yet
  - ✅ Path canonicalization fix (2025-11-07)
    - Fixed: src/extractors/base/extractor.rs lines 45-64
    - Relative paths now join to workspace_root before canonicalizing

  Option B: Bring back Tantivy (correctly)
  - Fix the Arc usage pattern (batch writes, release locks)
  - Get proper query parsing and scoring
  - Actually solve the search quality problem

  Option C: Improve embeddings for RAG ✅ COMPLETE (2025-11-07)
  - ✅ Extract full section CONTENT for markdown (not just headings)
    - Location: src/extractors/markdown/mod.rs lines 72-90
  - ✅ Include code_context in embedding text (3 lines before/after)
    - Location: src/embeddings/mod.rs lines 574-579
  - ✅ Semantic search fallback when text fails (unified experience)
    - Location: src/tools/search/mod.rs lines 279-363

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


Should we add a new tool specifically for semantic searching instead of it being a "mode" in fast_search? What other tools we already have could benefit from a semantic search "layer"?


PRIORITY ONE: ● julie - fast_search (MCP)(query: "process_standard_query expand_query", search_method: "text", limit: 5, search_target: "content", output: "lines")
  ⎿  Error: Tool execution failed: database disk image is malformed


---

## 📊 STATUS SUMMARY (2025-11-07 Evening)

### ✅ What's Working Well
1. **RAG Infrastructure Complete** - 88.9% token reduction validated
2. **Rich Embeddings** - Full section content + code context embedded
3. **Semantic Fallback** - Graceful degradation when text search fails
4. **Reference Workspaces** - Multi-workspace support operational
5. **31 Languages Supported** - Comprehensive language coverage

### 🔧 Outstanding Issues (Priority Order)

**Priority 1: Query Expansion Not Wired Up** (Quick Win - 1 day)
- Built but never connected to production search
- Location: src/utils/query_expansion.rs (exists)
- Needed: Import in src/tools/search/*.rs and call from process_standard_query
- Impact: Would significantly improve multi-word query success rate

**Priority 2: Database Corruption** (Blocker - requires restart)
- Current Julie MCP using old database format
- Fix: Exit Claude Code → cargo build --release → Restart Claude Code
- Impact: Blocks all Julie tool usage until restart

**Priority 3: Multi-Word Query Failures** (Fundamental - 1-2 weeks)
- Root cause: FTS5 has no query intelligence
- Options: Wire up query expansion OR bring back Tantivy
- Impact: Core search quality issue affecting daily dogfooding

### 🎯 Next Session Recommendation
1. Restart Claude Code with new build (fixes database + path canonicalization)
2. Wire up query expansion to process_standard_query (Option A completion)
3. Test multi-word queries with expansion enabled
4. Measure improvement vs. baseline

## 🔴 SQLITE CORRUPTION ROOT CAUSE ANALYSIS

**The Pattern**: Daily "database disk image is malformed" errors

**Why SQLite Corruption Shouldn't Happen**:
- SQLite is used in production by Apple, Android, Chrome, Firefox
- Designed to survive power failures and crashes
- WAL mode provides atomic commits
- Result: Corruption indicates **WE'RE DOING SOMETHING WRONG**

**Likely Culprits** (need investigation):
1. **Schema Migration Issues**
   - Building new version changes database schema
   - Old Julie MCP process holds connection to old schema
   - → Database version mismatch = corruption

2. **Multiple Writers Without WAL**
   - If not using WAL mode, concurrent writes cause corruption
   - Check: Are we calling `PRAGMA journal_mode=WAL` on every connection?

3. **Improper Shutdown**
   - Not calling connection.close() cleanly
   - Not checkpointing WAL before exit
   - Windows file locking issues

4. **Connection Pool Misuse**
   - Holding connections across build boundaries
   - Not respecting SQLite's threading constraints

**Action Items** (Critical):
- [ ] Verify WAL mode is enabled on ALL connections (not just some)
- [ ] Add database version migration system
- [ ] Implement clean shutdown handler (checkpoint WAL, close connections)
- [ ] Add connection validation on startup (detect schema mismatches)
- [ ] Consider: Delete .julie/indexes/* on build (fresh start each session)

**This should be Priority 0** - search quality doesn't matter if the database is constantly corrupting.
