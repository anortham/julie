# CLAUDE.md - Project Julie Development Guidelines

## Project Overview

**Julie** is a cross-platform code intelligence server built in Rust, rising from Miller's ashes with the right architecture. Julie provides LSP-quality features across 20+ programming languages using tree-sitter parsers, Tantivy search, and semantic embeddings.

### Key Project Facts
- **Language**: Rust (native performance, true cross-platform)
- **Purpose**: Code intelligence MCP server (search, navigation, editing)
- **Architecture**: Single binary, no external dependencies
- **Origin**: Rebuilt from Miller (TypeScript/Bun) due to Windows compatibility issues
- **Crown Jewels**: 27 tree-sitter extractors with comprehensive test suites

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

### Why This Matters
- **Real-world validation**: If Julie can't analyze its own code, it's not ready
- **Performance testing**: Large codebase stress testing
- **Usability validation**: We experience our own UX decisions

### Agent Instructions for Testing
When implementing new features, the agent should say:
> "The new [feature] is ready for testing. Please exit Claude Code, run `cargo build --release`, and restart Claude Code to test the new functionality."

---

## 🏗️ Architecture Principles

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
│   └── ...             # All other language extractors
├── search/              # Tantivy-based search engine
├── embeddings/          # ONNX-based semantic search
├── database/            # SQLite symbol storage
├── tools/               # MCP tool implementations
├── utils/               # Shared utilities
└── tests/               # Test infrastructure
```

---

## 🧪 Testing Standards

### Test Coverage Requirements
- **Extractors**: 100% test parity with Miller's test suites
- **Core Logic**: >90% coverage on search and navigation
- **MCP Tools**: Full integration testing
- **Cross-platform**: Automated testing on Windows, macOS, Linux

### Test Organization
```rust
// File: src/extractors/typescript.rs
pub struct TypeScriptExtractor { /* ... */ }

// File: src/tests/typescript_tests.rs
#[cfg(test)]
mod typescript_extractor_tests {
    use super::*;

    #[test]
    fn test_extract_function_declarations() {
        // Port Miller's test cases exactly
    }
}
```

### Running Tests
```bash
# All tests
cargo test

# Specific extractor
cargo test typescript_extractor

# With logging
RUST_LOG=debug cargo test

# Release mode (performance testing)
cargo test --release
```

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
Before adding any dependency, consider:
1. Does it break single binary deployment?
2. Does it require external libraries?
3. Is it cross-platform compatible?
4. Does it impact startup time significantly?

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

**Project Status**: Phase 0 - Foundation Setup
**Next Milestone**: Complete basic MCP server and directory structure
**Last Updated**: Project initialization