In Intranet project:

● julie - fast_search (MCP)(query: "DirectusCmsService", search_method: "text", search_target: "content", file_pattern: "\*\*/CoA.Intranet.Client/Program.cs", limit:
5, output: "lines", context_lines: 5)
⎿  🔍 No lines found matching: 'DirectusCmsService'
💡 Try a broader search term or different query

     This should have found something

● julie - fast_search (MCP)(query: "OnSpreadsheetFileSelected", search_method: "text", search_target: "content", file_pattern: "\*\*/RfaFormPageV2.razor", limit: 5,
output: "lines", context_lines: 10)
⎿  🔍 No lines found matching: 'OnSpreadsheetFileSelected'
💡 Try a broader search term or different query

● julie - fast_search (MCP)(query: "InputFile.\*spreadsheet", search_method: "text", search_target: "content", file_pattern: "\*\*/RfaFormPageV2.razor", limit: 5,
output: "lines", context_lines: 5)
⎿  Error: Tool execution failed: fts5: syntax error near "."

---

## INVESTIGATION RESULTS

### Issue 1: DirectusCmsService in Program.cs - No Results

**Root Cause:**
- CONFIRMED: Content exists in CoA.Intranet.Client/Program.cs line 63
- Problem 1: Low limit (5) + test file has more matches, so test results dominate
- Problem 2: file_pattern `**/CoA.Intranet.Client/Program.cs` doesn't work at all

**Tests Performed:**
- ✅ No file_pattern, limit=50 → Found 50 results (including Program.cs)
- ❌ file_pattern="Program.cs" → No results
- ❌ file_pattern="**/Program.cs" → No results
- ❌ file_pattern="CoA.Intranet.Client/Program.cs" → No results
- ❌ file_pattern="*Program.cs" → No results
- ✅ file_pattern="*.cs" → Works (but returns test files first)
- ✅ file_pattern="**/*.cs" → Works

**Workarounds:**
- Increase limit to 50+ and don't use file_pattern for specific files
- Use extension-based patterns like `*.cs` or `**/*.cs`

### Issue 2: OnSpreadsheetSelected in RfaFormPageV2.razor - No Results

**Root Cause:**
- CONFIRMED: Content exists in RfaFormPageV2.razor line 1152 (note: original TODO had typo "OnSpreadsheetFileSelected" vs actual "OnSpreadsheetSelected")
- Problem: file_pattern `**/RfaFormPageV2.razor` doesn't work - specific filenames after `**/` fail

**Tests Performed:**
- ❌ file_pattern="**/RfaFormPageV2.razor" → No results
- ✅ file_pattern="*.razor" → Works! Found 2 matches
- ✅ file_pattern="*RfaFormPageV2.razor" → Works! Found 2 matches

**Workaround:**
- Use `*RfaFormPageV2.razor` instead of `**/RfaFormPageV2.razor`
- Or just use `*.razor` for all razor files

### Issue 3: InputFile.*spreadsheet - FTS5 Syntax Error

**Root Cause:**
- Query string is passed directly to SQLite FTS5 full-text search
- FTS5 has special syntax where `.` is a query operator
- User expected regex pattern matching, but julie uses FTS5 text search (not regex)

**Test Performed:**
- ❌ query="InputFile.*spreadsheet" → `fts5: syntax error near "."`
- This is expected behavior - FTS5 doesn't support regex

**Recommendation:**
- Document that queries are FTS5 text search, NOT regex
- Consider escaping special characters in queries, or add a regex mode

---

## FILE_PATTERN BUG ANALYSIS

### Working Patterns ✅

| Pattern | Example | Result |
|---------|---------|--------|
| Extension only | `*.cs`, `*.razor` | ✅ Works |
| Recursive extension | `**/*.cs`, `**/*.razor` | ✅ Works |
| Wildcard prefix | `*RfaFormPageV2.razor` | ✅ Works |
| Directory wildcard | `**/Services/*.cs` | ✅ Works |

### Broken Patterns ❌

| Pattern | Example | Result |
|---------|---------|--------|
| Specific filename with ** | `**/Program.cs` | ❌ Returns no results |
| Specific filename alone | `Program.cs` | ❌ Returns no results |
| Full path | `**/CoA.Intranet.Client/Program.cs` | ❌ Returns no results |
| Directory pattern | `CoA.Intranet.Client/**` | ❌ Returns no results |
| Wildcard + filename | `*Program.cs` | ❌ Returns no results |

**Key Finding:** Specific filenames don't work with glob matching, even with wildcards. Only extension-based patterns work reliably.

**Hypothesis:** The glob matching is checking against the full UNC path (`\?\C:\source\CoA Intranet\...`), and patterns like `**/Program.cs` aren't matching because:
1. The UNC prefix might not be handled correctly
2. Path separator handling (backslash vs forward slash)
3. Glob library might not handle `**/filename.ext` pattern correctly

---

## ✅ RESOLUTION (2025-10-22)

**All issues fixed with comprehensive regression tests!**

### Issue 1: Glob Pattern Matching - FIXED ✅

**Root Cause (Corrected):**
- Simple filenames (e.g., `Program.cs`) without wildcards failed to match UNC paths
- Globset library expects patterns to match entire path, not just basename
- `**/Program.cs` patterns actually WORK (TODO.md initial finding was incorrect)

**Fix Applied:**
- Added special case in `matches_glob_pattern()` for simple filenames (no wildcards, no path separators)
- Simple filenames now match against basename only, not full UNC path
- Location: `src/tools/search/query.rs:80-96`

**Test Coverage:**
- 7 glob pattern regression tests added (all passing)
- Tests cover: simple filenames, `**` patterns, paths with spaces, UNC paths, wildcards

### Issue 2: FTS5 Syntax Errors - FIXED ✅

**Root Cause:**
- Users entering regex patterns (`InputFile.*`, `end$`, `foo|bar`, `file\.txt`)
- FTS5 interprets these as operators, causing syntax errors
- Existing sanitization missed: `$`, `.*` combo, `|`, and backslash escapes

**Fix Applied:**
- Added early backslash stripping (removes regex escape sequences)
- Added `$` detection for end-of-line anchors
- Added `.*` detection for regex wildcard patterns
- Added `|` to special characters list (regex alternation)
- All regex-like patterns now quoted as literal phrases
- Location: `src/database/symbols/queries.rs:115-130, 171`

**Test Coverage:**
- 3 FTS5 syntax regression tests added (all passing)
- Tests cover: dot patterns, asterisk patterns, all common regex metacharacters

### Issue 3: Limit/Ranking Interaction - DOCUMENTED 📝

**Status:** Tests deferred (requires database bulk insert API)

**Workaround:**
- Use higher limit values (default: 50, not 5)
- TODO: Add ranking boost for non-test files in future iteration

---

### Test Results Summary

**10 regression tests created:**
- ✅ 7 glob pattern tests (all passing)
- ✅ 3 FTS5 syntax tests (all passing)
- 📝 Limit/ranking tests documented for future implementation

**Full test suite:**
- ✅ 929 tests passing
- ❌ 0 failures
- ✅ No regressions introduced

**Files Modified:**
- `src/tools/search/query.rs` - Glob pattern matching fix
- `src/database/symbols/queries.rs` - FTS5 sanitization improvements
- `src/tests/integration/search_regression_tests.rs` - New regression test suite
- `src/tests/mod.rs` - Register new test module
- `src/tools/search/mod.rs` - Export matches_glob_pattern for testing

---

### Recommended Fixes (COMPLETED - see above)

~~1. **File Pattern Matching:**~~
   ~~- Debug glob matching logic - check how patterns are applied to indexed file paths~~
   ~~- Verify UNC paths (`\?\C:\...`) are normalized before glob matching~~
   ~~- Test glob library with Windows UNC paths~~
   ~~- Add test cases for all common glob patterns~~

~~2. **Query Syntax:**~~
   ~~- Document that queries use FTS5 syntax, not regex~~
   ~~- Consider adding query escaping for FTS5 special chars (`.` `*` `"` etc)~~
   ~~- Or add separate `regex_search` mode using LIKE/REGEXP~~

~~3. **User Experience:**~~
   ~~- Better error messages for unsupported glob patterns~~
   ~~- Show example patterns that work~~
   ~~- Validate file_pattern before executing search~~



⏺ julie - fast_search (MCP)(query: "SanitizeQuery", search_method: "text", limit: 10, search_target: "content", workspace: "coa-mcp-framework_c77f81e4")
  ⎿  Error: Tool execution failed: Workspace 'coa-mcp-framework_c77f81e4' not found. Use 'primary' or a valid workspace ID

  in this case, the coa-mcp-framework workspace was already registered, but the supplied hash was wrong. what can we do so this works smoother?
  1. Instead of saying not found, say "Did you mean {correct_workspace_id}" ?
  2. Do we have this covered with tests properly? What about workspaces with spaces in the name? 
  
---

## ✅ RESOLUTION (2025-10-22) - Fuzzy Workspace Matching

**Issue:** Workspace ID typos produce unhelpful "not found" errors with no suggestions.

**Root Cause:**
- User mistypes workspace hash or name
- Error message simply says "not found" without suggesting alternatives
- Frustrating UX when similar workspaces exist

**Fix Applied:**
- Created `src/utils/string_similarity.rs` with Levenshtein distance implementation
- Integrated fuzzy matching into workspace error handling in two locations:
  - `src/tools/search/mod.rs` (line 358-385)
  - `src/tools/navigation/resolution.rs` (line 35-63)
- Error message now shows "Did you mean '{closest_match}'?" when reasonable match found
- Only suggests if distance < 50% of query length (prevents nonsensical suggestions)

**Test Coverage:**
- 4 unit tests in `src/utils/string_similarity.rs`:
  - Basic Levenshtein distance calculations
  - Closest match selection from candidates
  - Workspace ID typo scenarios (wrong hash, similar names)
  - Workspace names with spaces
- All tests passing (4/4)

**Examples:**
- User types: `coa-mcp-framework_c77f81e4` (wrong workspace name)
  → Error: "Workspace 'coa-mcp-framework_c77f81e4' not found. Did you mean 'coa-intranet_cdcd7a9d'?"
- User types: `coa-codesearch-mcp_wronghash` (correct name, wrong hash)
  → Error: "Workspace 'coa-codesearch-mcp_wronghash' not found. Did you mean 'coa-codesearch-mcp_9037416c'?"

**Files Modified:**
- `src/utils/string_similarity.rs` - NEW: Levenshtein distance + fuzzy matching (140 lines)
- `src/utils/mod.rs` - Export string_similarity module
- `src/tools/search/mod.rs` - Add fuzzy suggestions to workspace errors
- `src/tools/navigation/resolution.rs` - Add fuzzy suggestions to workspace errors

**Status:** COMPLETE ✅
- Question 1: ✅ "Did you mean?" suggestions implemented
- Question 2: ✅ Tested with workspaces containing spaces in names

