# Testing Guide

**Last Updated:** 2025-11-07

Complete guide to Julie's testing methodology and standards.

## Test Coverage Requirements

- **Extractors**: 100% comprehensive test coverage
- **Editing Tools**: 90% coverage with SOURCE/CONTROL methodology
- **Core Logic**: >80% coverage on search and navigation
- **MCP Tools**: Full integration testing
- **Cross-platform**: Automated testing on Windows, macOS, Linux

## SOURCE/CONTROL Testing Methodology

**Critical Pattern for All File Modification Tools:**

1. **SOURCE files** - Original files that are NEVER modified
2. **CONTROL files** - Expected results after specific operations
3. **Test Process**: SOURCE → copy → edit → diff against CONTROL
4. **Verification**: Must match exactly using diff-match-patch

**Example Structure:**
```rust
struct EditingTestCase {
    name: &'static str,
    source_file: &'static str,    // Never modified
    control_file: &'static str,   // Expected result
    operation: &'static str,
    // ... operation parameters
}
```

**Implemented For:**
- ✅ FuzzyReplaceTool (18 unit tests)
- ✅ TraceCallPathTool (15 unit tests)
- ✅ RenameSymbolTool
- ✅ EditSymbolTool

## Running Tests

```bash
# All tests
cargo test

# Specific test suites
cargo test deep_dive              # DeepDiveTool unit tests (37 tests)
cargo test definition_promotion   # fast_search definition promotion tests (6 tests)
cargo test typescript_extractor   # Language extractor tests

# With output
cargo test -- --nocapture

# Coverage analysis
cargo tarpaulin

# Performance tests
cargo test --release
```

## Test Performance Strategy

**Fast Tests (default):**
- Unit tests and focused integration tests
- Run in <10 seconds total
- Execute with: `cargo test`
- Used during active development

**Slow Dogfooding Tests (ignored by default):**
- Real-world validation against Julie's own codebase
- Index entire workspace and run complex queries
- Take 60+ seconds each (16 tests total)
- Located in: `src/tests/tools/search_quality/dogfood_tests.rs`

**Running Slow Tests:**
```bash
# Run ONLY slow/ignored tests (for search quality validation)
cargo test --lib -- --ignored

# Run ALL tests (fast + slow) before releases
cargo test --lib -- --include-ignored
```

**When to Run Dogfooding Tests:**
- Before major releases
- After significant search/ranking changes
- After modifying Tantivy tokenization or query logic
- When validating search architecture changes
- Weekly regression checks

## Code Coverage Tooling

**Configuration**: `tarpaulin.toml`
- General threshold: 80%
- Editing tools threshold: 90% (critical for safety)
- Coverage reports: HTML, LCOV, JSON formats

**Commands:**
```bash
# Run coverage analysis
cargo tarpaulin

# Generate detailed HTML report
cargo tarpaulin --output-dir target/tarpaulin --output-format Html

# Check specific module coverage
cargo tarpaulin --include src/tools/editing.rs
```
