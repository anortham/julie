# Julie Search Flow Documentation

**Purpose**: Living document tracking how searches flow through Julie's CASCADE architecture
**Last Updated**: 2025-10-12
**Status**: ✅ CASCADE Architecture - 2-Tier (Simplified)

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
                            ↓ (background)
                       Embeddings
                      (rebuilt from
                        SQLite)
```

**Two-Tier Progressive Enhancement**:
1. **Tier 1: SQLite FTS5** (Immediate, <5ms) - Full-text search with BM25 ranking
2. **Tier 2: HNSW Semantic** (20-30s background) - AI-powered semantic search

**Startup Flow**:
- **Startup completes in <2 seconds** (SQLite only)
- HNSW embeddings build in background (20-30s)
- **Search works immediately** using best available tier

**Why 2-Tier Architecture**:
- SQLite FTS5 provides <5ms queries with BM25 ranking (sufficient for most searches)
- Simplified from 3-tier to 2-tier by removing Tantivy (not due to Tantivy issues, but our complex async architecture)
- Cleaner design: SQLite (truth) → HNSW (semantic) - no intermediate layer
- Reduced architectural complexity and surface area for potential issues

---

## 🎯 Search Flow Architecture

### High-Level Flow

```
User Query → MCP Tool → Workspace Filter → Search Mode Router → Tier Selection → Results
```

### Mode-Based Routing

**1. Text Search Mode** (`mode: "text"`):
- Uses SQLite FTS5 (always available)
- Best for: exact matches, code patterns, symbol names
- Latency: <5ms

**2. Semantic Search Mode** (`mode: "semantic"`):
- Tries HNSW semantic (if ready)
- Falls back to SQLite FTS5
- Best for: conceptual queries, "find code that does X"
- Latency: <50ms (semantic) or <5ms (fallback)

**3. Hybrid Search Mode** (`mode: "hybrid"`):
- Combines FTS + semantic results
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
- `workspace`: "primary" (default) or specific workspace ID
- `limit`: Max results (default 50)

---

### Layer 2: Indexing Status Tracking
**File**: `src/handler.rs`
**Struct**: `IndexingStatus`

```rust
pub struct IndexingStatus {
    pub sqlite_fts_ready: AtomicBool,    // Always true after indexing
    pub semantic_ready: AtomicBool,      // True after 20-30s
}
```

**Status Flags Control Fallback Chain**:
- If `semantic_ready = false` → Use SQLite FTS5
- Graceful degradation: HNSW → SQLite FTS5

---

### Layer 3: Search Implementation

#### A. Text Search Flow
**File**: `src/tools/search.rs`
**Function**: `text_search()`

```rust
async fn text_search(&self, handler: &JulieServerHandler) -> Result<Vec<Symbol>> {
    // Always use SQLite FTS5 for text search
    self.sqlite_fts_search(handler).await
}
```

**Single Tier**: SQLite FTS5 (always available)

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

    // Fall back to SQLite FTS5
    self.sqlite_fts_search(handler).await
}
```

**Tier Priority**: HNSW Semantic → SQLite FTS5

---

#### C. SQLite FTS5 Search
**File**: `src/database/mod.rs`
**Function**: `search_file_content_fts()`

**Features**:
- FTS5 full-text search with BM25 ranking
- Searches both file content and symbol code_context
- Sub-5ms query latency
- Always available (no background indexing needed)
- Supports multi-word AND/OR queries

**SQL Query**:
```sql
SELECT path, snippet(files_fts, -1, '<b>', '</b>', '...', 64) as snippet,
       rank
FROM files_fts
WHERE files_fts MATCH ?
ORDER BY rank
LIMIT ?
```

**Why SQLite FTS5 is Sufficient**:
- BM25 ranking provides relevance scoring
- Porter stemming handles variations (authentication → authenticate)
- Multi-word queries with AND/OR logic
- Sub-5ms latency competitive with specialized search engines
- No complex index management needed

---

### Layer 4: Background Index Building

#### HNSW Semantic Background Build
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

### SQLite FTS5 Query Syntax

**Exact Phrase**:
```sql
"exact phrase"      -- Must appear exactly
```

**Boolean Operators**:
```sql
authentication AND user     -- Both terms required
authentication OR login     -- Either term
authentication NOT test     -- Exclude "test"
```

**Prefix Matching**:
```sql
auth*                       -- Matches authentication, authorize, etc.
```

**Proximity Search**:
```sql
NEAR(authentication user, 5) -- Terms within 5 words of each other
```

---

## 📊 Performance Characteristics

### Startup Performance
- **SQLite indexing**: <2 seconds (blocking)
- **HNSW build**: 20-30s (background, non-blocking)
- **Total to full capability**: ~30s
- **Search available**: Immediately (SQLite FTS5)

### Search Latency
- **SQLite FTS5**: <5ms (full-text search with BM25)
- **HNSW Semantic**: <50ms (vector similarity)
- **Fallback overhead**: <1ms (negligible)

### Storage
- **SQLite database**: ~1-2KB per symbol
- **HNSW embeddings**: ~1-2KB per symbol
- **Embedding models cache**: ~128MB (one-time download)
- **Total savings**: ~5-10KB per symbol (simplified architecture)

---

## ✅ Success Metrics

### Reliability
- ✅ Single source of truth (SQLite)
- ✅ HNSW rebuildable from SQLite (<30s)
- ✅ Search always available (graceful degradation)
- ✅ No deadlocks (simplified architecture with fewer layers)

### Performance
- ✅ Startup time: <2 seconds (30-60x improvement)
- ✅ SQLite FTS: <5ms query latency
- ✅ HNSW background: <30s for 10k symbols
- ✅ No blocking during startup

### Simplicity
- ✅ 2-tier architecture (vs 3-tier)
- ✅ Simpler index management (2-tier vs 3-tier)
- ✅ No search engine locks/deadlocks
- ✅ Smaller disk footprint

### User Experience
- ✅ Immediate search (SQLite FTS5)
- ✅ Progressive enhancement (FTS → Semantic)
- ✅ Status indicators show capability
- ✅ Graceful degradation on failures

---

## 🔧 Debugging & Monitoring

### Check Indexing Status
```bash
# Watch indexing progress in logs
tail -f .julie/logs/julie.log

# Check SQLite database
sqlite3 .julie/indexes/{workspace_id}/db/symbols.db "SELECT COUNT(*) FROM symbols;"
sqlite3 .julie/indexes/{workspace_id}/db/symbols.db "SELECT COUNT(*) FROM files_fts;"

# Check HNSW embeddings
ls -lh .julie/indexes/{workspace_id}/vectors/
```

### Test Search Tiers
```bash
# Test SQLite FTS (always available)
# Query: "authentication"
# Expected: Results immediately, even during startup

# Test Semantic (after 20-30s)
# Query: "code that handles authentication"
# Expected: Conceptually similar functions, even with different names
```

---

## 🚀 Future Enhancements

### Potential Improvements
- [ ] **Query caching**: Cache frequent queries for <1ms response
- [ ] **Incremental updates**: Update indexes without full rebuild
- [ ] **Ranking tuning**: Boost relevance for code patterns
- [ ] **Multi-modal search**: Combine code + docs + tests in results
- [ ] **Query suggestions**: "Did you mean..." for typos
- [ ] **Search analytics**: Track popular queries and performance

---

**This document reflects the production 2-tier CASCADE architecture (simplified 2025-10-12).**
