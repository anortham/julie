# Julie TODO

## 🎯 Current Status (2025-11-20)

**Latest Release**: v1.13.0 (2025-11-20)
**Latest Development**: Julie 2.0 Phase 5b Extended - TOON Integration Complete ✅
**Languages Supported**: 30/30 ✅
**Architecture**: CASCADE (SQLite FTS5 → HNSW Semantic)

### ✅ Recent Completions

**Julie 2.0 Phase 5b Extended - TOON Integration Complete (2025-11-20)**
- ✅ TOON support added to 8 tools achieving 50-70% token savings
- ✅ Tools with TOON: fast_search, fast_refs, fast_goto, get_symbols, find_logic, trace_call_path, smart_refactor, fuzzy_replace
- ✅ Eliminated dual-output waste (was sending markdown + JSON, now TOON-only or JSON-only)
- ✅ Auto mode: 5+ results → TOON format, <5 results → JSON format
- ✅ All build warnings fixed, clean release build
- 📊 Impact: 50-70% token reduction across all tool outputs

---

## 🚨 CRITICAL: TOON Implementation Issues (2025-11-20 Gemini Audit)

### Priority 0: Fix Critical Fallback Bug 🔥

**BROKEN**: `fast_refs` and `fast_search` have incorrect TOON fallback logic

**Problem**: When TOON encoding fails, they return human-readable text instead of structured JSON
- `fast_refs`: Falls back to markdown string in `text_content`
- `fast_search`: Falls back to stringified JSON in `text_content`

**Impact**: Breaks machine-readable contract with MCP clients. Agent cannot parse response.

**Root Cause**: Fallback returns string instead of `structured_content` JSON object

**Correct Behavior** (as seen in find_logic, get_symbols, trace_call_path):
```rust
// ✅ CORRECT: Falls back to structured JSON
Err(e) => {
    warn!("TOON encoding failed, falling back to JSON");
    let structured = serde_json::to_value(&result)?;
    let structured_map = if let serde_json::Value::Object(map) = structured {
        map
    } else {
        return Err(anyhow!("Expected JSON object"));
    };
    Ok(CallToolResult::text_content(vec![])
        .with_structured_content(structured_map))
}
```

**Files to Fix**:
1. `src/tools/navigation/fast_refs.rs` - Refactor `create_result()` function
2. `src/tools/search/mod.rs` - Fix `call_tool()` fallback logic

**Testing Required**:
- Unit test for TOON encoding failure scenario
- Verify fallback returns structured JSON, not text
- Test with intentionally malformed data that fails TOON encoding

**Confidence**: 95% - Clear implementation bug with known fix pattern

---

## 🎯 Active Priorities

### Priority 1: Add TOON Support to Missing Tools

#### High Priority: fast_goto

**Reason**: Can return multiple definitions (just like fast_refs which already has TOON)
**File**: `src/tools/navigation/fast_goto.rs`
**Effort**: Low (copy pattern from fast_refs)
**Impact**: High (consistency + token savings for multi-definition results)

**Implementation**:
- Add `output_format: Option<String>` parameter
- Use auto threshold of 5 results
- Follow exact pattern from fast_refs (after fixing fallback bug!)

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

#### trace_call_path Parameter Type

**Issue**: Uses `output_format: String` with default `"json"`
**Others use**: `Option<String>` with default `None`

**File**: `src/tools/trace_call_path/mod.rs`

**Fix**:
```rust
// Change from:
#[serde(default = "default_output_format")]
pub output_format: String,

// To:
#[serde(default)]
pub output_format: Option<String>,
```

**Impact**: Low (cosmetic consistency)
**Effort**: Trivial (5 minute fix)

#### get_symbols Auto Threshold

**Issue**: Auto threshold of 5 is too low for file structure review
**Problem**: Files commonly have >5 symbols, JSON is more readable for quick reviews

**Suggested Change**: Increase auto threshold to 15-20 for get_symbols only

**Tradeoff**:
- Pro: Better UX for moderate-sized files
- Con: Less token savings in 5-15 symbol range

**Decision Required**: User preference - keep at 5 for max savings, or tune to 15-20 for UX?

---

### Priority 3: Code Quality - Eliminate Duplication

**Issue**: `output_format` match logic duplicated across 4 tools
**Files**: fast_refs, find_logic, get_symbols, trace_call_path
**Lines of Duplication**: ~30 lines × 4 tools = 120 lines total

**Proposed Solution**: Create shared helper in `src/tools/shared.rs`

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

### Phase 3: Refactor for Maintainability (Future Session)
1. Create shared `create_toonable_result()` helper
2. Refactor 4 tools to use helper
3. Update tests to cover helper function

### Phase 4: Advanced Optimizations (Future - Optional)
1. Custom TOON encoder for hierarchical data (get_symbols, trace_call_path)
2. Custom TOON encoder for string deduplication (fast_refs, fast_search)
3. Conditional column omission (all tools)
4. Measure token savings vs default TOON

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
