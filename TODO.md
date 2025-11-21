# Julie TODO

## 🎯 Current Status (2025-11-20)

**Latest Release**: v1.13.0 (2025-11-20)
**Latest Development**: Julie 2.0 Phase 5b Extended - TOON Integration Complete ✅
**Languages Supported**: 30/30 ✅
**Architecture**: CASCADE (SQLite FTS5 → HNSW Semantic)

### ✅ Recent Completions

**Julie 2.0 Phase 3 - TOON Duplication Elimination Complete (2025-11-20)**
- ✅ Created shared `create_toonable_result()` helper eliminating 230 lines of duplication
- ✅ Refactored 4 tools to use shared helper (fast_refs, fast_goto, find_logic, trace_call_path)
- ✅ Fixed all 32 test failures from refactoring
- ✅ All 1845 tests passing, clean build, committed to main (7e39f4b)
- 📊 Impact: Single source of truth for TOON logic, maintainable architecture

**Julie 2.0 Phase 1 & 2 - TOON Fixes & Extensions (2025-11-20)**
- ✅ Fixed critical fallback bugs in fast_refs/fast_search (broke MCP contract)
- ✅ Added TOON support to fast_goto (50-70% token savings)
- ✅ Standardized all tools to use Option<String> for output_format parameter
- ✅ Fixed 49 test compilation errors
- 📊 Impact: Correct JSON fallback behavior, consistent tool APIs

**Julie 2.0 Phase 5b Extended - TOON Integration Complete (2025-11-20)**
- ✅ TOON support added to 8 tools achieving 50-70% token savings
- ✅ Tools with TOON: fast_search, fast_refs, fast_goto, get_symbols, find_logic, trace_call_path, smart_refactor, fuzzy_replace
- ✅ Eliminated dual-output waste (was sending markdown + JSON, now TOON-only or JSON-only)
- ✅ Auto mode: 5+ results → TOON format, <5 results → JSON format
- ✅ All build warnings fixed, clean release build
- 📊 Impact: 50-70% token reduction across all tool outputs

---

## ~~🚨 CRITICAL: TOON Implementation Issues~~ ✅ FIXED (2025-11-20)

### ~~Priority 0: Fix Critical Fallback Bug~~ ✅ COMPLETE

**STATUS**: Fixed in Phase 1 - All TOON fallback bugs eliminated

**What Was Broken**: `fast_refs` and `fast_search` returned text strings instead of structured JSON when TOON encoding failed, breaking the MCP contract.

**What Was Fixed**:
- ✅ `fast_refs`: Now falls back to structured JSON via shared helper
- ✅ `fast_search`: Fixed 2 locations, made ToonResponse/ToonSymbol public
- ✅ All tools now use correct fallback behavior (structured JSON, not text)
- ✅ Tests updated and passing

**Result**: Machine-readable contract preserved. Agent can always parse responses.

---

## 🎯 Active Priorities

### Priority 1: Add TOON Support to Missing Tools

#### ~~High Priority: fast_goto~~ ✅ COMPLETE

**STATUS**: Fixed in Phase 2 - TOON support fully implemented

**What Was Done**:
- ✅ Added `output_format: Option<String>` parameter
- ✅ Auto threshold of 5 results
- ✅ Follows shared helper pattern
- ✅ 50-70% token savings for multi-definition results

#### Medium Priority: smart_refactor / rename_symbol

**Reason**: Large `files_modified` arrays when renaming across workspace
**File**: `src/tools/refactoring/mod.rs`
**Effort**: Medium (need to identify result size threshold)
**Impact**: Medium (only high for large refactorings)

**Implementation**:
- Add `output_format` parameter to rename/refactor tools
- Auto threshold: 10+ files modified
- TOON format for file lists is highly compact

#### Medium Priority: manage_workspace (list operation)

**Reason**: List operation could return many workspaces
**File**: `src/tools/workspace/commands/mod.rs`
**Current State**: Returns formatted string (not structured!)
**Effort**: High (need to restructure output first)

**Implementation**:
1. First: Convert list operation to return structured JSON
2. Then: Add TOON support with auto threshold of 5+ workspaces

---

### Priority 2: Fix Minor Inconsistencies

#### ~~trace_call_path Parameter Type~~ ✅ COMPLETE

**STATUS**: Fixed in Phase 2 - Now consistent with all other tools

**What Was Done**:
- ✅ Changed from `output_format: String` to `Option<String>`
- ✅ Updated 7 call sites to use `.as_deref()`
- ✅ Now matches pattern used by all other tools

#### get_symbols Auto Threshold ⏸️ DEFERRED

**Issue**: Auto threshold of 5 is too low for file structure review
**Problem**: Files commonly have >5 symbols, JSON is more readable for quick reviews

**Suggested Change**: Increase auto threshold to 15-20 for get_symbols only

**Tradeoff**:
- Pro: Better UX for moderate-sized files
- Con: Less token savings in 5-15 symbol range

**Decision Required**: User preference - keep at 5 for max savings, or tune to 15-20 for UX?

---

### ~~Priority 3: Code Quality - Eliminate Duplication~~ ✅ COMPLETE (Phase 3)

**STATUS**: Completed in Phase 3 - Shared helper implemented and deployed

**What Was Done**:
- ✅ Created `create_toonable_result()` helper in `src/tools/shared.rs`
- ✅ Refactored 4 tools to use shared helper
- ✅ Eliminated ~230 lines of duplication
- ✅ Single source of truth for TOON/JSON formatting
- ✅ All 1845 tests passing

**Original Issue**: `output_format` match logic duplicated across 4 tools
**Original Files**: fast_refs, find_logic, get_symbols, trace_call_path
**Original Duplication**: ~30 lines × 4 tools = 120 lines total

**Solution Implemented**: Created shared helper in `src/tools/shared.rs`

```rust
/// Generic helper for TOON/JSON output formatting
/// Centralizes TOON encoding, auto mode logic, and fallback handling
pub fn create_toonable_result<T: Serialize>(
    result_data: &T,
    output_format: Option<&str>,
    auto_threshold: usize,
    result_count: usize,
) -> anyhow::Result<CallToolResult> {
    let use_toon = match output_format {
        Some("toon") => true,
        Some("auto") => result_count >= auto_threshold,
        _ => false,
    };

    if use_toon {
        if let Ok(toon) = toon_format::encode_default(result_data) {
            debug!("✅ TOON encoded: {} chars for {} results", toon.len(), result_count);
            return Ok(CallToolResult::text_content(vec![toon.into()]));
        }
        warn!("❌ TOON encoding failed, falling back to JSON");
    }

    // Fallback to structured JSON
    let structured = serde_json::to_value(result_data)?;
    let structured_map = if let serde_json::Value::Object(map) = structured {
        map
    } else {
        return Err(anyhow!("Failed to serialize result to JSON object"));
    };

    Ok(CallToolResult::text_content(vec![]).with_structured_content(structured_map))
}
```

**Benefits**:
- Single source of truth for TOON logic
- Ensures all tools handle fallback correctly
- Future TOON improvements apply everywhere
- Reduces maintenance burden

**Effort**: Medium (2-3 hours)
- Create helper function
- Refactor 4 tools to use helper
- Test all 4 tools still work correctly

**Impact**: High (maintainability + ensures correct fallback everywhere)

---

## 🚀 Advanced Optimization Opportunities

### Custom TOON Encoders for Hierarchical Data

**Context**: Default TOON encoder is effective but suboptimal for tree structures

**Opportunity 1: Flatten Hierarchical Data**

**Tools Affected**: `get_symbols`, `trace_call_path`
**Current Approach**: YAML-like nested structure
**Problem**: Repetitive indentation and structure tokens

**Proposed Solution**: Tabular format with parent IDs

```
Current TOON (nested):
```yaml
- name: MyClass
  kind: class
  children:
    - name: method1
      kind: method
    - name: method2
      kind: method
```

Optimized TOON (flat with parent_id):
```
| id | parent_id | name     | kind   |
|----|-----------|----------|--------|
| 1  | null      | MyClass  | class  |
| 2  | 1         | method1  | method |
| 3  | 1         | method2  | method |
```

**Token Savings**: 30-50% additional reduction for deeply nested structures
**Effort**: High (custom TOON encoder implementation)
**Impact**: High for get_symbols (files with nested classes/functions)

---

### Custom TOON Encoders for Repeated Strings

**Opportunity 2: Shared String Optimization with @def Directives**

**Tools Affected**: `fast_refs`, `get_symbols`, `fast_search`
**Current Approach**: Repeat full file paths for every result
**Problem**: `file_path` string repeated 10-50 times in large result sets

**Proposed Solution**: TOON @def directive for string deduplication

```
Current TOON:
| symbol | file_path                          |
|--------|------------------------------------|
| foo    | src/services/auth/validation.rs    |
| bar    | src/services/auth/validation.rs    |
| baz    | src/services/auth/validation.rs    |

Optimized TOON:
@def p1 "src/services/auth/validation.rs"

| symbol | file_path |
|--------|-----------|
| foo    | $p1       |
| bar    | $p1       |
| baz    | $p1       |
```

**Token Savings**: 20-40% for results with repeated paths/strings
**Effort**: Medium (extend TOON encoder with @def support)
**Impact**: High for workspace-wide searches (many results from same files)

---

### Conditional Column Omission

**Opportunity 3: Omit Empty Columns**

**Tools Affected**: All tools with optional fields
**Example**: `doc_comment` field in search results
**Current Approach**: Include column even if all values are None

**Proposed Solution**: Custom encoder that omits columns where all values are None

```
Current TOON (doc_comment always None):
| symbol | kind     | doc_comment |
|--------|----------|-------------|
| foo    | function | null        |
| bar    | function | null        |
| baz    | function | null        |

Optimized TOON (column omitted):
| symbol | kind     |
|--------|----------|
| foo    | function |
| bar    | function |
| baz    | function |
```

**Token Savings**: 10-20% when optional fields consistently empty
**Effort**: Low (add column filtering to TOON encoder)
**Impact**: Medium (varies by query, biggest for results without docs)

---

## 📊 Implementation Plan

### Phase 1: Fix Critical Issues ✅ COMPLETE (2025-11-20)
1. ✅ Gemini audit complete
2. ✅ Fixed fallback bug in fast_refs (removed broken format_results_text helper)
3. ✅ Fixed fallback bug in fast_search (fixed 2 locations, made ToonResponse/ToonSymbol public)
4. ✅ Fixed 49 test compilation errors (missing output_format fields from Phase 5b)
5. ✅ Tests compiling and running (1,804 passing)

**Result**: Critical fallback bugs eliminated. TOON encoding failures now correctly fall back to structured JSON instead of text strings, preserving machine-readable contract with MCP clients.

### Phase 2: Add Missing TOON Support ✅ COMPLETE (2025-11-20)
1. ✅ Added TOON to fast_goto (output_format parameter, auto threshold 5, proper fallback)
2. ✅ Standardized trace_call_path parameter type (now Option<String> like other tools)
3. ⏸️ Deferred get_symbols auto threshold adjustment (keep at 5 for max token savings)

**Result**: fast_goto now has 50-70% token savings with TOON. All tools now consistently use Option<String> for output_format parameter.

### Phase 3: Refactor for Maintainability ✅ COMPLETE (2025-11-20)
1. ✅ Created shared `create_toonable_result()` helper in src/tools/shared.rs
2. ✅ Refactored 4 tools to use helper (fast_refs, fast_goto, find_logic, trace_call_path)
3. ✅ Eliminated ~230 lines of duplicated TOON encoding logic
4. ✅ Fixed all 32 test failures from refactoring (updated test assertions for new behavior)
5. ✅ All 1845 tests passing
6. ✅ Committed and pushed to main (commit 7e39f4b)

**Result**: Single source of truth for TOON/JSON formatting. All tools now use shared helper for consistent behavior. Future TOON improvements will automatically apply to all tools. Clean, maintainable codebase ready for Phase 4 custom encoders.

### Phase 4: Data Structure Optimizations ✅ IN PROGRESS (2025-11-20)
1. ✅ Research: TOON format doesn't support custom @def directives (aspirational in original TODO)
2. ✅ Pragmatic approach: Optimize data structures BEFORE TOON encoding (serde attributes)
3. ✅ Implemented: `skip_serializing_if = "Option::is_none"` on 9 Symbol optional fields
4. ✅ Tested: Comprehensive token savings measurement tests (3 passing)
5. ✅ **Results: 39.3% token reduction** (2000 chars / ~500 tokens for 10 symbols)
6. ⏸️ Deferred: Custom TOON encoders for hierarchical data (complex, lower ROI)
7. ⏸️ Deferred: String deduplication with @def (not supported by TOON format)

**Implementation Details:**
- Modified: `src/extractors/base/types.rs` - Added skip_serializing_if to 9 fields
- Tests: `src/tests/tools/phase4_token_savings.rs` - 3 comprehensive tests
- Fields optimized: signature, doc_comment, visibility, parent_id, metadata, semantic_group, confidence, code_context, content_type

**Result**: Simple, maintainable solution that works with ANY serialization format (JSON, TOON, etc.).

**Combined Impact (Phases 1-4):**
- Phase 3 TOON encoding: 50-70% reduction
- Phase 4 Data optimization: 39% additional reduction
- **Total: ~70-80% token savings!** 🚀

### Phase 5: Hierarchical TOON Optimization 🚧 READY TO IMPLEMENT (2025-11-20)

**Status**: Design complete, ready for implementation

**Problem**: trace_call_path is a "token monster" - returns recursive tree structures that don't fit TOON's tabular model, causing fallback to YAML-ish format with repeated keys at every level. Can easily hit 10,000-30,000+ characters on large queries.

**Impact Analysis**:
```
Current (YAML-ish with repeated keys):
- 32 paths × 6 nodes × 150 chars/node ≈ 28,800 chars

With Flat Table Optimization:
- Header: nodes[192]{id,parent_id,group,level,...} ≈ 120 chars
- Data rows: 192 rows × 55 chars/row ≈ 10,560 chars
- Total: ~10,680 chars

Savings: 63% reduction! 🚀
Combined with Phase 4: ~77% total savings!
```

**Design Decision - Single Flat Table (Option A)**:

**Key Insight**: Both JSON and TOON are for AI/machine consumption, not humans. Optimize purely for:
1. Token efficiency (smaller = better)
2. Parseability (structured, unambiguous)
3. Information preservation (lossless)
4. Reconstruction capability (parent_id pointers)

Human readability is **irrelevant** - if debugging needed, use `output_format: "json"`.

**Architecture**:

```rust
/// Generic trait for hierarchical data that can be flattened to TOON format
pub trait HierarchicalToonable {
    type NodeData: Serialize;

    /// Flatten recursive tree into single flat table with parent_id references
    fn flatten(&self) -> Vec<FlatNode<Self::NodeData>>;
}

/// Flattened node for hierarchical TOON encoding
#[derive(Debug, Clone, Serialize)]
pub struct FlatNode<T> {
    /// Unique ID within this result set
    pub id: usize,

    /// Parent node ID (null for root nodes)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<usize>,

    /// Group ID (e.g., which call path this belongs to)
    pub group: usize,

    /// Depth level in tree (0 = root)
    pub level: u32,

    /// The actual node data (inlined via #[serde(flatten)])
    #[serde(flatten)]
    pub data: T,
}
```

**Example Output**:
```
nodes[192]{id,parent_id,group,level,symbol_name,file_path,language,line,match_type}:
  0,null,0,0,extract_symbols,src/tests/bash/mod.rs,rust,22,direct
  1,0,0,1,test_extract_bash_functions,src/tests/bash/mod.rs,rust,53,direct
  2,0,0,1,test_extract_control_flow,src/tests/bash/mod.rs,rust,321,direct
  3,null,1,0,extract_symbols,src/tests/c/mod.rs,rust,27,direct
  4,3,1,1,test_extract_c_functions,src/tests/c/mod.rs,rust,54,direct
```

**Implementation Steps**:

1. **Create Generic Infrastructure** (`src/tools/hierarchical_toon.rs`):
   - `HierarchicalToonable` trait
   - `FlatNode<T>` struct
   - Helper functions for flattening recursive trees

2. **Implement for TraceCallPathResult**:
   - Create `FlatCallPathNode` data type
   - Implement `flatten()` using depth-first traversal
   - Track id, parent_id, group (path_id), level

3. **Integrate with Encoding**:
   - Create `create_hierarchical_toon_result()` helper
   - Use shared pattern from Phase 3 (auto mode, fallback, etc.)
   - Reusable for other hierarchical tools (get_symbols)

4. **Comprehensive Testing**:
   - Unit tests for flattening logic
   - Round-trip tests (flatten → reconstruct tree)
   - Token savings measurement vs current output
   - Edge cases: empty trees, single node, deep nesting

**Benefits**:
- ✅ Maximum token efficiency (single table, no redundancy)
- ✅ Simpler for AI agents (uniform schema, easy to parse)
- ✅ Trivial reconstruction (parent_id references)
- ✅ Reusable pattern (trait-based, works for any tree)
- ✅ Same TDD methodology from previous phases

**Why Not Multi-Table (Option B)?**
- Unnecessary complexity (two tables vs one)
- Token overhead from two headers
- Only benefit was "visual grouping" which AI doesn't need

**Affected Tools**:
- trace_call_path (primary - biggest offender)
- get_symbols (nested symbols: classes → methods → nested functions)
- Any future hierarchical data

**Files to Create/Modify**:
- NEW: `src/tools/hierarchical_toon.rs` - Generic infrastructure
- MODIFY: `src/tools/trace_call_path/mod.rs` - Implement trait
- NEW: `src/tests/tools/hierarchical_toon_tests.rs` - Comprehensive tests

**Success Criteria**:
- [ ] Generic `HierarchicalToonable` trait implemented
- [ ] trace_call_path returns flat tabular TOON (no repeated keys)
- [ ] Token savings: 60-70% reduction vs current YAML-ish output
- [ ] All existing tests pass
- [ ] New tests verify flattening correctness
- [ ] Round-trip reconstruction tests pass

**Next Steps**:
- Implement in fresh context following TDD methodology
- Start with trait definition and tests
- Implement for trace_call_path first (biggest impact)
- Extend to get_symbols once proven

---

## 🧠 Key Learnings from TOON Audit

1. **Fallback failures are silent bugs** - Happy path (TOON succeeds) hides broken contracts. Always test error paths.

2. **Consistency matters for agent UX** - Small inconsistencies (Option<String> vs String) create cognitive load.

3. **Code duplication spreads bugs** - fast_refs and fast_search both have same fallback bug because logic was copy-pasted.

4. **Gemini excels at codebase-wide analysis** - Read ~15,000 lines, found specific line-level issues with code examples.

5. **Default TOON is good, custom encoders are better** - 50-70% savings from default, potentially 70-90% with custom encoders.

6. **Token optimization is ongoing work** - First pass eliminates waste, subsequent passes tune for specific data patterns.

7. **Behavioral adoption vs documentation** (2025-11-20) - Agent used sed for 49 struct field additions instead of Julie's `fuzzy_replace` tool, despite it being documented. Problem: Instructions describe WHAT tools exist and HOW to use them, but not WHEN/WHY to use them over alternatives. Three types of knowledge needed: (1) Declarative (tool exists), (2) Procedural (syntax/usage), (3) Situational (when to choose this tool vs bash). Solution: Add "When to Use" guidance, behavioral triggers ("Before using sed, consider fuzzy_replace"), and anti-patterns to tool descriptions. Recent token optimization may have stripped too much behavioral guidance. Need to balance token efficiency with adoption effectiveness.

---

## 📝 Scratchpad / Investigation Notes

### Questions to Explore

- Should custom TOON encoders be per-tool or per-data-type?
- What's the best auto threshold for get_symbols? (5 vs 15-20)
- Should we expose TOON encoding as a library for other MCP servers?
- Can we auto-detect optimal TOON encoding strategy based on result structure?

### Testing Strategy

- Add TOON encoding failure tests to ALL tools (currently missing)
- Add tests for auto mode threshold boundary conditions (4 vs 5 vs 6 results)
- Add benchmark tests to measure token savings (default vs custom encoders)

---

## 📊 Key Metrics

**Token Reduction (TOON Phase 5b)**: 50-70% across 8 tools
**Search Performance**: <5ms (FTS5), <50ms (HNSW semantic)
**Language Coverage**: 30 languages with tree-sitter parsers
**Test Coverage**: Comprehensive (SOURCE/CONTROL methodology)

**Target Token Reduction (Custom Encoders)**: 70-90% (30-50% improvement over default)


Look at this: 

⏺ julie - checkpoint (MCP)(description: "Behavioral Adoption Insight Added to TODO.md: Discovered agent used sed for 49 struct modifications instead of Julie's
                          fuzzy_replace despite it being documented. Root cause: Instructions describe WHAT/HOW but not WHEN/WHY to use tools over alternatives. Three
                          knowledge types needed: (1) Declarative (tool exists), (2) Procedural (syntax), (3) Situational (when to choose). Recent token optimization may
                          have stripped too much behavioral guidance. Solution: Add \"When to Use\" sections, behavioral triggers, anti-patterns. Balance token efficiency
                          with adoption effectiveness. This is a real-world example of the behavioral adoption challenge - having the better tool doesn't matter if agents
                          don't know when to reach for it.", tags: ["behavioral-adoption","tool-instructions","learning","ux","dogfooding"])
  ⎿ ✅ Checkpoint saved successfully!

    📝 Behavioral Adoption Insight Added to TODO.md: Discovered agent used sed for 49 struct modifications instead of Julie's fuzzy_replace despite it being
    documented. Root cause: Instructions describe WHAT/HOW but not WHEN/WHY to use tools over alternatives. Three knowledge types needed: (1) Declarative (tool
    exists), (2) Procedural (syntax), (3) Situational (when to choose). Recent token optimization may have stripped too much behavioral guidance. Solution: Add "When
    to Use" sections, behavioral triggers, anti-patterns. Balance token efficiency with adoption effectiveness. This is a real-world example of the behavioral adoption
     challenge - having the better tool doesn't matter if agents don't know when to reach for it.
    🆔 checkpoint_691f757d_7ecdb2
    📂 .memories/2025-11-20/200933_a83f.json
    📍 Git: main @ e126e76
    🏷️  Tags: behavioral-adoption, tool-instructions, learning, ux, dogfooding

    Memory will be indexed automatically and searchable via fast_search.

The agent submits the checkpoint description and we're returning the whole thing back again in the tool result. This is wasting tokens. All we should really return is an indication if the tool was successful.

---

*Last Updated: 2025-11-20*
*Status: Julie 2.0 Phase 5b Extended Complete - TOON optimization in progress*
