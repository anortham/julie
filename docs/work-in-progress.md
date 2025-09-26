# Julie Development Plan - PROPER TDD APPROACH

## 🔴 CRITICAL PRINCIPLE: TEST-DRIVEN DEVELOPMENT

**For file editing tools, TDD is NON-NEGOTIABLE:**
1. **Write failing tests FIRST**
2. **Implement minimal code to pass**
3. **Refactor with confidence**

**File corruption is UNACCEPTABLE. Tests prevent disasters.**

---

## Phase 0: Line-Based Editing Tool (SINGLE TOOL WITH MODES)

### Tool Design (Following Existing Patterns)
**ONE tool, not SIX:**

```rust
#[mcp_tool(name = "line_edit")]
pub struct LineEditTool {
    pub file_path: String,
    pub operation: String,  // "insert", "replace", "delete", "read", "count"
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
    pub content: Option<String>,
    pub preserve_indentation: bool,
    pub backup: bool,
    pub dry_run: bool,
}
```

This follows our pattern - `FastSearchTool` has modes, not 5 separate search tools!

### TDD Implementation Steps:
1. **Write comprehensive tests FIRST:**
   - Test insert operations (beginning, middle, end, beyond EOF)
   - Test replace operations (single line, multi-line, entire file)
   - Test delete operations (single, range, entire file)
   - Test edge cases (empty files, single line, huge files)
   - Test indentation preservation
   - Test backup creation
   - Test dry-run mode

2. **Golden Master Testing:**
   - Control files (original state)
   - Target files (expected result)
   - Test against real file corpus

3. **Only then implement the tool**

---

## Phase 1: Fix CRITICAL BLOCKERS

### 1. Split Large Files (Using new line_edit tool)
- razor.rs (86KB) → razor/mod.rs + implementations
- java_tests.rs (83KB) → separate test modules
- Target: No file over 30KB

### 2. Clean Build Warnings
- Remove dead code or add #[allow(dead_code)]
- Professional codebase = zero warnings

### 3. Persistent Indexing
- Use existing .julie/db/ for SQLite
- Use .julie/index/tantivy/
- Only re-index changed files

---

## Phase 2: HIGH VALUE Features

### 1. search_and_replace Tool
Simple facade combining fast_search + line_edit:
```rust
pub struct SearchAndReplaceTool {
    pub search_query: String,
    pub find_text: String,
    pub replace_text: String,
    pub file_pattern: Option<String>,
}
```

### 2. Response Modes (Simple, not overengineered)
```rust
pub enum ResponseMode {
    Summary,  // 5 results, minimal context
    Normal,   // 10 results, 10 lines (default)
    Full      // 20 results, 20 lines
}
```

---

## Phase 3: Quality & Testing

### 1. Integration Tests
- Verify "extractor" returns <15K tokens
- Test search quality ranking
- Cross-language symbol resolution

### 2. Simple Token Budgeting
```rust
const RESULTS_BUDGET: f32 = 0.7;
const CONTEXT_BUDGET: f32 = 0.3;
```

---

## ❌ AVOID These Mistakes

1. **NO tool explosion** - One tool with modes, not 6 separate tools
2. **NO implementation without tests** - TDD or death
3. **NO large files** - Keep everything under 30KB
4. **NO overengineering** - Simple response modes, not complex patterns

---

## Success Metrics

- [ ] ALL editing operations have tests WRITTEN FIRST
- [ ] Golden master tests prevent file corruption
- [ ] Single line_edit tool with modes (not 6 tools)
- [ ] Zero build warnings
- [ ] No file over 30KB

---

## Immediate Actions (After Approval)

1. **Delete the 6-tool mess** we just created
2. **Write comprehensive tests** for line editing operations
3. **Implement single line_edit tool** with operation modes
4. **Test with golden masters** before any real use
5. **Update work-in-progress.md** with this plan ✅
6. **Use tool to split razor.rs** as proof of concept

This plan emphasizes TDD, prevents tool bloat, and follows established patterns.

---

## Future Enhancements to Consider

### diff-match-patch Upgrade
Current implementation uses `diffy` but `diff-match-patch-rs` provides:
- Fuzzy patch application (better for when base text changes)
- Character-level diffs (better precision for MCP)
- Better JSON-RPC compatibility
- More robust patch application with conflict resolution

### Response Modes Integration
Add to existing tools:
- FastSearchTool gets response_mode parameter
- Simple budget allocation between results and context
- No complex overengineered patterns

---

**Status**: Phase 0 - Ready to implement with strict TDD methodology
**Last Updated**: 2025-09-26 - Plan saved and ready for execution