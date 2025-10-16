# Julie Development TODO

## 🔥 Recently Completed (2025-10-15)

### ✅ Issue: 8GB Memory Leak from ONNX Embedding Engine
**Problem:** Memory grew to 8GB during embedding generation and was never released
**Root Cause:** `EmbeddingEngine` (containing ONNX `TextEmbedding` model) stored in handler forever
**Solution:** Implemented lazy cleanup with 5-minute inactivity timeout
- Engine stays alive during active development (fast incremental updates, no 2-3s reinit overhead)
- Drops automatically after 5 minutes of inactivity (releases ~8GB)
- Periodic background task checks every 60s
- Auto-reinitializes on next use when needed

**Files Modified:**
- `src/handler.rs` - Added timestamp tracking and cleanup task
- `src/tools/workspace/indexing.rs` - Integrated timestamp updates
- `src/main.rs` - Started cleanup task at server init

**Status:** ✅ Compiled successfully, ready for testing

### ✅ Issue: Multi-word FTS5 Query Failure
**Problem:** `"refresh workspace embedding"` returned no results (implicit AND required ALL words)
**Solution:** Changed from implicit AND to explicit OR in `sanitize_fts5_query()`
**Status:** ✅ Working - now returns 15 results

---

## 🚧 Known Search Issues

### Issue 1: Underscore tokenization ❌ NOT FIXED
**Problem:**
- Query: `generate_embeddings_async` → No results
- Actual function: `generate_embeddings_from_sqlite`
- Root cause: FTS5 porter tokenizer treats `generate_embeddings_async` as **ONE token**
- No token splitting on underscores!

**Why this happens:**
FTS5's `porter unicode61` tokenizer doesn't split on underscores. Tokens are:
- `generate_embeddings_async` = 1 token (no match)
- `generate_embeddings_from_sqlite` = 1 token (different!)

**Workaround:** Use wildcards: `generate_embeddings*` → 15 results ✅

### Issue 2: Scope resolution operator ❌ NOT FIXED
**Problem:**
- Query: `WorkspaceOperation::Refresh` → No results
- `::` treated as special character, might be getting filtered

---

## 💡 Proposed Solutions for Search Issues

### Option A: Enhance FTS5 Tokenization (Medium Complexity)
**Problem:** Can't customize FTS5 tokenizer beyond built-in options
**Possible Fix:**
- Change from `porter unicode61` to `unicode61 remove_diacritics 2` (no stemming)
- Add custom tokenizer via SQLite extension (C code, complex)
- OR: Pre-process symbol names before indexing (split on `_`, `::`)

### Option B: Query Enhancement (Low Complexity) ⭐ RECOMMENDED
**Automatic wildcard injection:**
```rust
// If query contains underscore, add wildcard variant
"generate_embeddings_async" → "generate_embeddings_async OR generate_embeddings*"

// If query contains ::, split and OR
"WorkspaceOperation::Refresh" → "WorkspaceOperation OR Refresh"
```

### Option C: Symbol Name Normalization at Index Time (Medium Complexity)
**Pre-split symbols during indexing:**
```rust
// Store additional searchable forms
Symbol: "generate_embeddings_from_sqlite"
Index as:
  - "generate_embeddings_from_sqlite" (exact)
  - "generate embeddings from sqlite" (spaces for FTS5)
  - Individual words: "generate", "embeddings", "from", "sqlite"
```

### Option D: Learn from coa-codesearch-mcp (Lucene/Tantivy Migration)
Your C# coa-codesearch-mcp project has significantly more sophisticated code search:
- **Multi-Field Indexing**: 3 fields (content, content_symbols, content_patterns)
- **Smart Query Routing**: Routes queries to optimal field
- **Code-Aware Tokenizer**: Preserves code patterns like `std::cout`, `->method`
- **CamelCase Splitting**: `UserService` → `["UserService", "User", "Service"]`
- **Underscore Splitting**: `user_service` → splits on underscores! ✅

**Consider:** Port to Tantivy (Lucene-like Rust library) with proven multi-field strategy

---

## 📋 Test Organization Issues

We need to reorganize the codebase. Tests are scattered:
- `src/tests/` - Main test infrastructure (GOOD)
- Individual extractor files with inline tests (BAD - creates clutter)
- `debug/` directory with real-world test files (NEEDS INTEGRATION)
- `tests/editing/` - SOURCE/CONTROL methodology files (GOOD)
- Various `.backup` files (CLEANUP NEEDED)

**Target structure documented in CLAUDE.md**

---

## ✅ Action Items

### TOMORROW (2025-10-16)
- [ ] **Test lazy drop memory cleanup** - Verify engine drops after 5min idle, memory is released
- [ ] **Test incremental updates** - Verify fast updates during active development (no reinit overhead)

### THIS WEEK
- [ ] **Implement Option B** - Automatic wildcard injection for `_` and `::`
- [ ] **Test organization cleanup** - Consolidate tests per CLAUDE.md structure
- [ ] **Clean up .backup files** - Remove temporary artifacts

### NEXT SPRINT
- [ ] **Evaluate search improvements** - Option C (symbol normalization) vs Option D (Tantivy migration)
- [ ] **Real-world validation** - Test Julie on large codebases (10k+ files)

---

**Last Updated:** 2025-10-15
