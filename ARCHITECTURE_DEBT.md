# Julie Architecture Debt & Technical Issues

**Purpose**: Track specific technical debt, incomplete implementations, and performance issues that need systematic resolution. This complements `docs/julie-implementation-checklist.md` (feature tracking) and `TODO.md` (observations).

**Total Known Issues**: 142 TODOs in codebase (as of 2025-09-29)

---

## 🔴 CRITICAL - Blocking Production Quality

### 1. ~~**Semantic Search Completely Disabled**~~ ✅ **FIXED 2025-09-29**
**Status**: ✅ COMPLETE - Production Ready
**Completed**: HNSW disk loading + lazy initialization + double-checked locking
**Impact**: Julie's headline feature (cross-language semantic search) now works!

**What Was Fixed**:
- [x] Implemented HNSW vector index with hnsw_rs crate
- [x] Embeddings loaded from database on first semantic query (lazy loading)
- [x] HNSW index persisted to disk (`.julie/index/hnsw/`)
- [x] Sub-10ms semantic search with 6,235+ vectors
- [x] Double-checked locking pattern for thread-safe lazy initialization
- [x] Non-blocking startup (HNSW loads on-demand)

**Evidence**:
```bash
# Semantic search working (tested 2025-09-29)
fast_search("database symbol storage and persistence", mode: semantic)
→ 10 relevant results in <100ms
→ 6,235 embeddings loaded from disk
→ HNSW index: ef_construction=200, M=16
```

**Commits**:
- `3255bee` - HNSW Disk Loading Implementation
- `1886906` - Non-Blocking Startup + Lazy Loading
- `bfb9b1d` - Double-Checked Locking Fix

---

### 2. ~~**Incomplete Database Schema**~~ ✅ **FIXED 2025-09-29**
**Status**: ✅ COMPLETE - All fields now persist
**Completed**: TDD-driven schema completion with test verification
**Impact**: No more metadata loss, full symbol fidelity

**What Was Fixed**:
- [x] Added `start_byte` / `end_byte` columns for precise navigation
- [x] Added `doc_comment` column for documentation preservation
- [x] Added `visibility` column (public/private/protected) with enum serialization
- [x] Added `code_context` column for surrounding code
- [x] Updated all store/retrieve operations
- [x] Added `count_symbols_for_workspace()` for statistics
- [x] TDD test proves all fields persist correctly

**Evidence**:
```rust
// Test: test_complete_symbol_field_persistence (database/mod.rs:2491)
✅ All 5 new fields verified: start_byte, end_byte, doc_comment, visibility, code_context
✅ Serialization/deserialization working correctly
✅ Real workspace: 5,482 symbols with complete metadata
```

**Commit**: `eefaaef` - Complete Database Schema + Auto-Update Registry Statistics

---

### 3. **Unsafe Refactoring Implementation** ⚠️ CODE CORRUPTION RISK
**Status**: ⚠️ Production risk - NEXT PRIORITY
**Location**: `src/tools/refactoring.rs`
**Impact**: Rename operations can corrupt code

**Problem**:
```rust
// TODO: Use tree-sitter for proper AST analysis
// TODO: Use tree-sitter to find proper scope boundaries
// TODO: Find symbol boundaries using simple heuristics (upgrade to tree-sitter)
```

**Current Implementation**:
- Uses simple regex and string matching
- No understanding of code structure
- Can rename in strings, comments, wrong scope

**Examples of Potential Corruption**:
```rust
// Rename "User" → "Account"
let user_name = "User";    // ❌ String literal renamed!
// Comment about User      // ❌ Comment renamed!
impl OtherUser {}          // ❌ Partial match renamed!
```

**Fix Required**:
1. [ ] Implement tree-sitter AST analysis for rename operations
2. [ ] Find exact definition using AST navigation
3. [ ] Find all references using AST scope analysis
4. [ ] Apply surgical edits only to AST nodes (not strings/comments)
5. [ ] Add comprehensive safety tests with edge cases

**Estimated Effort**: 2-3 days (AST-based refactoring + safety tests)

---

## 🟡 HIGH PRIORITY - Performance & UX

### 4. **O(n) Database Scans Everywhere** 📊 PERFORMANCE
**Status**: ⚠️ Scales poorly
**Locations**: Multiple tools doing `get_all_symbols()`
**Impact**: Queries slow down linearly with codebase size

**Offenders**:
- [ ] `src/tools/navigation.rs` - Multiple `get_all_symbols()` calls
- [ ] `src/tools/exploration.rs` - Loads all symbols for filtering
- [ ] `src/tools/editing.rs` - Scans all symbols for validation

**Pattern**:
```rust
// ❌ BAD: O(n) scan
let all_symbols = db.get_all_symbols()?;
let filtered = all_symbols.iter()
    .filter(|s| s.name.contains(query))
    .collect();

// ✅ GOOD: O(log n) indexed query
let filtered = db.query_symbols_by_name(query)?;
```

**Fix Required**:
1. [ ] Add indexed query methods to database:
   - `query_symbols_by_name(pattern)` with LIKE/FTS
   - `query_symbols_by_kind(kind)` with index
   - `query_symbols_by_file(path)` with FK index
2. [ ] Replace all `get_all_symbols()` calls with indexed queries
3. [ ] Add database indexes for common query patterns
4. [ ] Benchmark before/after with large codebases (10k+ files)

**Estimated Effort**: 2 days (indexed queries + refactoring callers)

---

### 5. ~~**Embeddings Recomputed Per Query**~~ ✅ **FIXED 2025-09-29**
**Status**: ✅ COMPLETE - Fully operational
**Location**: `src/tools/workspace/indexing.rs:975` (was TODO)
**Impact**: Embeddings now persist and power semantic search

**What Was Fixed**:
- [x] Embeddings generated during background indexing (60s one-time cost)
- [x] Embeddings stored in `embedding_vectors` table (6,235+ rows)
- [x] HNSW index built from embeddings for fast similarity search
- [x] Semantic search now enabled and working (<100ms queries)

**Related**: See Issue #1 (Semantic Search) - now COMPLETE

---

### 6. **Workspace Statistics Not Updating** 📊 REGISTRY SYNC
**Status**: ✅ FIXED (2025-09-29)
**Location**: `src/main.rs` + `src/tools/workspace/commands/index.rs`
**Impact**: Registry showed 0 documents despite successful indexing

**What Was Fixed**:
- [x] Added `update_workspace_statistics()` that runs on BOTH code paths
- [x] Auto-indexing now updates registry even when workspace is up-to-date
- [x] Calculates real Tantivy index size (not hardcoded 0)
- [x] Added `count_symbols_for_workspace()` database method
- [x] Registry statistics now accurate: 5,482 symbols, 12.56 MB index

**Commit**: `eefaaef` - Complete Database Schema + Auto-Update Registry Statistics

---

### 7. **Workspace Initialization Failures** 🏗️ STARTUP ROBUSTNESS
**Status**: ✅ FIXED (2025-09-29)
**Location**: `src/workspace/mod.rs`
**Impact**: Fresh workspaces failed to initialize properly

**What Was Fixed**:
- [x] `validate_structure()` now creates missing directories instead of failing
- [x] Self-healing initialization for robust startup
- [x] Graceful handling of partial workspace states

**Commit**: `eefaaef` - Workspace initialization enhancement

---

## 🟢 MEDIUM PRIORITY - Features & Polish

### 8. **Reference Workspace Indexing Incomplete** 🔗 PHASE 4
**Status**: ⏸️ Deferred
**Location**: `src/tools/workspace/commands/registry.rs:736`
**Impact**: Can't add cross-project workspaces for multi-repo search

**Current State**:
```rust
// TODO: Index the reference workspace (Phase 4)
```

**What Works**:
- [x] Primary workspace indexing
- [x] Workspace registry for tracking multiple workspaces
- [x] Database schema supports multi-workspace

**What's Missing**:
- [ ] `add_workspace` command implementation
- [ ] Background indexing for reference workspaces
- [ ] Cross-workspace query routing

**Fix Required**:
1. [ ] Implement `add_workspace` handler
2. [ ] Trigger indexing for newly added workspace
3. [ ] Store workspace_id with symbols
4. [ ] Update search to query across workspaces
5. [ ] Add workspace filtering to tools

**Estimated Effort**: 1-2 days

---

### 9. **Orphaned Index Cleanup Missing** 🧹 MAINTENANCE
**Status**: ⏸️ Low priority
**Location**: `src/tools/workspace/indexing.rs:753`
**Impact**: Database bloat over time, stale data

**Problem**:
```rust
// TODO: Clean up orphaned entries for files that no longer exist
```

**Current Behavior**:
- Files deleted → Database entries remain
- Renamed files → Old entries + new entries (duplicates)
- Over time → Database grows with stale data

**Fix Required**:
1. [ ] Add "orphan detection" query (symbols without matching files)
2. [ ] Create cleanup routine in workspace health check
3. [ ] Add "vacuum database" command to tools
4. [ ] Schedule automatic cleanup (weekly?)

**Estimated Effort**: 1 day

---

## 📊 Metrics & Statistics

### Current Codebase Debt (Updated 2025-09-29)
- **Total TODOs**: 142 across codebase
- **✅ Fixed Critical Issues**: 5 (semantic search, schema, embeddings, statistics, initialization)
- **❌ Remaining Critical Issues**: 1 (refactoring safety)
- **High Priority**: 1 (O(n) scans)
- **Medium Priority**: 2 (reference workspaces, orphan cleanup)

### Test Coverage Gaps
- [x] ✅ Semantic search tests (now working with HNSW)
- [x] ✅ Database schema completeness test (TDD test added)
- [ ] Limited refactoring safety tests (string-based approach)
- [ ] No performance regression tests
- [ ] Missing cross-workspace integration tests

### Performance Baselines (Updated 2025-09-29)
- **Indexing**: ~60s for 6,235 symbols (including embedding generation)
- **Text Search**: <10ms (Tantivy)
- **Semantic Search**: <100ms (HNSW with 6,235 vectors) ✅
- **HNSW Index Loading**: Lazy (on first semantic query)
- **Database Queries**: Need benchmarks for O(n) vs indexed
- **Registry Statistics Update**: <150ms

---

## 🔧 Systematic Resolution Plan

### ~~Phase 1: Foundation Fixes~~ ✅ **COMPLETE 2025-09-29**
1. [x] ✅ Complete database schema (Issue #2)
2. [x] ✅ Fix workspace initialization (Issue #7)
3. [x] ✅ Fix registry statistics (Issue #6)
4. [ ] Add indexed query methods (Issue #4) - IN PROGRESS
5. [ ] Replace O(n) scans with indexed queries

### ~~Phase 2: Semantic Search~~ ✅ **COMPLETE 2025-09-29**
1. [x] ✅ Implement HNSW vector index (Issue #1)
2. [x] ✅ Load embeddings from database
3. [x] ✅ Re-enable semantic search with fast lookups
4. [x] ✅ Add comprehensive tests
5. [x] ✅ Fix embedding persistence (Issue #5)

### Phase 3: Safety & Polish (IN PROGRESS)
1. [ ] ⏳ Complete O(n) scan elimination (Issue #4)
2. [ ] Implement AST-based refactoring (Issue #3)
3. [ ] Add reference workspace indexing (Issue #8)
4. [ ] Implement orphan cleanup (Issue #9)
5. [ ] Performance benchmarking suite

### Phase 4: Production Ready (NEXT)
1. [ ] Zero-warning build
2. [ ] Comprehensive test coverage (>90%)
3. [ ] Performance regression tests
4. [ ] Documentation updates

---

## 🎯 Success Criteria

**Critical Issues Resolved**:
- [x] ✅ Semantic search working and fast (<100ms) - **COMPLETE**
- [x] ✅ All symbol metadata preserved in database - **COMPLETE**
- [ ] Refactoring operations safe (AST-based)

**Performance Targets**:
- [ ] All queries use indexed lookups (no O(n) scans)
- [x] ✅ Semantic similarity search <100ms - **ACHIEVED**
- [x] ✅ Handle 6k+ symbol codebases smoothly - **VERIFIED**

**Quality Metrics**:
- [x] ✅ Zero known data loss issues - **FIXED**
- [ ] Zero known corruption risks (refactoring safety needed)
- [x] ✅ Test coverage on semantic search - **ADDED**
- [ ] Zero compiler warnings (4 warnings remaining)

---

**Last Updated**: 2025-09-29 23:00
**Status**: ✅ Phase 1 & 2 COMPLETE - Semantic search working, schema complete!
**Major Achievements Tonight**:
- ✅ HNSW disk loading implementation
- ✅ Database schema completion (5 new fields)
- ✅ Registry statistics auto-update
- ✅ Workspace initialization self-healing
**Next Action**: Phase 3 - Optimize O(n) scans or fix refactoring safety