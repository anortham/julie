# Julie Search Flow Documentation

**Purpose**: Living document tracking how searches flow through Julie's architecture
**Last Updated**: 2025-09-30
**Status**: ✅ FILE_CONTENT search working (ranking issue identified)

---

## 🎯 High-Level Search Flow

```
User Query → MCP Tool → Workspace Filter → Search Engine → Query Type Detection → Results
```

---

## 📊 Current State (2025-09-30)

### ✅ What Works
- Database storage: 21,525 total symbols (30 FILE_CONTENT)
- Tantivy indexing: 5,597 symbols indexed (30 FILE_CONTENT confirmed)
- Code symbol search: Returns results for code (functions, classes, etc.)
- FILE_CONTENT search: All 4 fixes applied, searchability confirmed
- Unit test: `test_file_content_search()` passes ✅
- Real-world tests:
  - "CLAUDE" → finds FILE_CONTENT_CLAUDE.md ✅
  - "dogfooding" → finds FILE_CONTENT at position #4 ✅
  - "architecture" → finds FILE_CONTENT at position #2 ✅

### ⚠️ Known Limitations
- **Ranking Issue**: FILE_CONTENT symbols rank lower than code symbols
  - When many code matches exist, FILE_CONTENT may not appear in top results
  - "Miller" search (100 results) → FILE_CONTENT not in top 75
  - This is expected behavior for a code-focused search tool
  - Future enhancement: Boost FILE_CONTENT for documentation-focused queries

### 🔍 Fixes Applied
1. **Tokenizer Fix** (src/search/schema.rs:112): Changed code_context field from code_aware to standard tokenizer
2. **Database Storage Fix** (src/tools/workspace/indexing.rs:1014): FILE_CONTENT symbols now cloned to database
3. **Tantivy Query Fix** (src/search/engine/queries.rs:111): Added code_context to exact_symbol_search fields
4. **SQL Query Fix** (src/database/mod.rs): Modified 4 queries to search code_context field: `WHERE (name LIKE ?1 OR code_context LIKE ?1)`

---

## 🏗️ Architecture Layers

### Layer 1: MCP Tool Entry Point
**File**: `src/tools/search.rs`
**Function**: `FastSearchTool::call_tool()`

```rust
// Line 78-154
pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult>
```

**Flow**:
1. Check system readiness (`SystemReadiness` enum)
2. Route based on mode:
   - `"text"` → `text_search()`
   - `"semantic"` → `semantic_search()`
   - `"hybrid"` → `hybrid_search()`
3. Format and return results

**Parameters**:
- `query`: Search string
- `mode`: "text", "semantic", or "hybrid"
- `language`: Optional language filter
- `file_pattern`: Optional glob filter
- `workspace`: "primary", "all", or workspace ID
- `limit`: Max results (default 50)

---

### Layer 2: Workspace Resolution
**File**: `src/tools/search.rs`
**Function**: `resolve_workspace_filter()`

```rust
// Determines which workspace(s) to search
async fn resolve_workspace_filter(&self, handler: &JulieServerHandler)
    -> Result<Option<Vec<String>>>
```

**Logic**:
- `"primary"` → Single primary workspace ID
- `"all"` → None (searches all workspaces)
- Specific ID → That workspace only

**Critical Decision Point**:
- If workspace filter specified → Use **database search** with filter
- If "all" workspaces → Use **Tantivy search** engine

---

### Layer 3A: Text Search (Tantivy Path)
**File**: `src/tools/search.rs`
**Function**: `text_search()`

```rust
// Line 156-200+
async fn text_search(&self, handler: &JulieServerHandler) -> Result<Vec<Symbol>>
```

**Flow**:
1. Check workspace filter
2. If workspace filter exists → Use `database_search_with_workspace_filter()`
3. Otherwise → Use Tantivy:
   - Get persistent search engine from workspace
   - Call `search_engine.search(&self.query)`
   - Convert Tantivy results to symbols

**This is likely where FILE_CONTENT gets lost!**

---

### Layer 3B: Database Search (SQLite Path)
**File**: `src/tools/search.rs`
**Function**: `database_search_with_workspace_filter()`

```rust
async fn database_search_with_workspace_filter(
    &self,
    handler: &JulieServerHandler,
    workspace_ids: Vec<String>,
) -> Result<Vec<Symbol>>
```

**Flow**:
1. Get database from handler
2. Query symbols by name pattern using `LIKE %query%`
3. Filter by workspace IDs
4. Apply language/file pattern filters
5. Return matching symbols

**Question**: Does this search `code_context` field?

---

### Layer 4: Tantivy Search Engine
**File**: `src/search/engine/queries.rs`
**Function**: `SearchEngine::search()`

```rust
// Line 15-68
pub async fn search(&self, query: &str) -> Result<Vec<SearchResult>>
```

**Flow**:
1. Detect intent: `query_processor.detect_intent(query)`
2. Transform query based on intent
3. Route to specialized search:
   - `ExactSymbol` → `exact_symbol_search()`
   - `GenericType` → `generic_type_search()`
   - `OperatorSearch` → `operator_search()`
   - `FilePath` → `file_path_search()`
   - `SemanticConcept` → `semantic_search()`
   - `Mixed` → `mixed_search()`
   - Default → `semantic_search()`

**Intent Detection**: Uses regex patterns
- `"CLAUDE"` (all-caps) → `ExactSymbol` intent
- `"Project Julie"` (multi-word) → `SemanticConcept` intent

---

### Layer 5: Specialized Search Methods
**File**: `src/search/engine/queries.rs`

#### A. exact_symbol_search()
**Line**: ~71-145
**Fields Searched**: `symbol_name` only
**Problem**: Doesn't search `code_context` field!

#### B. semantic_search()
**Line**: 271-370+
**Fields Searched**:
- `all_text` (code_aware tokenizer)
- `symbol_name`
- `signature`
- `doc_comment`
- `code_context` ✅ (added in our fix, standard tokenizer)

**QueryParser fields** (Line 275-283):
```rust
vec![
    fields.all_text,        // code_aware tokenizer
    fields.symbol_name,     // code_aware tokenizer
    fields.signature,       // code_aware tokenizer
    fields.doc_comment,     // standard tokenizer
    fields.code_context,    // standard tokenizer (OUR FIX)
]
```

#### C. generic_type_search()
**Line**: ~147-211
**Fields Searched**: signature, signature_exact, symbol_name, all_text, code_context

#### D. operator_search()
**Line**: ~213-250
**Fields Searched**: signature, signature_exact, all_text, code_context

---

## 🔧 Tokenizer Configuration

### Schema Definition
**File**: `src/search/schema.rs`

**Line 111-112** (OUR FIX):
```rust
// Use standard tokenizer for code_context to support FILE_CONTENT (markdown/text files)
let code_context = schema_builder.add_text_field("code_context", text_options.clone());
```

**Line 161** (POTENTIAL PROBLEM):
```rust
let all_text = schema_builder.add_text_field("all_text", code_index_only);
// code_index_only uses code_aware tokenizer!
```

### Tokenizers Used
1. **code_aware** (`CodeTokenizer`):
   - Splits on: alphanumeric + underscore only
   - Breaks on: `.`, `-`, `#`, `,`, `!`, spaces, etc.
   - CamelCase splitting: `getUserData` → ["get", "user", "data"]
   - Used for: `all_text`, `symbol_name`, `signature`, etc.

2. **standard** (Tantivy default):
   - Preserves words with punctuation
   - Better for natural language (markdown, prose)
   - Used for: `code_context`, `doc_comment`

---

## 🐛 Root Cause Analysis

### Hypothesis 1: Intent Routing Issue
- "CLAUDE" → ExactSymbol → Only searches `symbol_name` field
- FILE_CONTENT symbols have name like "FILE_CONTENT_CLAUDE.md"
- Doesn't match query "CLAUDE"
- **Solution**: Add code_context to exact_symbol_search()

### Hypothesis 2: all_text Field Problem
- `all_text` uses code_aware tokenizer
- `code_context` content gets re-tokenized when added to `all_text`
- Even though `code_context` field uses standard tokenizer, `all_text` uses code_aware
- **Solution**: Either:
  a. Don't include FILE_CONTENT in all_text
  b. Query code_context directly (already done in semantic_search)

### Hypothesis 3: Symbol Filtering
- FILE_CONTENT symbols have `kind: module`, `language: text`
- Some search logic might filter out these symbols
- **Check**: Do any filters exclude `language: text`?

---

## 🎯 Next Steps

### Immediate Debugging
1. [ ] Add debug logging to see which symbols are returned from Tantivy
2. [ ] Check if FILE_CONTENT symbols are in Tantivy results but filtered out
3. [ ] Test exact_symbol_search() specifically for FILE_CONTENT
4. [ ] Check language/kind filters in search results processing

### Code Fixes
1. [ ] Add code_context to exact_symbol_search() query fields
2. [ ] Add code_context to file_path_search() if relevant
3. [ ] Consider separate handling for FILE_CONTENT vs code symbols
4. [ ] Add integration test that uses MCP tool (not just SearchEngine)

### Testing
1. [x] Unit test with in-memory engine: `test_file_content_search()` ✅
2. [ ] Integration test with MCP tool
3. [ ] Real-world test with actual markdown files
4. [ ] Performance test with large text files

---

## 📝 Known Issues

### Issue #1: "CLAUDE" Search Hangs
- **Trigger**: All-caps query
- **Intent**: ExactSymbol
- **Problem**: Unknown - needs investigation
- **Workaround**: Use lowercase or multi-word queries

### Issue #2: Database/Tantivy Mismatch
- **Database**: 29 FILE_CONTENT symbols
- **Logs**: 262 FILE_CONTENT symbols indexed
- **Problem**: Unclear why counts differ
- **Status**: Database storage fix applied (clones FILE_CONTENT to both vecs)

---

## 🔍 Debugging Commands

```bash
# Count FILE_CONTENT in database
sqlite3 .julie/db/symbols.db "SELECT COUNT(*) FROM symbols WHERE name LIKE 'FILE_CONTENT_%';"

# Check FILE_CONTENT properties
sqlite3 .julie/db/symbols.db "SELECT name, kind, language, LENGTH(code_context) FROM symbols WHERE name LIKE 'FILE_CONTENT_%' LIMIT 5;"

# Check Tantivy index
ls -lh .julie/index/tantivy/

# Check logs for indexing
grep "FILE_CONTENT" .julie/logs/julie.log.2025-09-30 | tail -20

# Test search
cargo test test_file_content_search -- --nocapture
```

---

**This document should be updated whenever:**
- Search flow changes
- New search modes added
- Bugs discovered
- Fixes applied
- Performance optimizations made