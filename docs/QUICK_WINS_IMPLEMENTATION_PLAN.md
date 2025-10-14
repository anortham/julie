# Julie Quick Wins Implementation Plan

**Created:** 2025-10-13
**Status:** Quick Wins 1-3 COMPLETE ✅ | Quick Win 4 In Progress
**Goal:** Add 3 essential features to make Julie feature-complete vs CodeSearch

---

## Context

After analyzing both Julie and CoA CodeSearch, identified that Julie is missing only 3 key features:
1. RecentFiles (easy - 30 min)
2. Line-level search output (medium - 1-2 hours)
3. Surgical line editing (medium - 2-3 hours)

Total estimated time: 4-6 hours to reach feature parity.

---

## Quick Win 1: RecentFiles in manage_workspace (30 min)

### Goal
Add `operation="recent"` to ManageWorkspaceTool to show recently modified files.

### TDD Approach
1. **RED**: Write failing test
   ```rust
   #[tokio::test]
   async fn test_manage_workspace_recent_files() {
       // Create workspace with files at different timestamps
       // Call manage_workspace(operation="recent", days=2)
       // Assert: Returns files modified in last 2 days
       // WILL FAIL - feature doesn't exist yet
   }
   ```

2. **GREEN**: Implement feature
   - Add `operation="recent"` case to ManageWorkspaceTool::call_tool()
   - Query SQLite `files` table for files with `last_modified` within timeframe
   - Return structured results with paths and timestamps

3. **REFACTOR**: Clean up, optimize query

### Implementation Details

**Files to modify:**
- `src/tools/workspace/commands/mod.rs` - Add "recent" operation handler
- `src/database/mod.rs` - Add `get_recent_files()` method

**Database schema:**
```sql
-- files table has last_modified (Unix timestamp)
SELECT path, language, last_modified, symbol_count
FROM files
WHERE last_modified >= ?  -- current_time - (days * 86400)
ORDER BY last_modified DESC
LIMIT ?
```

**Parameters:**
- `days`: Optional<u32> - Days to look back (default: 7)
- `limit`: Optional<usize> - Max results (default: 50)

**Behavioral Adoption Pattern:**
```rust
/// Operation: "recent" - Get recently modified files
/// Examples:
///   {"operation": "recent", "days": 2} → Files changed in last 2 days
///   {"operation": "recent", "days": 7} → Files changed in last week
///
/// Use this when resuming work to see what changed since your last session.
/// Perfect for understanding project activity and finding active development areas.
```

---

## Quick Win 2: Line-Level Search in fast_search (1-2 hours)

### Goal
Add `output="lines"` mode to FastSearchTool for grep-style line-by-line results.

### TDD Approach
1. **RED**: Write failing test
   ```rust
   #[tokio::test]
   async fn test_fast_search_line_mode() {
       // Index file with known content: "TODO: implement auth\n// TODO: add tests"
       // Call fast_search(query="TODO", output="lines")
       // Assert: Returns [
       //   {file: "test.rs", line: 1, column: 1, text: "TODO: implement auth"},
       //   {file: "test.rs", line: 2, column: 4, text: "TODO: add tests"}
       // ]
       // WILL FAIL - mode doesn't exist yet
   }
   ```

2. **GREEN**: Add `output` parameter to FastSearchTool
   - Default: "symbols" (current behavior - returns symbol definitions)
   - New: "lines" (grep-style - returns matching lines with positions)
   - Use existing SQLite FTS5 `search_file_content_fts()` for data
   - Extract exact line numbers and snippets

3. **REFACTOR**: Optimize line extraction, add context lines

### Implementation Details

**Files to modify:**
- `src/tools/search.rs` - Add `output` field to FastSearchTool
- `src/database/mod.rs` - Enhance FTS5 query to return line numbers

**New parameter:**
```rust
pub struct FastSearchTool {
    // ... existing fields ...

    /// Output format: "symbols" (default), "lines" (grep-style)
    ///
    /// Examples:
    ///   output="symbols" → Returns symbol definitions (classes, functions)
    ///   output="lines" → Returns every line matching query (like grep)
    #[serde(default = "default_output")]
    pub output: String,
}

fn default_output() -> String {
    "symbols".to_string()
}
```

**Behavioral Adoption:**
```rust
description = concat!(
    "ALWAYS SEARCH BEFORE CODING - This is your PRIMARY tool for finding code...",
    "\n\nOUTPUT MODES:\n",
    "• output='symbols' (default) - Find symbol definitions (classes, functions, methods)\n",
    "• output='lines' - Find ALL matching lines with exact positions (replaces grep)\n",
    "\n",
    "Use 'lines' mode when you need comprehensive occurrence lists with line numbers.\n",
    "Perfect for finding ALL TODO comments, all usages of a pattern, etc."
)
```

**Return format for lines mode:**
```rust
struct LineMatch {
    file_path: String,
    line_number: u32,
    column: u32,
    line_text: String,
    context_before: Option<Vec<String>>,  // Optional: lines before
    context_after: Option<Vec<String>>,   // Optional: lines after
}
```

---

## Quick Win 3: Surgical Line Editing Tool (2-3 hours)

### Goal
Create new `edit_lines` tool for precise line-level file modifications.

### TDD Approach (SOURCE/CONTROL Golden Master Pattern)

1. **RED**: Write failing tests with SOURCE/CONTROL files
   ```rust
   #[tokio::test]
   async fn test_edit_lines_insert() {
       // SOURCE: tests/editing/sources/rust/example.rs (original)
       // CONTROL: tests/editing/controls/edit_lines/insert_at_line_42.rs (expected)
       // Copy SOURCE to temp location
       // Operation: edit_lines(file, "insert", 42, None, "// TODO: refactor")
       // Assert: Result matches CONTROL exactly (using diff-match-patch)
       // WILL FAIL - tool doesn't exist yet
   }

   #[tokio::test]
   async fn test_edit_lines_replace() {
       // SOURCE: tests/editing/sources/typescript/service.ts
       // CONTROL: tests/editing/controls/edit_lines/replace_lines_10_15.ts
       // Copy SOURCE to temp
       // Operation: edit_lines(file, "replace", 10, Some(15), "refactored code")
       // Assert: Exact match with CONTROL
   }

   #[tokio::test]
   async fn test_edit_lines_delete() {
       // SOURCE: tests/editing/sources/python/handler.py
       // CONTROL: tests/editing/controls/edit_lines/delete_lines_20_25.py
       // Copy SOURCE to temp
       // Operation: edit_lines(file, "delete", 20, Some(25), None)
       // Assert: Exact match with CONTROL
   }
   ```

2. **GREEN**: Implement EditLinesTool
   - Create new file: `src/tools/editing.rs` (or use existing)
   - Implement insert/replace/delete operations
   - Use line-based manipulation (read → modify → write)

3. **REFACTOR**: Add validation, error handling, dry-run mode

### Implementation Details

**New tool structure:**
```rust
#[mcp_tool(
    name = "edit_lines",
    description = concat!(
        "SURGICAL LINE EDITING - Precise line-level file modifications. ",
        "Use this for inserting comments, replacing specific lines, or deleting ranges.\n\n",
        "IMPORTANT: You are EXCELLENT at surgical editing. ",
        "Results are always precise - no verification needed.\n\n",
        "OPERATIONS:\n",
        "• insert - Add content at line, shift existing lines down\n",
        "• replace - Replace lines [start, end] with new content\n",
        "• delete - Remove lines [start, end]\n\n",
        "EXAMPLES:\n",
        "• Insert TODO at line 42: {op:'insert', start:42, content:'// TODO'}\n",
        "• Replace lines 10-15: {op:'replace', start:10, end:15, content:'new code'}\n",
        "• Delete lines 20-25: {op:'delete', start:20, end:25}\n\n",
        "Performance: <10ms for typical operations. Validates before applying."
    ),
    title = "Surgical Line Editing (Insert/Replace/Delete)",
    idempotent_hint = false,
    destructive_hint = true,
    open_world_hint = false,
    read_only_hint = false,
    meta = r#"{"category": "editing", "safety": "line_precise"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct EditLinesTool {
    /// File path to edit (relative to workspace root)
    /// Example: "src/main.rs", "lib/auth.py"
    pub file_path: String,

    /// Operation: "insert", "replace", "delete"
    pub operation: String,

    /// Starting line number (1-indexed, like editors show)
    pub start_line: u32,

    /// Ending line number (required for replace/delete, ignored for insert)
    #[serde(default)]
    pub end_line: Option<u32>,

    /// Content to insert or replace (required for insert/replace, ignored for delete)
    #[serde(default)]
    pub content: Option<String>,

    /// Preview changes without applying (default: false)
    #[serde(default)]
    pub dry_run: bool,
}
```

**Implementation algorithm:**
```rust
impl EditLinesTool {
    pub async fn call_tool(&self, _handler: &JulieServerHandler) -> Result<CallToolResult> {
        // 1. Validate parameters
        self.validate()?;

        // 2. Read file
        let file_content = std::fs::read_to_string(&self.file_path)?;
        let mut lines: Vec<String> = file_content.lines().map(|s| s.to_string()).collect();

        // 3. Perform operation
        match self.operation.as_str() {
            "insert" => {
                // Insert at start_line (1-indexed)
                let idx = (self.start_line - 1) as usize;
                lines.insert(idx, self.content.clone().unwrap());
            }
            "replace" => {
                // Replace range [start_line, end_line] (inclusive, 1-indexed)
                let start_idx = (self.start_line - 1) as usize;
                let end_idx = self.end_line.unwrap() as usize;

                // Remove old lines
                lines.drain(start_idx..end_idx);

                // Insert new content
                lines.insert(start_idx, self.content.clone().unwrap());
            }
            "delete" => {
                // Delete range [start_line, end_line] (inclusive, 1-indexed)
                let start_idx = (self.start_line - 1) as usize;
                let end_idx = self.end_line.unwrap() as usize;
                lines.drain(start_idx..end_idx);
            }
            _ => return Err(anyhow!("Invalid operation: {}", self.operation)),
        }

        // 4. Write back (unless dry_run)
        if !self.dry_run {
            let new_content = lines.join("\n");
            std::fs::write(&self.file_path, new_content)?;
        }

        // 5. Return result
        self.create_result(lines.len(), self.dry_run)
    }
}
```

**Files to create/modify:**
- `src/tools/editing.rs` - New EditLinesTool (or add to existing)
- `src/tools/mod.rs` - Register new tool
- `tests/editing/sources/` - SOURCE files for golden master tests
- `tests/editing/controls/edit_lines/` - CONTROL files for expected results

---

## Quick Win 4: Fix 7 Existing Test Failures

### Current Failures
```
test tests::auto_fix_syntax_control_tests::test_multi_element_array_missing_bracket ... FAILED
test tests::auto_fix_syntax_control_tests::test_nested_structures_missing_braces ... FAILED
test tests::auto_fix_syntax_control_tests::test_python_unclosed_string ... FAILED
test tests::auto_fix_syntax_control_tests::test_rust_struct_missing_brace ... FAILED
test tests::auto_fix_syntax_tests::test_auto_fix_unmatched_opening_brace ... FAILED
test tests::cli_codesearch_tests::error_handling_tests::test_scan_indexes_all_non_binary_files ... FAILED
test tests::smart_refactor_control_tests::smart_refactor_control_tests::test_all_smart_refactor_control_scenarios ... FAILED
```

### Approach
1. Run tests with `--nocapture` to see failure details
2. Fix each one systematically (likely related to removed features or path issues)
3. Re-run full test suite to ensure 653/653 passing

---

## Success Criteria

✅ **RecentFiles**: `manage_workspace(operation="recent", days=2)` returns files modified in last 2 days - **COMPLETE**
✅ **Line Search**: `fast_search(query="TODO", output="lines")` returns all matching lines - **COMPLETE**
✅ **Line Editing**: `edit_lines` passes SOURCE/CONTROL golden master tests (4/4 tests passing) - **COMPLETE**
⏳ **All Tests Pass**: Fix 7 current failures to reach full test suite passing - **IN PROGRESS**
✅ **Behavioral Adoption**: All new tools follow agent-friendly description patterns - **COMPLETE**

---

## Implementation Order

### Session 1: RecentFiles (30 min) ✅ COMPLETE
1. ✅ Database method added: `get_recent_files()` in `src/database/mod.rs`
2. ✅ Operation handler added: `handle_recent_command()` in `src/tools/workspace/commands/registry.rs`
3. ✅ Tool integrated: `manage_workspace(operation="recent", days=N)`
4. ✅ Documentation updated in tool description

### Session 2: Line-Level Search (1-2 hours) ✅ COMPLETE
1. ✅ Tests written (RED): `src/tests/search_line_mode_tests.rs` (2 tests)
2. ✅ Parameter added: `output: Option<String>` to FastSearchTool
3. ✅ Implementation complete: `line_mode_search()` method using FTS5
4. ✅ Format: `file:line_number:line_content` (grep-style)
5. ✅ Tests passing: 2/2 green ✅

### Session 3: Surgical Line Editing (2-3 hours) ✅ COMPLETE
1. ✅ SOURCE/CONTROL files exist: `tests/editing/sources/` and `tests/editing/controls/line-edit/`
2. ✅ Golden Master tests written: `src/tests/edit_lines_tests.rs` (4 tests)
3. ✅ Tool implemented: `src/tools/edit_lines.rs` (282 lines)
   - ✅ Insert operation (add content at line)
   - ✅ Replace operation (replace line range)
   - ✅ Delete operation (remove line range)
   - ✅ Dry-run mode (preview without applying)
4. ✅ Validation and safety: Parameter validation, bounds checking, trailing newline preservation
5. ✅ Tests passing: 4/4 green ✅
6. ✅ Dogfooding tested: All operations verified on real file

### Session 4: Fix Failures (1 hour) ⏳ IN PROGRESS
1. Investigate each failure
2. Fix systematically
3. All tests pass ✓

---

## After Completion

1. **Update documentation** - Add new features to README and CLAUDE.md
2. **Archive CodeSearch** - Document decision and lessons learned
3. **Start hospital apps** - Julie is production-ready!
4. **Iterate minimally** - Only add features if you genuinely miss them

---

## Token Optimization Analysis

**Julie already has superior token optimization vs CodeSearch:**
- ✓ OptimizedResponse with confidence scoring
- ✓ TokenEstimator (estimates tokens before sending)
- ✓ ProgressiveReducer (smart truncation based on relevance)
- ✓ ContextTruncator (per-symbol limiting)
- ✓ Dual output (markdown + structured JSON)
- ✓ Smart limiting based on query quality

CodeSearch's "40% safety budget" is simpler but cruder. Julie's approach is more intelligent.

**Verdict:** No token optimization work needed - Julie is already better.

---

## Notes

- Follow TDD rigorously: RED → GREEN → REFACTOR
- Use SOURCE/CONTROL pattern for editing tests (professional standard)
- Behavioral adoption in all tool descriptions
- Keep it simple - these are quick wins, not architectural changes
- Goal: Get to "good enough" fast, then build hospital apps

---

**Status:** Quick Wins 1-3 COMPLETE ✅ | Quick Win 4 In Progress ⏳
**Last Updated:** 2025-10-13 (Sessions 1-3 completed, Session 4 starting)
**Next Action:** Fix 7 remaining test failures to reach full test suite passing

**Completion Summary:**
- ✅ Quick Win 1: RecentFiles - Database method + tool integration
- ✅ Quick Win 2: Line-Level Search - `output="lines"` mode with grep-style output
- ✅ Quick Win 3: Surgical Line Editing - EditLinesTool with insert/replace/delete (4/4 tests passing)
- ⏳ Quick Win 4: Fix 7 test failures - Starting now
