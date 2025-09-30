# CLAUDE.md - Project Julie Development Guidelines

## Project Overview

**Julie** is a cross-platform code intelligence server built in Rust, rising from Miller's ashes with the right architecture. Julie provides LSP-quality features across 20+ programming languages using tree-sitter parsers, Tantivy search, and semantic embeddings.

### Key Project Facts
- **Language**: Rust (native performance, true cross-platform)
- **Purpose**: Code intelligence MCP server (search, navigation, editing)
- **Architecture**: Single binary, no external dependencies
- **Origin**: Rebuilt from Miller (TypeScript/Bun) due to Windows compatibility issues
- **Crown Jewels**: 26 tree-sitter extractors with comprehensive test suites (100% Miller parity)

### 🏆 Current Language Support (26/26 - Complete Miller Parity)

**All 26 extractors operational and validated against real-world GitHub code:**

**Core Languages:**
- Rust, TypeScript, JavaScript, Python, Java, C#, PHP, Ruby, Swift, Kotlin

**Systems Languages:**
- C, C++, Go, Lua

**Specialized Languages:**
- GDScript, Vue SFCs, Razor, SQL, HTML, CSS, Regex, Bash, PowerShell, Zig, Dart

**Key Achievements:**
- ✅ **Zero compromises** - All Miller extractors ported, none disabled
- ✅ **Native Rust performance** - No CGO/FFI dependencies
- ✅ **Cross-platform ready** - Windows, macOS, Linux compatible
- ✅ **Production validated** - Tested against real GitHub repositories
- ✅ **Miller test parity** - 100% compatibility with proven methodology

---

## 🔴 CRITICAL: TDD Methodology (Non-Negotiable)

This project **MUST** follow Test-Driven Development:

### TDD Cycle for All Development
1. **RED**: Write a failing test first
2. **GREEN**: Write minimal code to make test pass
3. **REFACTOR**: Improve code while keeping tests green

### Bug Hunting Protocol
**NEVER** fix a bug without following this sequence:
1. **Find the bug** through investigation
2. **Write a failing test** that reproduces the bug exactly
3. **Verify the test fails** (confirms bug reproduction)
4. **Fix the bug** with minimal changes
5. **Verify the test passes** (confirms bug fixed)
6. **Ensure no regressions** (all other tests still pass)

### Test-First Examples
```rust
// ❌ WRONG: Implementing before testing
fn extract_typescript_functions() {
    // implementation code...
}

// ✅ CORRECT: Test first, then implement
#[cfg(test)]
mod tests {
    #[test]
    fn test_extract_typescript_functions() {
        let code = "function getUserData() { return data; }";
        let symbols = extract_symbols(code);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "getUserData");
        // This WILL FAIL initially - that's the point!
    }
}
```

---

## 🐕 Dogfooding Strategy

We will be using Julie to develop Julie (eating our own dog food):

### Development Workflow
1. **Development Mode**: Always work in `debug` mode for fast iteration
2. **Testing New Features**: When ready to test new functionality:
   - Agent asks user to exit Claude Code
   - User runs: `cargo build --release`
   - User restarts Claude Code with new Julie build
   - Test the new features in live MCP session

### Workspace Storage Location
**CRITICAL**: Julie stores workspace data at **project level**, not user home:
- **Workspace data**: `<project>/.julie/` (e.g., `/Users/murphy/Source/julie/.julie/`)
- **NOT** at `~/.julie/` (this is a common mistake!)

**Directory Structure:**
```
<project>/.julie/
├── db/
│   └── symbols.db           # SQLite database with all symbols
├── index/                   # Tantivy search indexes
├── vectors/                 # Semantic embeddings (HNSW)
├── cache/                   # Query caches
├── logs/                    # Debug logs
└── workspace_registry.json  # Workspace metadata
```

### Why This Matters
- **Real-world validation**: If Julie can't analyze its own code, it's not ready
- **Performance testing**: Large codebase stress testing
- **Usability validation**: We experience our own UX decisions

### Agent Instructions for Testing
When implementing new features, the agent should say:
> "The new [feature] is ready for testing. Please exit Claude Code, run `cargo build --release`, and restart Claude Code to test the new functionality."

---

## 🏗️ Architecture Principles

### 📚 Architecture Documentation
**IMPORTANT**: Read these documents to understand Julie's architecture:
- **[SEARCH_FLOW.md](docs/SEARCH_FLOW.md)** - How searches flow through the system (KEEP THIS UPDATED!)
- **TODO.md** - Current observations and ideas
- **ARCHITECTURE_DEBT.md** - Known issues and technical debt

### Core Design Decisions
1. **Native Rust**: No FFI, no CGO, no external dependencies
2. **Tree-sitter Native**: Direct Rust bindings for all language parsers
3. **Tantivy Search**: 2x faster than Lucene, pure Rust
4. **ONNX Embeddings**: ort crate for semantic understanding
5. **Single Binary**: Deploy anywhere, no runtime required

### Module Structure
```
src/
├── main.rs              # MCP server entry point
├── extractors/          # Language-specific symbol extraction
│   ├── mod.rs          # Extractor management
│   ├── base.rs         # BaseExtractor trait and common types
│   ├── typescript.rs   # TypeScript/JavaScript extractor
│   └── ...             # All other language extractors (26 total)
├── search/              # Tantivy-based search engine
│   ├── mod.rs          # Search infrastructure
│   ├── schema.rs       # Tantivy schema definitions
│   ├── tokenizers.rs   # Code-specific tokenization
│   └── engine/         # Search engine implementation
├── embeddings/          # ONNX-based semantic search
├── database/            # SQLite symbol storage
├── tools/               # MCP tool implementations
│   ├── mod.rs          # Tool registration and management
│   ├── search.rs       # Fast search, goto, refs tools
│   ├── editing.rs      # FastEditTool, LineEditTool
│   ├── refactoring.rs  # SmartRefactorTool (RenameSymbol, etc.)
│   └── workspace/      # Workspace management tools
├── workspace/           # Multi-workspace registry
├── tracing/             # Logging and telemetry
├── utils/               # Shared utilities
└── tests/               # Comprehensive test infrastructure (see below)
```

---

## 🧪 Testing Standards

### Test Coverage Requirements
- **Extractors**: 100% test parity with Miller's test suites
- **Editing Tools**: 90% coverage with SOURCE/CONTROL methodology
- **Core Logic**: >80% coverage on search and navigation
- **MCP Tools**: Full integration testing
- **Cross-platform**: Automated testing on Windows, macOS, Linux

### 🚨 Current Test Organization Issues (NEEDS REORGANIZATION)

**Test files are currently scattered across:**
- `src/tests/` - Main test infrastructure (GOOD)
- `src/tests/*/` - Language-specific extractor tests (ACCEPTABLE)
- Individual extractor files with inline tests (BAD - creates clutter)
- `debug/` directory with real-world test files (NEEDS INTEGRATION)
- `tests/editing/` - SOURCE/CONTROL methodology files (GOOD)
- Various `.backup` files (CLEANUP NEEDED)

### 🎯 Target Test Organization (TO BE IMPLEMENTED)

```
src/tests/                           # Central test infrastructure
├── mod.rs                          # Test module management
├── test_helpers.rs                 # Shared test utilities
├── real_world_validation.rs        # Real-world test coordinator
│
├── extractors/                     # Language extractor tests
│   ├── mod.rs                     # Extractor test management
│   ├── typescript_tests.rs       # TypeScript/JavaScript tests
│   ├── python_tests.rs           # Python tests
│   └── ...                       # All 26 language extractors
│
├── tools/                          # Tool-specific tests
│   ├── editing_tests.rs           # FastEditTool tests (SOURCE/CONTROL)
│   ├── line_edit_control_tests.rs # LineEditTool tests (SOURCE/CONTROL)
│   ├── refactoring_tests.rs       # SmartRefactorTool tests
│   ├── search_tools_tests.rs      # Search/navigation tests
│   └── editing_safety_tests.rs    # Safety and concurrency tests
│
└── integration/                    # End-to-end integration tests
    ├── mcp_integration_tests.rs   # Full MCP server tests
    └── performance_tests.rs       # Performance benchmarks

tests/editing/                      # SOURCE/CONTROL test data
├── sources/                       # Original files (never edited)
│   ├── typescript/
│   ├── python/
│   └── ...
└── controls/                      # Expected results after edits
    ├── fast-edit/
    ├── line-edit/
    └── refactor/

debug/                             # Development test data (TO BE ORGANIZED)
├── test-workspace-real/           # Real-world files for validation
└── test-workspace-editing-controls/ # Existing SOURCE/CONTROL examples
```

### 📊 Code Coverage Tooling

**Configuration**: `tarpaulin.toml`
- General threshold: 80%
- Editing tools threshold: 90% (critical for safety)
- Coverage reports: HTML, LCOV, JSON formats

**Commands**:
```bash
# Run coverage analysis
cargo tarpaulin

# Generate detailed HTML report
cargo tarpaulin --output-dir target/tarpaulin --output-format Html

# Check specific module coverage
cargo tarpaulin --include src/tools/editing.rs
```

### 🛡️ SOURCE/CONTROL Testing Methodology (Professional Standard)

**Critical Pattern for All File Modification Tools:**

1. **SOURCE files** - Original files that are NEVER modified
2. **CONTROL files** - Expected results after specific operations
3. **Test Process**: SOURCE → copy → edit → diff against CONTROL
4. **Verification**: Must match exactly using diff-match-patch

**Example Structure**:
```rust
// Test case definition
struct EditingTestCase {
    name: &'static str,
    source_file: &'static str,    // Never modified
    control_file: &'static str,   // Expected result
    operation: &'static str,
    // ... operation parameters
}

// Test execution
fn run_test(test_case: &EditingTestCase) -> Result<()> {
    // 1. Copy SOURCE to temp location
    let test_file = copy_source_file(test_case.source_file)?;

    // 2. Perform operation
    perform_edit_operation(&test_file, &test_case)?;

    // 3. Load CONTROL (expected result)
    let expected = load_control_file(test_case.control_file)?;

    // 4. Verify exact match using diff-match-patch
    verify_exact_match(&test_file, &expected)?;
}
```

**Implemented For:**
- ✅ FastEditTool (3 test cases with perfect matches)
- ✅ LineEditTool (2/3 test cases working)
- ❌ SmartRefactorTool (TODO)

### Running Tests
```bash
# All tests
cargo test

# Specific test suites
cargo test editing_tests          # FastEditTool SOURCE/CONTROL tests
cargo test line_edit_control      # LineEditTool SOURCE/CONTROL tests
cargo test typescript_extractor   # Language extractor tests

# With output
cargo test -- --nocapture

# Coverage analysis
cargo tarpaulin

# Performance tests
cargo test --release
```

### 🚨 URGENT: Test Organization Tasks

1. **Consolidate scattered tests** into `src/tests/` structure
2. **Clean up `.backup` files** and temporary test artifacts
3. **Integrate `debug/` test files** into real-world validation
4. **Complete SOURCE/CONTROL** for all editing tools
5. **Standardize test naming** and module organization
6. **Document test-running procedures** for contributors

---

## 🎯 Performance Targets (Non-Negotiable)

Julie must significantly outperform Miller:

### Benchmarks
- **Search Latency**: <10ms (vs Miller's 50ms)
- **Parsing Speed**: 5-10x faster than Miller
- **Memory Usage**: <100MB typical (vs Miller's ~500MB)
- **Startup Time**: <1s cold start
- **Indexing Speed**: Process 1000 files in <30s

### Performance Testing
```bash
# Benchmark suite
cargo bench

# Profile memory usage
valgrind --tool=massif cargo run --release

# Profile CPU usage
perf record cargo run --release
perf report
```

---

## 🔧 Development Commands

### Daily Development
```bash
# Fast iteration (debug build)
cargo build && cargo run

# Run specific tests during development
cargo test typescript_extractor --no-capture

# Watch for changes
cargo watch -x "build" -x "test"

# Check for issues
cargo clippy
cargo fmt
```

### Release Preparation
```bash
# Optimized build
cargo build --release

# Cross-platform builds
cargo build --target x86_64-pc-windows-msvc --release
cargo build --target x86_64-unknown-linux-gnu --release

# Size optimization
cargo bloat --release
```

---

## 🚨🔴 TREE-SITTER VERSION WARNING 🔴🚨

### ⚠️ ABSOLUTELY DO NOT CHANGE TREE-SITTER VERSIONS ⚠️

**THE FOLLOWING VERSIONS ARE LOCKED AND TESTED:**
- `tree-sitter = "0.25"` (REQUIRED for harper-tree-sitter-dart)
- `tree-sitter-kotlin-ng = "1.1.0"` (modern Kotlin parser)
- `harper-tree-sitter-dart = "0.0.5"` (modern Dart parser)

**CHANGING THESE WILL CAUSE:**
- ❌ API incompatibilities between different tree-sitter versions
- ❌ Native library linking conflicts
- ❌ Hours of debugging version hell
- ❌ Complete build failures
- ❌ Breaking all extractors

**IF YOU MUST CHANGE VERSIONS:**
1. Update ALL parser crates simultaneously
2. Test every single extractor
3. Update API calls if needed (0.20 vs 0.25 APIs are different)
4. Verify no native library conflicts
5. Test on all platforms

**WE ALREADY WENT THROUGH VERSION HELL - DON'T DO IT AGAIN!**

---

## 🚨 Critical Success Factors

### Must-Have Requirements
1. **Windows Compatibility**: Single `cargo build` must work on Windows
2. **Test Parity**: Every Miller test must pass in Julie
3. **Performance**: 5-10x improvement over Miller
4. **Memory Safety**: No unsafe code unless absolutely necessary
5. **Error Handling**: Comprehensive error handling with proper error types

### Deal Breakers
- CGO/FFI dependencies (breaks Windows compatibility)
- External runtime requirements (breaks single binary goal)
- Slower than Miller (defeats the purpose of rewrite)
- Test failures (indicates incomplete migration)

---

## 🎪 Migration Strategy from Miller

### Extractor Porting Process
1. **Copy Miller's tests exactly** - Don't change test logic
2. **Create Rust extractor structure** - Following base extractor pattern
3. **Port logic incrementally** - Function by function with tests
4. **Verify 100% test pass rate** - No compromises

### Test Suite Validation
```bash
# Miller had these test files - all must pass in Julie:
typescript-extractor.test.ts -> typescript_tests.rs
javascript-extractor.test.ts -> javascript_tests.rs
python-extractor.test.ts -> python_tests.rs
# ... and 24 more extractors
```

---

## 🔍 Debugging Guidelines

### Logging Strategy
```rust
use tracing::{debug, info, warn, error};

// Use structured logging
debug!(file_path = %path, symbols_found = symbols.len(), "Extracted symbols");
```

### Debug Mode Features
- Detailed tracing logs
- Symbol extraction debugging
- Search query analysis
- Performance timing

### Production Mode
- Minimal logging
- Error reporting only
- Optimized performance
- No debug overhead

---

## 📦 Dependencies Management

### Core Dependencies (Already in Cargo.toml)
- `rust-mcp-sdk`: MCP protocol implementation
- `tree-sitter`: Parser framework with all language bindings
- `tantivy`: Search engine
- `ort`: ONNX runtime for embeddings
- `tokio`: Async runtime
- `rayon`: Data parallelism

### Adding New Dependencies

**🔴 CRITICAL: ALWAYS verify dependency versions first!**

Before adding any dependency:
1. **Use crates.io search to verify the latest version**: https://crates.io/search?q=CRATE_NAME
2. **Use web search to verify the API and examples** - Don't guess API!
3. **Check current documentation and examples** for breaking changes
4. Does it break single binary deployment?
5. Does it require external libraries?
6. Is it cross-platform compatible?
7. Does it impact startup time significantly?

**Examples**:
- Before adding `tokio = "1.47"`, search https://crates.io/search?q=tokio (latest is 1.47.1)
- Before adding `blake3 = "1.5"`, search https://crates.io/search?q=blake3 (latest is 1.8.0)
- Search "FastEmbed Rust API documentation" to understand current API and avoid compilation errors

---

## 🎯 Success Metrics

### Phase Completion Criteria
Each phase must meet these criteria:
- [ ] All tests pass
- [ ] Performance targets met
- [ ] Cross-platform builds successful
- [ ] Memory usage within limits
- [ ] No regressions from previous phases

### Final Success Definition
Julie is successful when:
1. **All Miller tests pass** in Rust
2. **Performance targets exceeded** (5-10x improvement)
3. **Windows deployment works** (single binary)
4. **Dogfooding successful** (can analyze its own code)
5. **Production ready** (stable, fast, reliable)

### TODO
Read the TODO.md file. Your user updates this file to track observations and ideas that come up during coding sessions.
---

*This document should be updated as the project evolves. All contributors must follow these guidelines without exception.*

**Project Status**: Phase 3.1 - Professional Testing Infrastructure Complete ✅
**Current Achievements**:
- ✅ All 26 Language Extractors Operational (Miller Parity)
- ✅ SOURCE/CONTROL Testing Methodology Implemented
- ✅ Code Coverage Tooling (tarpaulin) Configured
- ✅ FastEditTool + LineEditTool Control Tests Operational
- ✅ Real-World Validation Against GitHub Repositories
- ✅ Professional Error Detection (File Corruption Prevention)

**Next Milestone**: Complete SmartRefactorTool Control Tests + Test Organization Cleanup
**Last Updated**: 2025-01-28 - Professional Testing Infrastructure Achievement