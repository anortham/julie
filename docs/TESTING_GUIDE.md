# Testing Guide

**Last Updated:** 2026-03-25

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
3. **Test Process**: SOURCE -> copy -> edit -> diff against CONTROL
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
- FuzzyReplaceTool
- RenameSymbolTool
- EditSymbolTool

## Running Tests

```bash
# Default -- run after EVERY change
cargo xtask test dev

# When touching startup/workspace/system flows
cargo xtask test system

# When changing search/scoring/tokenization
cargo xtask test dogfood

# Broad pre-merge pass
cargo xtask test full

# List all buckets
cargo xtask test list

# Narrow filter for a specific test (use ONLY when debugging a failure)
cargo test --lib test_stemming
cargo test -p julie-extractors typescript_extractor
```

## Test Tiers

| Tier | Command | When to use |
|------|---------|-------------|
| smoke | `cargo xtask test smoke` | Quick sanity check |
| dev | `cargo xtask test dev` | After normal changes (default) |
| system | `cargo xtask test system` | Startup/workspace/system changes |
| dogfood | `cargo xtask test dogfood` | Search/scoring/tokenization changes |
| full | `cargo xtask test full` | Pre-merge broad pass |

## Dogfooding Tests

The `search_quality` bucket loads a real 100MB SQLite fixture, backfills a Tantivy index, and runs real searches. It is a regression guard, not a fast unit-tier pass.

**When to run:**
- After significant search/ranking changes
- After modifying Tantivy tokenization or query logic
- Before major releases

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
```
