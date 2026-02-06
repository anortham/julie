# CLAUDE.md - Project Julie Development Guidelines

## 🔥 CRITICAL: WORKSPACE ARCHITECTURE (Overview)

**Each workspace has SEPARATE PHYSICAL FILES:**
- Primary workspace: `.julie/indexes/julie_316c0b08/db/symbols.db` + `tantivy/`
- Reference workspace: `.julie/indexes/coa-mcp-framework_c77f81e4/db/symbols.db` + `tantivy/`

**WORKSPACE ISOLATION HAPPENS AT FILE LEVEL, NOT QUERY LEVEL:**
- Tool receives workspace param → Routes to correct .db file → Opens connection
- Connection is LOCKED to that workspace
- Database functions can ONLY query that workspace

**For detailed architecture info**, use Julie's code intelligence tools:
```
fast_search(query="workspace routing", search_target="definitions", file_pattern="docs/**")
```

See: **docs/WORKSPACE_ARCHITECTURE.md** for complete details.

---

## 🚨 PROJECT ORGANIZATION STANDARDS (NON-NEGOTIABLE)

### File Size Limits
**MANDATORY**: No implementation file shall exceed **500 lines**.

- Implementation files: **≤ 500 lines** (strictly enforced)
- Test files: **≤ 1000 lines** (acceptable for comprehensive test suites)
- **Any file exceeding these limits MUST be refactored into smaller modules**

**Rationale**: Files larger than 500 lines:
- Cannot be fully read by AI agents (token limits)
- Are difficult to understand and maintain
- Violate single responsibility principle

### Test Organization (Option A - Enforced)
**All tests in `src/tests/`, all fixtures in `fixtures/`**

```
src/tests/              # ALL test code (.rs files with #[test] functions)
├── database_tests.rs   # Tests for database module
├── search_tests.rs     # Tests for search functionality
└── ...

fixtures/               # ALL test data (SOURCE/CONTROL files, samples)
├── editing/           # SOURCE/CONTROL for editing tools
└── real-world/        # Real-world code samples
```

**Rules:**
- ✅ ALL test code goes in `src/tests/`
- ✅ ALL test data/fixtures goes in `fixtures/`
- ❌ NO inline `#[cfg(test)] mod tests` in implementation files
- ❌ NO test data in `tests/` directory

### Module Boundaries
**Each module MUST have a single, clear responsibility:**

```rust
// ✅ GOOD: Clear, focused responsibility
src/database/
├── mod.rs          # Public API, re-exports
├── schema.rs       # Schema definitions only
├── migrations.rs   # Migration logic only
└── queries.rs      # Query operations only

// ❌ BAD: God object
src/database/
└── mod.rs          # 4,837 lines of everything
```

---

## Project Overview

**Julie** is a cross-platform code intelligence server built in Rust with production-grade architecture. Julie provides LSP-quality features across 30 programming languages using tree-sitter parsers, Tantivy full-text search with code-aware tokenization, and instant search availability.

### Key Project Facts
- **Language**: Rust (native performance, cross-platform)
- **Purpose**: Code intelligence MCP server (search, navigation, editing)
- **Architecture**: Tantivy full-text search + SQLite structured storage
- **Origin**: Native Rust implementation for true cross-platform compatibility
- **Crown Jewels**: 30 tree-sitter extractors with comprehensive test suites

### 🏆 Current Language Support (30/30 - Complete)

**Core Languages:** Rust, TypeScript, JavaScript, Python, Java, C#, PHP, Ruby, Swift, Kotlin  
**Systems Languages:** C, C++, Go, Lua  
**Specialized:** GDScript, Vue, Razor, QML, R, SQL, HTML, CSS, Regex, Bash, PowerShell, Zig, Dart  
**Documentation:** Markdown, JSON, TOML

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

### Test-First Example
```rust
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

See: **docs/TESTING_GUIDE.md** for comprehensive testing standards and SOURCE/CONTROL methodology.

---

## 🐕 Dogfooding Strategy

**MANDATORY**: We use Julie to develop Julie (eating our own dog food).

**ALWAYS USE JULIE'S TOOLS** when developing.

**MANDATORY**: When dogfooding and you find a bug, investigate it. Don't work around it and keep going.

### Development Workflow
1. **Development Mode**: Always work in `debug` mode for fast iteration
2. **Testing New Features**: When ready to test:
   - Agent asks user to exit Claude Code
   - User runs: `cargo build --release`
   - User restarts Claude Code with new Julie build
   - Test features in live MCP session
3. **Backward Compatibility**: We don't need it (MCP server, not REST API)
4. **Target User**: YOU (Claude) and other AI coding agents are the target user
   - Review code from standpoint of you being the user
   - Optimize tool output for YOU
   - Optimize functionality for YOU

---

## 🐛 Debugging & Monitoring

### 🚨 LOG LOCATION (STOP LOOKING IN THE WRONG PLACE!)

**Logs are PROJECT-LEVEL, not user-level!**

**CORRECT log location:**
```
/Users/murphy/source/julie/.julie/logs/julie.log.2025-11-13
```

**WRONG** (don't look here):
```
~/.julie/logs/  ← THIS DOES NOT EXIST!
~/Library/Logs/ ← System logs only (crash reports)
~/.config/Julie/ ← Wrong location
```

**When checking logs, ALWAYS use:**
```bash
# Primary workspace logs (use date for current day)
tail -f .julie/logs/julie.log.$(date +%Y-%m-%d)

# Check indexing progress
tail -50 .julie/logs/julie.log.$(date +%Y-%m-%d) | grep -E "Tantivy|indexing|Background"

# View recent errors
tail -100 .julie/logs/julie.log.$(date +%Y-%m-%d) | grep -i error

# List all log files
ls -lh .julie/logs/
```

**Directory Structure:**
```
.julie/logs/
├── julie.log.2025-11-13    # Current day (dated files)
├── julie.log.2025-11-12    # Previous day
└── julie.log.2025-11-11    # Older logs
```

---

## 🏗️ Architecture Principles (Brief)

### Core Design Decisions
1. **Tantivy Search**: Code-aware full-text search with CamelCase/snake_case tokenization
2. **Per-Workspace Isolation**: Each workspace gets own db/tantivy in `indexes/{workspace_id}/`
3. **Native Rust**: No FFI, no CGO, no external dependencies
4. **Tree-sitter Native**: Direct Rust bindings for all language parsers
5. **SQLite Storage**: Symbols, identifiers, relationships, types, files
6. **Single Binary**: Deploy anywhere, no runtime required
7. **Instant Search**: Tantivy index available immediately after indexing
8. **Relative Unix-Style Path Storage**: All file paths stored as relative with `/` separators

For detailed architecture info, see: **docs/SEARCH_FLOW.md** and **docs/ARCHITECTURE.md**

### Module Structure (Brief)
```
src/
├── main.rs              # MCP server entry point
├── extractors/          # Language-specific symbol extraction (30 languages)
├── search/              # Tantivy search engine and tokenizer
├── database/            # SQLite structured storage
├── tools/               # MCP tool implementations
├── workspace/           # Multi-workspace registry
└── tests/               # Comprehensive test infrastructure
```

---

## 📚 For Detailed Documentation

**This file contains only the essentials for daily development.**

For detailed information on any topic, **use Julie's code intelligence tools**:

```rust
// Ask conceptual questions
fast_search(
    query="How does workspace routing work?",
    search_target="definitions",
    file_pattern="docs/**"
)

// Find specific documentation
fast_search(
    query="SOURCE/CONTROL testing methodology",
    search_target="content",
    file_pattern="docs/*.md"
)
```

### Documentation Index

- **docs/WORKSPACE_ARCHITECTURE.md** - Detailed workspace routing, storage, isolation
- **docs/TESTING_GUIDE.md** - SOURCE/CONTROL methodology, test coverage, running tests
- **docs/DEVELOPMENT.md** - Daily commands, debugging, release process
- **docs/PERFORMANCE.md** - Performance targets and benchmarking
- **docs/DEPENDENCIES.md** - Tree-sitter versions, dependency management
- **docs/SEARCH_FLOW.md** - Tantivy search architecture
- **docs/RAG_TRANSFORMATION.md** - RAG POC results and token reduction
- **docs/RAG_POC_PROGRESS.md** - POC progress tracker (100% complete)
- **docs/ARCHITECTURE.md** - Token optimization strategies
- **docs/INTELLIGENCE_LAYER.md** - Cross-language intelligence

### Query Examples

**Architecture questions:**
- *"How does Tantivy search work?"*
- *"What is workspace isolation?"*

**Implementation questions:**
- *"What is SOURCE/CONTROL testing?"*
- *"How do I run performance tests?"*

**Julie will return targeted documentation sections (355-525 tokens) instead of full files (2,000-9,000 tokens), achieving 83-94% token reduction while maintaining complete context quality.**

---

**Last Updated:** 2025-11-07
**Status:** Production Ready (v1.1.0)
**Project Status:** See **docs/RAG_POC_PROGRESS.md** for current milestone achievements

---

*Use Julie's code intelligence tools to access detailed documentation on-demand. Save 85-95% of context tokens.*
- you CANNOT build the release build while we're running the server in session!