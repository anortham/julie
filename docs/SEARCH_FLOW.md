# Julie Search Flow Documentation

**Purpose**: Living document tracking how searches flow through Julie's CASCADE architecture
**Last Updated**: 2025-09-30
**Status**: ✅ CASCADE Architecture Complete

---

## 🌊 CASCADE Architecture Overview

**Core Principle**: SQLite is the single source of truth. All indexes cascade downstream.

```
Files → Tree-sitter → Symbols + Content
                            ↓
                        SQLite
                  (single source of truth)
                  ├─ files.content (file text)
                  ├─ files_fts (FTS5 index)
                  └─ symbols (extracted symbols)
                            ↓
                  ┌─────────┴─────────┐
                  ↓ (background)      ↓ (background)
               Tantivy              Embeddings
            (rebuilt from          (rebuilt from
             SQLite)                SQLite)
```

**Three-Tier Progressive Enhancement**:
1. **Tier 1: SQLite FTS5** (Immediate, <5ms) - Basic full-text search
2. **Tier 2: Tantivy** (5-10s background) - Advanced code-aware search
3. **Tier 3: HNSW Semantic** (20-30s background) - AI-powered semantic search

**Startup Flow**:
- **Startup completes in <2 seconds** (SQLite only)
- Tantivy builds in background (5-10s)
- HNSW embeddings build in background (20-30s)
- **Search works immediately** using best available tier

---

## 🎯 Search Flow Architecture

### High-Level Flow

```
User Query → MCP Tool → Workspace Filter → Search Mode Router → Tier Selection → Results
```

### Mode-Based Routing

**1. Text Search Mode** (`mode: "text"`):
- Tries Tantivy first (if ready)
- Falls back to SQLite FTS5
- Best for: exact matches, code patterns, symbol names

**2. Semantic Search Mode** (`mode: "semantic"`):
- Tries HNSW semantic (if ready)
- Falls back to Tantivy (if ready)
- Falls back to SQLite FTS5
- Best for: conceptual queries, "find code that does X"

**3. Hybrid Search Mode** (`mode: "hybrid"`):
- Combines Tantivy + semantic results
- Merges and re-ranks by relevance
- Best for: comprehensive searches

---

## 🏗️ Implementation Layers

### Layer 1: MCP Tool Entry Point
**File**: `src/tools/search.rs`
**Function**: `FastSearchTool::call_tool()`

**Responsibilities**:
1. Check system readiness (`IndexingStatus` flags)
2. Route based on mode: text/semantic/hybrid
3. Apply workspace filters
4. Format and return results

**Key Parameters**:
- `query`: Search string
- `mode`: "text", "semantic", or "hybrid"
- `language`: Optional language filter
- `file_pattern`: Optional glob filter
- `workspace`: "primary", "all", or workspace ID
- `limit`: Max results (default 50)

---

### Layer 2: Indexing Status Tracking
**File**: `src/handler.rs`
**Struct**: `IndexingStatus`

```rust
pub struct IndexingStatus {
    pub sqlite_fts_ready: AtomicBool,    // Always true after indexing
    pub tantivy_ready: AtomicBool,       // True after 5-10s
    pub semantic_ready: AtomicBool,      // True after 20-30s
}
```

**Status Flags Control Fallback Chain**:
- If `tantivy_ready = false` → Use SQLite FTS5
- If `semantic_ready = false` → Use Tantivy or FTS5
- Graceful degradation through tiers

---

### Layer 3: Search Implementation

#### A. Text Search Flow
**File**: `src/tools/search.rs`
**Function**: `text_search()`

```rust
async fn text_search(&self, handler: &JulieServerHandler) -> Result<Vec<Symbol>> {
    let status = handler.indexing_status();

    // Try Tantivy first (if ready)
    if status.tantivy_ready.load(Ordering::Relaxed) {
        if let Ok(results) = self.try_tantivy_search(handler).await {
            if !results.is_empty() {
                return Ok(results);
            }
        }
    }

    // Fall back to SQLite FTS5
    self.sqlite_fts_search(handler).await
}
```

**Tier Priority**: Tantivy → SQLite FTS5

---

#### B. Semantic Search Flow
**File**: `src/tools/search.rs`
**Function**: `semantic_search()`

```rust
async fn semantic_search(&self, handler: &JulieServerHandler) -> Result<Vec<Symbol>> {
    let status = handler.indexing_status();

    // Try HNSW semantic (if ready)
    if status.semantic_ready.load(Ordering::Relaxed) {
        if let Ok(results) = self.try_hnsw_search(handler).await {
            return Ok(results);
        }
    }

    // Fall back to Tantivy (if ready)
    if status.tantivy_ready.load(Ordering::Relaxed) {
        if let Ok(results) = self.try_tantivy_search(handler).await {
            return Ok(results);
        }
    }

    // Fall back to SQLite FTS5
    self.sqlite_fts_search(handler).await
}
```

**Tier Priority**: HNSW Semantic → Tantivy → SQLite FTS5

---

#### C. SQLite FTS5 Search
**File**: `src/database/mod.rs`
**Function**: `search_file_content_fts()`

**Features**:
- FTS5 full-text search with BM25 ranking
- Searches both file content and symbol code_context
- Sub-5ms query latency
- Always available (no background indexing needed)

**SQL Query**:
```sql
SELECT path, snippet(files_fts, -1, '<b>', '</b>', '...', 64) as snippet,
       rank
FROM files_fts
WHERE files_fts MATCH ?
ORDER BY rank
LIMIT ?
```

---

### Layer 4: Background Index Building

#### A. Tantivy Background Build
**File**: `src/tools/workspace/indexing.rs`
**Function**: `build_tantivy_from_sqlite()`

**Process**:
1. Pull all symbols from SQLite
2. Pull all file contents from SQLite
3. Create FILE_CONTENT symbols from file contents
4. Index into Tantivy (code-aware tokenization)
5. Set `tantivy_ready = true`

**Timing**: 5-10s for 10k symbols

---

#### B. HNSW Semantic Background Build
**File**: `src/tools/workspace/indexing.rs`
**Function**: `generate_embeddings_from_sqlite()`

**Process**:
1. Pull all symbols from SQLite
2. Generate embeddings using ONNX model (Xenova/bge-small-en-v1.5)
3. Build HNSW index for fast vector similarity
4. Set `semantic_ready = true`

**Timing**: 20-30s for 10k symbols

**Cache Location**: `<workspace>/.julie/cache/embeddings/`

---

## 🔍 Query Processing

### Tantivy Query Intent Detection
**File**: `src/search/engine/queries.rs`
**Function**: `QueryProcessor::detect_intent()`

**Intent Types**:
- `ExactSymbol`: All-caps queries, quoted strings
- `GenericType`: Generic syntax like `Vec<T>`
- `OperatorSearch`: Operators like `+`, `*`, `==`
- `FilePath`: Paths like `src/main.rs`
- `SemanticConcept`: Natural language queries
- `Mixed`: Combination of above

**Example Intent Routing**:
- `"getUserData"` → ExactSymbol → Fast symbol_name search
- `"authentication logic"` → SemanticConcept → Semantic search
- `"Vec<String>"` → GenericType → Generic type search

---

### Tokenization Strategy

**1. Code-Aware Tokenizer** (`CodeTokenizer`):
- Splits on: alphanumeric + underscore only
- CamelCase splitting: `getUserData` → ["get", "user", "data"]
- Used for: `symbol_name`, `signature`, `all_text`

**2. Standard Tokenizer** (Tantivy default):
- Preserves words with punctuation
- Better for natural language
- Used for: `code_context`, `doc_comment`

**Fields in Tantivy Schema**:
```rust
all_text         -> code_aware tokenizer
symbol_name      -> code_aware tokenizer
signature        -> code_aware tokenizer
doc_comment      -> standard tokenizer
code_context     -> standard tokenizer  // For FILE_CONTENT
```

---

## 📊 Performance Characteristics

### Startup Performance
- **SQLite indexing**: <2 seconds (blocking)
- **Tantivy build**: 5-10s (background, non-blocking)
- **HNSW build**: 20-30s (background, non-blocking)
- **Total to full capability**: ~30-40s
- **Search available**: Immediately (SQLite FTS5)

### Search Latency
- **SQLite FTS5**: <5ms (basic text search)
- **Tantivy**: <10ms (code-aware search)
- **HNSW Semantic**: <50ms (vector similarity)
- **Fallback overhead**: <1ms (negligible)

### Storage
- **SQLite database**: ~1-2KB per symbol
- **Tantivy index**: ~5-10KB per symbol
- **HNSW embeddings**: ~1-2KB per symbol
- **Embedding models cache**: ~128MB (one-time download)

---

## ✅ Success Metrics

### Reliability
- ✅ Single source of truth (SQLite)
- ✅ Tantivy rebuildable from SQLite (<10s)
- ✅ HNSW rebuildable from SQLite (<30s)
- ✅ Search always available (graceful degradation)

### Performance
- ✅ Startup time: <2 seconds (30-60x improvement)
- ✅ SQLite FTS: <5ms query latency
- ✅ Tantivy background: <10s for 10k symbols
- ✅ No blocking during startup

### User Experience
- ✅ Immediate search (SQLite FTS5)
- ✅ Progressive enhancement (FTS → Tantivy → Semantic)
- ✅ Status indicators show capability
- ✅ Graceful degradation on failures

---

## 🔧 Debugging & Monitoring

### Check Indexing Status
```bash
# Watch indexing progress in logs
tail -f .julie/logs/julie.log

# Check SQLite database
sqlite3 .julie/db/symbols.db "SELECT COUNT(*) FROM symbols;"
sqlite3 .julie/db/symbols.db "SELECT COUNT(*) FROM files_fts;"

# Check Tantivy index
ls -lh .julie/index/tantivy/

# Check HNSW embeddings
ls -lh .julie/vectors/
```

### Test Search Tiers
```bash
# Test SQLite FTS (always available)
# Query: "authentication"
# Expected: Results immediately, even during startup

# Test Tantivy (after 5-10s)
# Query: "getUserData"
# Expected: Code-aware tokenization, camelCase splitting

# Test Semantic (after 20-30s)
# Query: "code that handles authentication"
# Expected: Conceptually similar functions, even with different names
```

---

## 🚀 Future Enhancements

### Potential Improvements
- [ ] **Query caching**: Cache frequent queries for <1ms response
- [ ] **Incremental updates**: Update indexes without full rebuild
- [ ] **Ranking tuning**: Boost FILE_CONTENT for documentation queries
- [ ] **Multi-modal search**: Combine code + docs + tests in results
- [ ] **Query suggestions**: "Did you mean..." for typos
- [ ] **Search analytics**: Track popular queries and performance

---

**This document reflects the production CASCADE architecture (2025-09-30).**
