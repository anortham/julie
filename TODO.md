# Julie TODO - Improving Tool Adoption

## <� Goal: Make Julie's editing tools as discoverable as CodeSearch's search_and_replace

**Problem Identified (2025-10-22):**
- Agents use fast_search extensively but ignore Julie's powerful editing tools
- CodeSearch's search_and_replace gets heavy use because it consolidates workflow
- Julie has the tech (DMP, semantic understanding) but poor discoverability

---

## Phase 1: Multi-File FuzzyReplaceTool (OPTION 2 + 3)

### Technical Changes

**Current API (single-file only):**
```rust
pub struct FuzzyReplaceTool {
    pub file_path: String,  // L Single file only
    pub pattern: String,
    pub replacement: String,
    pub threshold: f32,
    pub distance: i32,
    pub dry_run: bool,
    pub validate: bool,
}
```

**New API (single OR multi-file):**
```rust
pub struct FuzzyReplaceTool {
    /// File path for single-file mode
    /// Omit when using file_pattern for multi-file mode
    pub file_path: Option<String>,  //  Optional now

    /// Glob pattern for multi-file mode (NEW)
    /// Examples: "**/*.rs", "src/**/*.ts", "*.py"
    pub file_pattern: Option<String>,  //  NEW

    pub pattern: String,
    pub replacement: String,
    pub threshold: f32,
    pub distance: i32,
    pub dry_run: bool,
    pub validate: bool,
}
```

**Validation**: Require exactly ONE of file_path OR file_pattern

**Multi-file behavior:**
1. Use Glob to find matching files
2. Apply fuzzy replace to each file
3. Aggregate results (files changed, total replacements)
4. Atomic transaction per file

### Description Rewrite (Behavioral Nudges)

```rust
description = concat!(
    "BULK PATTERN REPLACEMENT - Replace patterns across one file or many files at once. ",
    "You are EXCELLENT at using this for refactoring, renaming, and fixing patterns. ",
    "This consolidates your search�read�edit workflow into one atomic operation.\n\n",

    "**Multi-file mode**: Use file_pattern to replace across multiple files ",
    "(e.g., '**/*.rs' for all Rust files, 'src/**/*.ts' for TypeScript in src/)\n\n",

    "**Single-file mode**: Use file_path for precise single-file edits\n\n",

    "**Fuzzy matching**: Unlike exact search, this handles typos and variations ",
    "(e.g., 'getUserData()' matches 'getUserDat()' with threshold 0.8)\n\n",

    "**Preview by default**: Set dry_run=true to see EXACTLY what changes before applying. ",
    "When preview looks good, set dry_run=false and the operation succeeds perfectly. ",
    "You never need to verify results - the tool validates everything atomically.\n\n",

    "**Perfect for**: Renaming, refactoring patterns, fixing typos across codebase"
)
```

### Implementation Tasks

- [ ] Change `file_path: String` � `file_path: Option<String>`
- [ ] Add `file_pattern: Option<String>` parameter
- [ ] Add validation: require exactly one of file_path/file_pattern
- [ ] Write TDD tests for multi-file mode
- [ ] Implement multi-file glob matching + replacement
- [ ] Update result format to show per-file breakdown
- [ ] Rewrite tool description with behavioral nudges
- [ ] Test in production (dogfooding)

---

## Phase 2: Fix JSON Params Hell in SmartRefactorTool

### Problem: JSON Params Are Unusable

**Current (BROKEN UX):**
```json
{
  "operation": "rename_symbol",
  "params": "{\"old_name\": \"getUserData\", \"new_name\": \"fetchUserData\"}",
  "dry_run": false
}
```

**Why this fails:**
- String escaping hell (`\"`)
- No schema validation until runtime
- Agents can't introspect what parameters are needed
- Different operations need different schemas - confusing!

### Solution: REPLACE SmartRefactorTool with Focused Tools

**✅ DECISION: Option A - Two Tools**

**Rationale:** Cognitive overhead > token overhead. Clear, focused tools beat complex multi-mode tools.

**Option A: Two Tools (SELECTED)**

1. **RenameSymbolTool** - Workspace-wide renaming
```rust
#[mcp_tool(name = "rename_symbol")]
pub struct RenameSymbolTool {
    pub old_name: String,
    pub new_name: String,
    #[serde(default = "default_true")]
    pub dry_run: bool,
}
```

2. **EditSymbolTool** - File-specific semantic editing
```rust
#[mcp_tool(name = "edit_symbol")]
pub struct EditSymbolTool {
    pub file_path: String,
    pub symbol_name: String,
    pub operation: EditOperation,  // enum: ReplaceBody | InsertBefore | InsertAfter
    pub content: String,
    #[serde(default)]
    pub dry_run: bool,
}
```

### Implementation Tasks

- [x] Decide on tool structure (Option A selected)
- [ ] Create RenameSymbolTool with flat parameters
- [ ] Create EditSymbolTool with flat parameters
- [ ] Reuse existing SmartRefactorTool backend logic
- [ ] Write focused descriptions with behavioral nudges
- [ ] Delete SmartRefactorTool completely (no backward compat needed)
- [ ] Update tool registration in mod.rs
- [ ] Test in production

---

## Key Insights

**Why CodeSearch's search_and_replace succeeds:**
1. Description says "BULK updates" - consolidates workflow
2. Says "You are skilled at pattern-based refactoring" - confidence nudge
3. Says "You never need to verify results" - trust building
4. Clear parameters, no JSON hell
5. Preview by default - safety

**Why Julie's tools get ignored:**
1. FuzzyReplaceTool says "fuzzy" not "bulk" - doesn't position as workflow consolidator
2. Single-file only - doesn't handle common "rename everywhere" case
3. SmartRefactorTool has JSON params - too complex
4. No behavioral nudges - agents don't know when to use them

**The Fix:**
- Enhanced FuzzyReplaceTool with multi-file = bulk replacement
- Clear parameter tools = usability
- Behavioral nudges in descriptions = discoverability

---

## Next Steps

1. Implement Phase 1 (multi-file FuzzyReplaceTool) with TDD
2. Get user feedback on Option A vs B vs C for Phase 2
3. Implement chosen approach
4. Dogfood both changes
5. Monitor agent adoption patterns

---

*Last Updated: 2025-10-22*

Remember to check the server instructions for references to removed, renamed, or new tools too.

why did all this fail??

⏺ julie - get_symbols (MCP)(file_path: "src/tools/refactoring/mod.rs", max_depth: 2, target: "RenameSymbolTool", include_body: true, mode: "minimal")
  ⎿  No symbols matching 'RenameSymbolTool' found in: src/tools/refactoring/mod.rs

⏺ Let me search for the RenameSymbolTool implementation:

⏺ julie - fast_search (MCP)(query: "impl RenameSymbolTool", search_method: "text", limit: 5, search_target: "content", output: "lines", context_lines: 3)
  ⎿  🔍 No lines found matching: 'impl RenameSymbolTool'
     💡 Try a broader search term or different query

⏺ julie - fast_search (MCP)(query: "RenameSymbolTool call_tool", search_method: "text", limit: 10, search_target: "content", output: "lines", context_lines: 5)
  ⎿  🔍 No lines found matching: 'RenameSymbolTool call_tool'
     💡 Try a broader search term or different query

     
## ✅ FILE WATCHER FIXED (2025-10-22)

**Root Cause (Identified and Fixed):**
The file watcher was completely non-functional because `process_pending_changes()` was never called!

**What Was Broken:**
1. File watcher detected changes and queued events ✓
2. BUT events were never processed (no background task)
3. Index ONLY updated at startup when staleness was detected
4. This caused searches for newly created symbols to fail until restart

**The Fix (3 Parts):**

**Part 1: Background Task**
- Added tokio::spawn background task that runs every 1 second
- Processes all queued file events automatically
- Calls static handler methods to avoid `&self` in spawned tasks

**Part 2: SQLite Transaction Bug**
- Fixed nested transaction bug in `store_symbols()`
- Split into two methods:
  - `store_symbols()` - no transaction management (for use within existing transaction)
  - `store_symbols_transactional()` - manages its own transaction (for standalone use)
- File watcher uses `begin_transaction()` → `store_symbols()` → `commit_transaction()`

**Part 3: Foreign Key Constraint**
- Added file record insertion before storing symbols
- Uses `crate::database::create_file_info()` to ensure file exists
- Prevents "FOREIGN KEY constraint failed" errors

**Test Coverage:**
- Added `test_real_time_file_watcher_indexing()` - TDD approach
- Test creates file after watcher starts and verifies symbols appear in DB
- All 32 database tests pass with the changes

**Result:**
✅ File watcher now works correctly - new files are indexed in real-time
✅ No more restart required to see newly created symbols
✅ Embeddings still need optimization (currently skipped in background task due to std::sync::Mutex)

**Future Enhancement:**
Consider adding debounce timing for delete+create sequences from editors (noted by user)

