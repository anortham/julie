# Julie Architecture Debt & Technical Issues

**Purpose**: Track specific technical debt, incomplete implementations, and performance issues that need systematic resolution. This complements `docs/julie-implementation-checklist.md` (feature tracking) and `TODO.md` (observations).

**Total Known Issues**: 142 TODOs in codebase (as of 2025-09-29)

---

## 🔴 CRITICAL - Blocking Production Quality

### 1. **Semantic Search Completely Disabled** ⚠️ HIGH IMPACT
**Status**: ❌ Disabled since inception
**Location**: `src/tools/navigation.rs:262-289`
**Impact**: One of Julie's headline features (cross-language semantic search) doesn't work

**Problem**:
```rust
// Line 262-266
// TODO: DISABLED - This AI embedding computation on all 2458 symbols was causing UI hangs
// The expensive O(n) AI processing needs to be optimized or made optional
if false && exact_matches.is_empty() {
    // Disabled for performance
```

**Root Cause**:
- Line 276: `db_lock.get_all_symbols()` - loads ALL 6,085 symbols
- Line 286-289: Loops through EVERY symbol computing embeddings in real-time
- Result: 60+ seconds of ML work per query = UI hangs

**What's Broken**:
- [ ] No HNSW vector index (`src/embeddings/vector_store.rs:14` - only in-memory HashMap stub)
- [ ] Embeddings computed on-the-fly instead of loaded from database
- [ ] No efficient similarity search infrastructure
- [ ] `vectors/` directory empty (no persisted index)

**Fix Required**:
1. [ ] Implement HNSW index using [hnswlib](https://crates.io/crates/hnswlib) or similar
2. [ ] Load pre-computed embeddings from `embedding_vectors` table (DONE: generated during indexing)
3. [ ] Build HNSW index from stored embeddings on startup
4. [ ] Use HNSW for O(log n) approximate nearest neighbor search
5. [ ] Re-enable semantic search with fast index lookups

**Estimated Effort**: 2-3 days (HNSW integration + testing)

---

### 2. **Incomplete Database Schema** 🗄️ DATA LOSS
**Status**: ❌ Multiple fields missing
**Location**: `src/database/mod.rs` (multiple TODOs)
**Impact**: Losing critical symbol metadata on every index

**Missing Fields**:
- [ ] `start_byte` / `end_byte` - Needed for precise code navigation (TODO line 843, 844)
- [ ] `doc_comment` - Losing documentation strings (TODO line 845)
- [ ] `visibility` - Can't filter by public/private (TODO line 846)
- [ ] `code_context` - Losing surrounding code context (TODO line 847)

**Impact**:
- Navigation tools can't jump to exact byte positions
- Lost documentation means AI can't see function docs
- Can't implement "show only public API" filters
- Semantic understanding degraded without code context

**Fix Required**:
1. [ ] Add columns to `symbols` table schema
2. [ ] Update `store_symbols()` to persist all fields
3. [ ] Update `load_symbols()` to read new fields
4. [ ] Migration script for existing databases
5. [ ] Update tests to verify all fields

**Estimated Effort**: 1 day (schema migration + testing)

---

### 3. **Unsafe Refactoring Implementation** ⚠️ CODE CORRUPTION RISK
**Status**: ⚠️ Production risk
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

### 5. **Embeddings Recomputed Per Query** 🧠 60s WASTE
**Status**: ✅ FIXED (2025-09-29) but still disabled
**Location**: `src/tools/workspace/indexing.rs:975` (was TODO)
**Impact**: Was silently discarding all embedding computation

**History**:
- ❌ Before 2025-09-29: Generated 6,085 embeddings, threw them away
- ✅ After fix: Embeddings now persisted to database
- ⚠️ Still disabled: Semantic search still off due to lack of HNSW index

**Current State**:
- [x] Embeddings generated during background indexing (60s one-time cost)
- [x] Embeddings stored in `embedding_vectors` table (6,085 rows)
- [ ] No fast similarity search (need HNSW index)
- [ ] Semantic search still disabled in navigation.rs

**Next Steps**:
See Issue #1 (HNSW implementation) to make embeddings usable

---

## 🟢 MEDIUM PRIORITY - Features & Polish

### 6. **Reference Workspace Indexing Incomplete** 🔗 PHASE 4
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

### 7. **Orphaned Index Cleanup Missing** 🧹 MAINTENANCE
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

### Current Codebase Debt
- **Total TODOs**: 142 across codebase
- **Critical Issues**: 3 (semantic search, schema, refactoring safety)
- **High Priority**: 2 (O(n) scans, embedding recomputation - fixed)
- **Medium Priority**: 2 (reference workspaces, orphan cleanup)

### Test Coverage Gaps
- [ ] No tests for semantic search (because it's disabled!)
- [ ] Limited refactoring safety tests (string-based approach)
- [ ] No performance regression tests
- [ ] Missing cross-workspace integration tests

### Performance Baselines (for future benchmarks)
- **Indexing**: ~60s for 6,085 symbols (including embedding generation)
- **Text Search**: <10ms (Tantivy)
- **Semantic Search**: N/A (disabled)
- **Database Queries**: Unknown (need benchmarks for O(n) vs indexed)

---

## 🔧 Systematic Resolution Plan

### Phase 1: Foundation Fixes (Week 1)
1. Complete database schema (Issue #2)
2. Add indexed query methods (Issue #4)
3. Replace O(n) scans with indexed queries

### Phase 2: Semantic Search (Week 2)
1. Implement HNSW vector index (Issue #1)
2. Load embeddings from database
3. Re-enable semantic search with fast lookups
4. Add comprehensive tests

### Phase 3: Safety & Polish (Week 3)
1. Implement AST-based refactoring (Issue #3)
2. Add reference workspace indexing (Issue #6)
3. Implement orphan cleanup (Issue #7)
4. Performance benchmarking suite

### Phase 4: Production Ready (Week 4)
1. Zero-warning build
2. Comprehensive test coverage (>90%)
3. Performance regression tests
4. Documentation updates

---

## 🎯 Success Criteria

**Critical Issues Resolved**:
- [ ] Semantic search working and fast (<500ms)
- [ ] All symbol metadata preserved in database
- [ ] Refactoring operations safe (AST-based)

**Performance Targets**:
- [ ] All queries use indexed lookups (no O(n) scans)
- [ ] Semantic similarity search <500ms
- [ ] Handle 10k+ file codebases smoothly

**Quality Metrics**:
- [ ] Zero known data loss issues
- [ ] Zero known corruption risks
- [ ] Test coverage >90% on critical paths
- [ ] Zero compiler warnings (professional polish)

---

**Last Updated**: 2025-09-29
**Status**: Architectural audit complete, systematic resolution plan defined
**Next Action**: Begin Phase 1 (Foundation Fixes) with database schema completion