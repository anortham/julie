# Disabled SafeEditTool Integration Tests

This directory contains **7 comprehensive integration test modules** for the deprecated `SafeEditTool`.

## 📊 Test Files (3,714 lines total)

1. **editing_safety_tests.rs** (~918 lines) - Critical safety scenarios
   - Concurrency: concurrent edits to same file
   - Permissions: readonly files and directories
   - Encoding: UTF-8 handling, binary file rejection
   - Security: path traversal prevention, symlink handling
   - Performance: large files, long lines

2. **editing_tests.rs** - General editing operations
3. **fast_edit_search_replace_tests.rs** - Pattern-based editing
4. **line_edit_control_tests.rs** - SOURCE/CONTROL methodology for line editing
5. **line_edit_tests.rs** - Line-level editing operations
6. **transactional_editing_tests.rs** - Transactional safety (memory-based, no .backup files)
7. **transactional_integration_tests.rs** - Integration tests for transactional safety

## 🚨 Why Disabled?

SafeEditTool was replaced by:
- **FuzzyReplaceTool** - Pattern matching with fuzzy logic
- **EditLinesTool** - Line-level operations

The old tool had a complex API with multiple modes. The new tools have simpler, focused APIs.

## ✅ Current Test Coverage

**Unit tests exist and pass (24 tests):**
- FuzzyReplaceTool: 18 unit tests (similarity, fuzzy matching, validation)
- EditLinesTool: 6 unit tests (delete, insert, replace, dry run, paths, CRLF)

**Location:** `src/tests/tools/editing/fuzzy_replace.rs` and `edit_lines.rs`

## ❌ Integration Test Gap

**What's missing:** MCP integration tests through JulieServerHandler that test:

### Critical Safety Scenarios (HIGH PRIORITY)
- **Concurrency safety** - Multiple tools editing same file simultaneously
- **Permission handling** - Readonly files, readonly directories
- **UTF-8 safety** - Multi-byte characters, emoji, CJK text
- **Security** - Path traversal attacks, symlink handling

### Performance/Edge Cases (MEDIUM PRIORITY)
- **Large files** - Memory safety with 100KB+ files
- **Long lines** - Lines exceeding 10KB
- **Binary files** - Graceful rejection of non-text files

### Validation (LOWER PRIORITY)
- **Dry run mode** - Preview without modification
- **Validation** - Balance checking, corruption prevention

## 🔧 Migration Strategy

### Phase 1: Extract Reusable Patterns
Create shared test infrastructure from SafeEditTool tests:
- `SafetyTestFixture` - Temp directories, file creation, permissions
- `extract_text_from_result()` - Parse CallToolResult responses
- Permission helpers - make_readonly(), restore_permissions()

### Phase 2: Adapt for New Tools
Convert SafeEditTool test patterns to new tool APIs:

**FuzzyReplaceTool integration tests:**
```rust
// Old: SafeEditTool with mode="pattern_replace"
let tool = SafeEditTool {
    file_path: path,
    mode: "pattern_replace".to_string(),
    find_text: Some("old".to_string()),
    replace_text: Some("new".to_string()),
    // ... 10+ other parameters
};

// New: FuzzyReplaceTool with simple API
let tool = FuzzyReplaceTool {
    file_path: path,
    pattern: "old".to_string(),
    replacement: "new".to_string(),
    threshold: 0.8,
    distance: 1000,
    dry_run: false,
    validate: true,
};
```

**EditLinesTool integration tests:**
```rust
// Old: SafeEditTool with mode="line_replace"
let tool = SafeEditTool {
    file_path: path,
    mode: "line_replace".to_string(),
    start_line: Some(1),
    end_line: Some(3),
    content: Some("new content".to_string()),
    // ... 10+ other parameters
};

// New: EditLinesTool with focused API
let tool = EditLinesTool {
    file_path: path,
    operation: "replace".to_string(),
    start_line: 1,
    end_line: Some(3),
    content: "new content".to_string(),
    dry_run: false,
};
```

### Phase 3: Prioritize Critical Tests
Focus on high-value integration tests:
1. **Concurrency** - Most critical for production safety
2. **Permissions** - Prevents destructive operations
3. **UTF-8** - Data corruption prevention
4. **Security** - Prevents malicious usage

### Phase 4: SOURCE/CONTROL Methodology
Adapt line_edit_control_tests.rs patterns:
- Preserve SOURCE files (never modified)
- Create CONTROL files (expected results)
- Use diff-match-patch for exact verification

## 📝 Implementation Checklist

- [ ] Extract SafetyTestFixture to shared test infrastructure
- [ ] Create FuzzyReplaceTool integration test suite
- [ ] Create EditLinesTool integration test suite
- [ ] Port concurrency tests (HIGH PRIORITY)
- [ ] Port permission tests (HIGH PRIORITY)
- [ ] Port UTF-8 tests (HIGH PRIORITY)
- [ ] Port security tests (HIGH PRIORITY)
- [ ] Port performance tests (MEDIUM PRIORITY)
- [ ] Adapt SOURCE/CONTROL methodology for new tools
- [ ] Achieve 90%+ coverage on editing tools (per tarpaulin.toml)

## 🎯 Success Criteria

Integration tests are complete when:
- ✅ All critical safety scenarios covered
- ✅ Tests run through MCP handler layer (not just unit tests)
- ✅ 90%+ code coverage on FuzzyReplaceTool and EditLinesTool
- ✅ No regressions from SafeEditTool functionality

---

**Status:** Tests preserved for reference, awaiting migration to new tool APIs.
**Created:** 2025-10-16
**Lines of code:** ~3,714 lines (valuable test patterns)
**Estimated migration effort:** 8-12 hours for complete integration test suite
