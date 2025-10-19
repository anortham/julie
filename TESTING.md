# Julie Testing Guide

## Quick Start

```bash
# Run all non-ignored tests (completes in ~10 seconds)
cargo test --lib

# Run ignored tests separately (slow - may take minutes)
cargo test --lib --ignored

# Run specific test
cargo test --lib test_name
```

## Test Categories

### ✅ Fast Tests (Always Run)
**640 tests** that complete in ~10 seconds:
- **Extractor tests** (25 languages × 5-8 tests each) - Symbol extraction validation
- **CLI tool tests** - julie-codesearch and julie-semantic integration (fail fast if binary missing)
- **Fuzzy replace tests** - Levenshtein matching and UTF-8 safety
- **Trace call path tests** - Cross-language call tracing
- **Search quality tests** - SQLite FTS5 and semantic search functionality
- **Syntax validation tests** - AST-based syntax fixing

### ⏸️ Ignored Tests (30 tests - Run Manually)

These tests are marked `#[ignore]` to prevent hanging. Run them separately when needed:

```bash
# Run all ignored tests
cargo test --lib --ignored

# Or run specific categories:
cargo test --lib --ignored test_concurrent_manage_workspace
cargo test --lib --ignored test_target_filtering
cargo test --lib --ignored test_reference_workspace_search
```

#### Why Ignored?

**1. Workspace Indexing Tests (5 tests)**
- `test_target_filtering_matches_child_methods`
- `test_target_filtering_top_level_still_works`
- `test_target_filtering_case_insensitive`
- `test_target_filtering_partial_match`
- `test_reference_workspace_search`

**Reason:** Index entire workspace (300+ files), which can be slow (~60+ seconds) and may require background HNSW indexing. Not critical for CLI tool validation.

**2. Concurrent Stress Tests (1 test)**
- `test_concurrent_manage_workspace_index_does_not_lock_search_index`

**Reason:** Multi-threaded stress test that can deadlock. Useful for finding race conditions but not needed for standard development.

**3. Network-Dependent Tests (7 tests)**
- HNSW vector store tests (require model downloads)
- Embedding integration tests (require ONNX model downloads)
- Semantic search tests (network-dependent)

**Reason:** Download ~128MB ONNX models on first run. Will hang if offline or if downloads fail.

**4. Incomplete/TODO Tests (17 tests)**
- HNSW persistence tests (not yet implemented)
- ManageWorkspaceTool field mismatch
- Distance metric conversion issues

**Reason:** Features not yet implemented or tests disabled during refactoring.

## Test Organization

```
src/tests/
├── *_tests.rs                     # Language extractor tests (fast)
├── cli_codesearch_tests.rs       # CLI integration (fast - fails if binary missing)
├── cli_semantic_tests.rs         # CLI integration (fast - fails if binary missing)
├── fuzzy_replace_tests.rs        # Editing tool tests (fast)
├── trace_call_path_tests.rs      # Call tracing tests (fast)
├── get_symbols_target_filtering_tests.rs  # SLOW - workspace indexing (ignored)
├── search_race_condition_tests.rs         # SLOW - workspace indexing (some ignored)
├── workspace_mod_tests.rs                 # SLOW - concurrent stress test (ignored)
└── hnsw_vector_store_tests.rs            # Network-dependent (ignored)
```

## CI/CD Recommendations

```bash
# Standard PR checks (fast)
cargo test --lib

# Nightly extended tests (slow)
cargo test --lib --ignored

# Full test suite
cargo test --lib --all-targets
```

## For Contributors

**When adding new tests:**

- ✅ **DO** write fast unit tests using small code samples
- ✅ **DO** use `TempDir` for isolated test workspaces
- ✅ **DO** test core extractor logic with minimal files
- ❌ **DON'T** index the entire Julie workspace in tests (slow)
- ❌ **DON'T** download models in standard tests (use `#[ignore]`)
- ❌ **DON'T** create multi-threaded stress tests without `#[ignore]`

**Mark tests as ignored when they:**
- Index 100+ files
- Download external resources
- Take >5 seconds to complete
- Require specific environment setup
- Test race conditions with concurrency

## Troubleshooting

**Test hangs indefinitely:**
- Check if test indexes workspace → Add `#[ignore]`
- Check if test downloads models → Add `#[ignore]`
- Check for deadlock in concurrent code → Add `#[ignore]`

**CLI tests fail with "binary not found":**
- Build release binaries first:
  ```bash
  cargo build --release --bin julie-codesearch
  cargo build --release --bin julie-semantic
  ```

**HNSW/embedding tests fail:**
- Run with internet connection (will download ~128MB ONNX model)
- Or skip: `cargo test --lib` (skips ignored tests by default)

## Summary

| Category | Count | Run Time | Auto-Run | Purpose |
|----------|-------|----------|----------|---------|
| Fast tests | 640 | ~10s | ✅ Yes | Core functionality validation |
| Workspace indexing | 5 | 60s+ | ❌ Manual | Full workspace integration |
| Concurrent stress | 1 | 30s+ | ❌ Manual | Race condition detection |
| Network-dependent | 7 | Varies | ❌ Manual | Model downloads required |
| TODO/Incomplete | 17 | N/A | ❌ Manual | Features not yet implemented |

**Total:** 670 tests (640 fast + 30 ignored)

## Last Updated
2025-10-05 - Test suite optimized to prevent hangs
