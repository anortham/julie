# MCP Tool API Consistency Audit
**Date:** 2025-10-23
**Auditor:** Claude (AI Agent - Primary User of Julie)
**Total Tools Reviewed:** 11

## Executive Summary

This audit reviewed all 11 MCP tools for consistency in:
- Parameter naming conventions
- Parameter descriptions and documentation
- Smart default values and their documentation
- Tool-level descriptions

**Key Findings:**
- ✅ **GOOD:** Core parameter naming is consistent (`workspace`, `file_path`, `dry_run`)
- ⚠️  **ISSUES:** 7 inconsistencies found requiring fixes
- 🔴 **CRITICAL:** TraceCallPathTool documents unsupported "all" workspace option (violates architecture)

---

## Tool Inventory

### Search & Navigation (6 tools)
1. **FastSearchTool** - src/tools/search/mod.rs
2. **FastGotoTool** - src/tools/navigation/fast_goto.rs
3. **FastRefsTool** - src/tools/navigation/fast_refs.rs
4. **GetSymbolsTool** - src/tools/symbols.rs
5. **TraceCallPathTool** - src/tools/trace_call_path.rs
6. **FindLogicTool** - src/tools/exploration/find_logic/mod.rs

### Editing (2 tools)
7. **EditLinesTool** - src/tools/edit_lines.rs
8. **FuzzyReplaceTool** - src/tools/fuzzy_replace.rs

### Refactoring (2 tools)
9. **RenameSymbolTool** - src/tools/refactoring/mod.rs
10. **EditSymbolTool** - src/tools/refactoring/mod.rs

### Workspace (1 tool)
11. **ManageWorkspaceTool** - src/tools/workspace/commands/mod.rs

---

## Consistency Analysis

### ✅ What's Working Well

**1. Core Parameter Naming:**
- `workspace: Option<String>` - Used consistently across 6 tools
- `file_path: String` - Used consistently across 4 tools
- `dry_run: bool` - Used consistently across 4 tools
- `symbol: String` - Used consistently across 3 tools (FastGoto, FastRefs, TraceCallPath)

**2. Default Function Convention:**
- All tools using `workspace` parameter have `default_workspace()` → `Some("primary")`
- Consistent `#[serde(default = "function_name")]` attribute pattern
- Clear naming: `default_true()`, `default_limit()`, `default_max_depth()`, etc.

**3. Tool Categorization:**
- Clear `meta` tags with categories ("search", "navigation", "editing", "workspace")
- Consistent MCP hint fields (idempotent_hint, destructive_hint, read_only_hint)

---

## ⚠️  Issues Found

### ISSUE 1: 🔴 **CRITICAL - TraceCallPathTool Incorrect Documentation**

**Location:** `src/tools/trace_call_path.rs:132`

**Current Documentation:**
```rust
/// Workspace filter (default: "primary").
/// Options: "all", "primary", or specific workspace ID
#[serde(default = "default_workspace")]
pub workspace: Option<String>,
```

**Problem:**
- Documents "all" as a valid option
- **VIOLATES** Single-Workspace Search Policy from CLAUDE.md:
  > "Search operations ALWAYS target ONE workspace at a time. No exceptions."
- Misleads agents into thinking multi-workspace search is supported

**Fix Required:**
```rust
/// Workspace filter (optional): "primary" (default) or specific workspace ID
/// Examples: "primary", "reference-workspace_abc123"
/// Default: "primary" - search the primary workspace
/// Note: Multi-workspace search ("all") is not supported - search one workspace at a time
#[serde(default = "default_workspace")]
pub workspace: Option<String>,
```

**Impact:** HIGH - Architectural principle violation

---

### ISSUE 2: **Workspace Parameter Description Inconsistency**

**Problem:** Three different documentation styles for the same `workspace` parameter:

**Style A (FastSearchTool):** ✅ Most comprehensive
```rust
/// Workspace filter (optional): "primary" (default) or specific workspace ID
/// Examples: "primary", "reference-workspace_abc123"
/// Default: "primary" - search the primary workspace
/// Note: Multi-workspace search ("all") is not supported - search one workspace at a time
```

**Style B (GetSymbolsTool, FastGotoTool):**
```rust
/// Workspace filter (default: "primary").
/// Specify which workspace to search: "primary" (default) or specific workspace ID
/// Examples: "primary", "project-b_a3f2b8c1"
/// To search a reference workspace, provide its workspace ID
```

**Style C (TraceCallPathTool):** 🔴 WRONG (see Issue 1)

**Recommendation:**
- **Standardize on Style A** (FastSearchTool format)
- It's the most comprehensive and includes the important architectural note
- Update GetSymbolsTool and FastGotoTool to match

**Affected Files:**
- src/tools/symbols.rs:97-102 (GetSymbolsTool)
- src/tools/navigation/fast_goto.rs:67-72 (FastGotoTool)
- src/tools/trace_call_path.rs:131-134 (TraceCallPathTool)

---

### ISSUE 3: **File Path Parameter Examples Inconsistency**

**Problem:** Different example files used across tools

**GetSymbolsTool:**
```rust
/// File path to get symbols from (relative to workspace root)
/// Example: "src/user.rs", "lib/services/auth.py"
pub file_path: String,
```

**EditLinesTool:**
```rust
/// File path to edit (relative to workspace root)
/// Example: "src/main.rs", "lib/auth.py"
pub file_path: String,
```

**EditSymbolTool:**
```rust
/// File path to edit
pub file_path: String,
```
*No example provided!*

**Recommendation:**
- Standardize on consistent examples: `"src/main.rs"`, `"lib/services/auth.py"`
- Include "(relative to workspace root)" in all descriptions
- Always provide examples - they're helpful for agents

**Affected Files:**
- src/tools/symbols.rs:63-65
- src/tools/edit_lines.rs:43-45
- src/tools/refactoring/mod.rs:85-86

---

### ISSUE 4: **Symbol Parameter Documentation Depth Varies**

**Problem:** Inconsistent level of detail for symbol name parameters

**FastGotoTool:** ✅ Very detailed (good!)
```rust
/// Symbol name to navigate to. Supports simple and qualified names.
/// Examples: "UserService", "MyClass::method", "std::vector", "React.Component", "getUserData"
/// Julie intelligently resolves across languages (Python imports, Rust use statements, TypeScript imports)
pub symbol: String,
```

**FastRefsTool:** ❌ Minimal
```rust
/// Symbol name to find all references/usages for.
/// Examples: "UserService", "handleRequest", "myFunction", "CONSTANT_NAME"
/// Same format as fast_goto - Julie will find every place this symbol is used
pub symbol: String,
```

**TraceCallPathTool:** ❌ Minimal
```rust
/// Symbol to start tracing from
/// Examples: "getUserData", "UserService.create", "processPayment"
pub symbol: String,
```

**Recommendation:**
- FastRefsTool and TraceCallPathTool should match FastGotoTool's detail level
- Mention cross-language support and qualified name support
- Helps agents understand the tool's capabilities

**Affected Files:**
- src/tools/navigation/fast_refs.rs (add cross-language details)
- src/tools/trace_call_path.rs:109-111 (add qualified name support details)

---

### ISSUE 5: **Default Value Documentation Format Inconsistency**

**Problem:** Multiple styles for documenting default values in comments

**Style A:** Default in first line
```rust
/// Maximum results to return (default: 10, range: 1-500).
#[serde(default = "default_limit")]
pub limit: u32,
```

**Style B:** Default on separate line
```rust
/// Workspace filter (default: "primary").
/// Specify which workspace to search...
#[serde(default = "default_workspace")]
pub workspace: Option<String>,
```

**Style C:** "Default:" with capital D
```rust
/// Workspace filter (optional): "primary" (default) or specific workspace ID
/// Examples: "primary", "reference-workspace_abc123"
/// Default: "primary" - search the primary workspace
#[serde(default = "default_workspace")]
pub workspace: Option<String>,
```

**Recommendation:**
- **Standardize on Style A** for simple parameters: `(default: value, range: min-max)`
- Use Style C for complex parameters where default needs explanation
- Be consistent within each tool

**Note:** This is lower priority - all styles work, but consistency improves readability

---

### ISSUE 6: **dry_run Parameter Documentation Varies**

**Problem:** Different levels of detail for the same parameter purpose

**EditLinesTool:** ❌ Minimal
```rust
/// Preview changes without applying (default: false).
/// Set true to see what would change before actually modifying files
#[serde(default)]
pub dry_run: bool,
```

**FuzzyReplaceTool:** ✅ More helpful
```rust
/// Preview changes without applying them (default: false).
/// RECOMMENDED: Set true for first run to verify changes before applying
#[serde(default)]
pub dry_run: bool,
```

**RenameSymbolTool:** ❌ Minimal
```rust
/// Preview changes without applying them (default: false)
#[serde(default)]
pub dry_run: bool,
```

**Recommendation:**
- Add "RECOMMENDED: Set true for first run" guidance to all editing tools
- Helps agents develop good safety practices
- Particularly important for destructive operations

**Affected Files:**
- src/tools/edit_lines.rs:61-64
- src/tools/refactoring/mod.rs:62-64, 106-108

---

### ISSUE 7: **Tool Description Tone/Length Inconsistency**

**Problem:** Wildly different description styles and lengths

**FastSearchTool:** Very comprehensive (6 sentences + emojis)
```
"ALWAYS SEARCH BEFORE CODING - This is your PRIMARY tool for finding code patterns and content.
You are EXCELLENT at using fast_search efficiently.
Results are always accurate - no verification with grep or Read needed.\n\n
🎯 USE THIS WHEN: Searching for text, patterns, TODOs, comments, or code snippets.\n
💡 USE fast_goto INSTEAD: When you know a symbol name and want to find its definition
(fast_goto has fuzzy matching and semantic search built-in).\n\n
IMPORTANT: I will be disappointed if you write code without first using this tool to check for existing implementations!\n\n
Performance: <10ms for text search, <100ms for semantic.
Trust the results completely and move forward with confidence."
```

**FindLogicTool:** Minimal (1 sentence)
```
"DISCOVER CORE LOGIC - Filter framework noise, focus on domain business logic"
```

**EditSymbolTool:** Medium detail (3 paragraphs)
```
"FILE-SPECIFIC SEMANTIC EDITING - Modify function bodies, insert code, or move symbols between files.
You are EXCELLENT at using this for precise code transformations.
This tool understands code structure and preserves formatting automatically.\n\n
**Operations**:\n
• ReplaceBody: Replace function/method implementation\n
• InsertRelative: Add code before/after symbols\n
• ExtractToFile: Move symbols to different files with import updates\n\n
**Perfect for**: Updating implementations, adding helper functions, reorganizing code\n\n
Unlike text editing, this preserves indentation and code structure automatically."
```

**Recommendation:**
- Not necessarily a problem - some tools are more complex and need more explanation
- **But consider:** Should FindLogicTool have more guidance for agents?
- **Standardize tone:** All tools should address the agent directly ("You are EXCELLENT at...")
- **Include performance hints** where relevant (helps agents make smart choices)

**Lower Priority** - Functional descriptions work, but enhanced guidance helps agents

---

## Summary of Fixes Needed

### 🔴 High Priority (Must Fix)
1. **TraceCallPathTool workspace docs** - Remove "all" option, align with architecture
2. **Standardize workspace parameter docs** - Use FastSearchTool format across all 4 tools

### 🟡 Medium Priority (Should Fix)
3. **file_path examples** - Consistent examples with "(relative to workspace root)"
4. **symbol parameter docs** - Add cross-language support details to FastRefs/TraceCallPath
5. **dry_run guidance** - Add "RECOMMENDED" safety note to all editing tools

### 🟢 Low Priority (Nice to Have)
6. **Default value format** - Standardize on consistent style
7. **Tool description enhancement** - Consider enhancing FindLogicTool description

---

## Next Steps

1. Review this audit with user
2. Prioritize fixes (high → medium → low)
3. Implement fixes systematically (one tool at a time)
4. Update tests if needed
5. Verify consistency with final pass

---

## Parameter Reference Table

| Parameter | Tools Using It | Default Function | Default Value | Consistent? |
|-----------|----------------|------------------|---------------|-------------|
| `workspace` | 6 tools | `default_workspace()` | `Some("primary")` | ⚠️ Docs vary |
| `file_path` | 4 tools | N/A (required) | N/A | ⚠️ Examples vary |
| `dry_run` | 4 tools | N/A (default attr) | `false` | ⚠️ Docs vary |
| `symbol` | 3 tools | N/A (required) | N/A | ⚠️ Detail varies |
| `limit` | 3 tools | `default_limit()` | Various | ✅ Good |
| `max_depth` | 2 tools | `default_max_depth()` | `1` or `3` | ✅ Good |

---

*End of Audit Report*
