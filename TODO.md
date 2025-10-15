# Julie Search Issues & Solutions

## 🔍 Root Cause Analysis (2025-10-15)

### Issue 1: Multi-word query failure ✅ FIXED
**Problem:** `"refresh workspace embedding"` returned no results (implicit AND required ALL words)
**Fix Applied:** Changed from implicit AND to explicit OR in `sanitize_fts5_query()`
**Status:** ✅ Working - now returns 15 results

### Issue 2: Underscore tokenization ❌ NOT FIXED
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

### Issue 3: Scope resolution operator ❌ NOT FIXED
**Problem:**
- Query: `WorkspaceOperation::Refresh` → No results
- `::` treated as special character, might be getting filtered

---

## 🚀 Lessons from coa-codesearch-mcp (Lucene-based)

Your C# coa-codesearch-mcp project has **significantly more sophisticated** code search:

### 1. **Multi-Field Indexing Strategy**
Three specialized fields with different analyzers:
- `content` → Standard code search with CamelCase splitting
- `content_symbols` → Symbol-only (identifiers, class names)
- `content_patterns` → Pattern-preserving (special chars intact)

### 2. **Smart Query Routing** (`SmartQueryPreprocessor`)
Routes queries to optimal field based on content:
```csharp
// Detects special chars (::, ->, []) → content_patterns field
// Detects symbols (CamelCase, identifiers) → content_symbols field
// Standard text → content field

// Removes noise words: "class UserService" → "UserService"
```

### 3. **Code-Aware Tokenizer** (`CodeTokenizer`)
Recognizes and preserves code patterns as **single tokens**:
- `std::cout` → 1 token (not 3!)
- `->method` → 1 token
- `[Fact]` → 1 token
- `: IRepository` → 1 token
- `List<T>` → 1 token with generic handling

### 4. **CamelCase Splitting** (`CamelCaseFilter`)
Smart splitting for better searchability:
- `UserService` → `["UserService", "User", "Service"]` (3 tokens!)
- `OAuth2Provider` → `["OAuth2Provider", "OAuth", "2", "Provider"]`
- `user_service` → `["user_service", "user", "service"]` ← **Splits on underscores!**
- `McpToolBase<TParams, TResult>` → Extracts and splits all parts

---

## 💡 Proposed Solutions for Julie

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

### Option D: Lucene Migration (High Complexity, High Value)
**Full Lucene port from coa-codesearch-mcp:**
- Use `tantivy` (Lucene-like Rust library)
- Port CodeAnalyzer, CamelCaseFilter, SmartQueryPreprocessor
- Get sophisticated code search that already works!

---

## 🎯 Recommendation

**Short-term (TODAY):** Option B - Query enhancement with automatic wildcards
- Low complexity, immediate improvement
- Handles 80% of cases where users type underscored names

**Medium-term (NEXT SPRINT):** Option C - Symbol name normalization
- Better UX than requiring wildcards
- Works with existing FTS5 infrastructure

**Long-term (BACKLOG):** Option D - Consider Tantivy migration
- Your coa-codesearch-mcp code is battle-tested and sophisticated
- Tantivy gives us Lucene-quality search in Rust
- Can reuse the proven multi-field strategy

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

1. [ ] **IMMEDIATE:** Commit the multi-word OR fix (already done)
2. [ ] **TODAY:** Implement Option B (automatic wildcard injection for `_` and `::`)
3. [ ] **THIS WEEK:** Test organization cleanup per CLAUDE.md
4. [ ] **NEXT:** Evaluate Option C (symbol normalization) vs Option D (Tantivy migration)
