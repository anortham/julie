# CLAUDE.md - Project Julie Development Guidelines

## 🔥 CRITICAL: WORKSPACE ARCHITECTURE (READ THIS FIRST) 🔥

**STOP! If you're about to write code that deals with workspaces, databases, or filtering, READ THIS SECTION FIRST.**

### The #1 Confusion Point: Workspace Filtering

**EACH WORKSPACE HAS SEPARATE PHYSICAL FILES:**
- Primary workspace: `.julie/indexes/julie_316c0b08/db/symbols.db` + `vectors/`
- Reference workspace: `.julie/indexes/coa-mcp-framework_c77f81e4/db/symbols.db` + `vectors/`

**WORKSPACE ISOLATION HAPPENS AT FILE LEVEL, NOT QUERY LEVEL:**

```
Tool receives workspace param → Routes to correct .db file → Opens connection
                                                                      ↓
                                                    Connection is LOCKED to that workspace
                                                    Database functions can ONLY query that workspace
                                                    NO SQL filtering on workspace_id possible!
```

**TOOL LEVEL (workspace parameter is ESSENTIAL):**
```rust
FastSearchTool {
    workspace: Some("primary")  // ← THIS MATTERS - routes to correct DB file
}
```

**DATABASE LEVEL (vestigial parameters REMOVED as of 2025-10-18):**
```rust
pub fn count_symbols(&self) -> Result<i64> {
    // ✅ CLEAN: No _workspace_id parameter - connection is already scoped to ONE workspace
    // Connection was opened to a specific .db file, can't query other workspaces
}
```

**See "Workspace Storage Architecture" section below for full details.**

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
- Make refactoring and testing harder

### Test Organization (Option A - Enforced)
**All tests in `src/tests/`, all fixtures in `fixtures/`**

```
src/tests/              # ALL test code (.rs files with #[test] functions)
├── database_tests.rs   # Tests for database module
├── search_tests.rs     # Tests for search functionality
└── ...

fixtures/               # ALL test data (SOURCE/CONTROL files, samples)
├── editing/           # SOURCE/CONTROL for editing tools
└── real-world/        # Real-world code samples for validation
```

**Rules:**
- ✅ ALL test code goes in `src/tests/`
- ✅ ALL test data/fixtures goes in `fixtures/`
- ❌ NO inline `#[cfg(test)] mod tests` in implementation files
- ❌ NO test data in `tests/` directory (that's a Rust convention for integration tests)

### Module Boundaries
**Each module MUST have a single, clear responsibility:**

```rust
// ✅ GOOD: Clear, focused responsibility
src/database/
├── mod.rs          # Public API, re-exports
├── schema.rs       # Schema definitions only
├── migrations.rs   # Migration logic only
└── queries.rs      # Query operations only

// ❌ BAD: God object with multiple responsibilities
src/database/
└── mod.rs          # 4,837 lines of everything
```

**Enforcement:**
- Each module does ONE thing
- Related functionality grouped logically
- Clear boundaries prevent coupling
- Public API minimal and well-documented

---

## Project Overview

**Julie** is a cross-platform code intelligence server built in Rust with production-grade architecture. Julie provides LSP-quality features across 26 programming languages using tree-sitter parsers, CASCADE architecture (SQLite FTS5 → HNSW Semantic), and instant search availability.

### Key Project Facts
- **Language**: Rust (native performance, true cross-platform)
- **Purpose**: Code intelligence MCP server (search, navigation, editing)
- **Architecture**: CASCADE (SQLite FTS5 → HNSW Semantic) - 2-tier single source of truth with progressive enhancement
- **Origin**: Native Rust implementation for true cross-platform compatibility
- **Crown Jewels**: 26 tree-sitter extractors with comprehensive test suites

### 🏆 Current Language Support (26/26 - Complete Language Support)

**All 26 extractors operational and validated against real-world GitHub code:**

**Core Languages:**
- Rust, TypeScript, JavaScript, Python, Java, C#, PHP, Ruby, Swift, Kotlin

**Systems Languages:**
- C, C++, Go, Lua

**Specialized Languages:**
- GDScript, Vue SFCs, Razor, QML, SQL, HTML, CSS, Regex, Bash, PowerShell, Zig, Dart

**Key Achievements:**
- ✅ **Zero compromises** - All extractors ported, none disabled
- ✅ **Native Rust performance** - No CGO/FFI dependencies
- ✅ **Cross-platform ready** - Windows, macOS, Linux compatible
- ✅ **Production validated** - Tested against real GitHub repositories
- ✅ **Comprehensive test coverage** - 100% compatibility with proven methodology

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

**MANDATORY** We will be using Julie to develop Julie (eating our own dog food) **ALWAYS USE JULIE'S TOOLS**

**MANDATORY** When dogfooding, you are also looking for bugs, if you see one, investigate it, don't just work around it and keep going.

### Development Workflow
1. **Development Mode**: Always work in `debug` mode for fast iteration
2. **Testing New Features**: When ready to test new functionality:
   - Agent asks user to exit Claude Code
   - User runs: `cargo build --release`
   - User restarts Claude Code with new Julie build
   - Test the new features in live MCP session
3. **Backward Compatibility**: We don't need it!
   - This is an MCP server not a REST API
   - We aren't concerned with backward compatiblity
   - We aren't concerned with maintaining API compatibiility
4. **Target User**: YOU, CLAUDE (and other AI coding agents) are the target user of this app
   - You should review code from the standpoint of you being the target user
   - Tool output should be optimized for YOU
   - Tool functionality should be optimized for YOU
5. **Agent Usage**: make use of the @agent-rust-tdd-implementer whenver possible

### Workspace Storage Architecture

# 🚨 READ THIS FIRST - WORKSPACE ROUTING 🚨

**IF YOU'RE CONFUSED ABOUT WORKSPACE FILTERING, READ THIS SECTION NOW:**

## How Workspace Isolation ACTUALLY Works

**Each workspace has its own PHYSICAL database and HNSW index files:**

```
.julie/indexes/
├── julie_316c0b08/              ← PRIMARY workspace
│   ├── db/symbols.db            ← SEPARATE SQLite database
│   └── vectors/                 ← SEPARATE HNSW index
│       ├── hnsw_index.hnsw.graph
│       └── hnsw_index.hnsw.data
│
└── coa-mcp-framework_c77f81e4/  ← REFERENCE workspace
    ├── db/symbols.db            ← SEPARATE SQLite database
    └── vectors/                 ← SEPARATE HNSW index
        ├── hnsw_index.hnsw.graph
        └── hnsw_index.hnsw.data
```

### Workspace Routing Happens in TWO Places:

**1. TOOL LEVEL (Where You Specify Workspace) - THIS IS CORRECT:**
```rust
// Tool receives workspace parameter - THIS ROUTES TO THE CORRECT DB FILE
FastSearchTool {
    query: "getUserData",
    workspace: Some("primary")  // ← Routes to julie_316c0b08/db/symbols.db
}

FastSearchTool {
    query: "getUserData", 
    workspace: Some("coa-mcp-framework_c77f81e4")  // ← Routes to coa-mcp-framework_c77f81e4/db/symbols.db
}
```

**2. DATABASE LAYER (Connection Already Scoped) - VESTIGIAL PARAMS REMOVED:**
```rust
// ✅ CLEAN (as of 2025-10-18): No _workspace_id parameter - connection already scoped
impl Database {
    pub fn count_symbols(&self) -> Result<i64> {
        // self.conn is ALREADY connected to ONE specific symbols.db file
        // Can't filter by workspace here - wrong architectural layer!
        // Vestigial _workspace_id parameters have been removed for clarity
    }
}
```

### KEY ARCHITECTURAL FACTS:

1. **Workspace selection happens when opening the DB connection** (tool → handler → workspace registry → open DB file)
2. **Once DB is open, you're locked to that workspace** - can't query other workspaces
3. **Database functions have NO workspace parameters** (removed 2025-10-18) - connection is already scoped
4. **Tool-level `workspace` parameters are ESSENTIAL** - they route to the correct DB file
5. **Each workspace is PHYSICALLY ISOLATED** - separate .db files, separate HNSW indexes

### What This Means For You:

- ✅ **DO** pass `workspace` parameter in tool calls (routes to correct DB)
- ❌ **DON'T** think database functions filter by workspace (they can't - connection is scoped)
- ✅ **DO** understand workspace isolation is PHYSICAL (separate files)
- ❌ **DON'T** look for SQL WHERE clauses on workspace_id (wrong layer!)

### Common Mistakes:

**WRONG ASSUMPTION:** "The database has all workspaces in one .db file and filters with WHERE workspace_id = ?"
**REALITY:** Each workspace has its OWN .db file. Filtering happens by opening the right file.

**WRONG ASSUMPTION:** "_workspace_id parameters in DB functions filter queries"
**REALITY (FIXED 2025-10-18):** DB connection is already scoped. Vestigial parameters have been removed for clarity.

---

**CRITICAL CONCEPT: Primary vs Reference Workspaces**

Julie distinguishes between two types of workspaces:

**Primary Workspace:**
- The workspace where you're actively developing (where you run Julie)
- Has its own `.julie/` directory at the workspace root
- Stores indexes for ITSELF **and ALL reference workspaces** in `.julie/indexes/`
- Full `JulieWorkspace` object with complete machinery
- Example: `/Users/murphy/source/julie/.julie/`

**Reference Workspaces:**
- Other codebases you want to search/reference (e.g., dependencies, related projects)
- Do **NOT** have their own `.julie/` directories
- Indexes stored in **primary workspace's** `.julie/indexes/{workspace_id}/`
- Just indexed data - not independent workspace objects
- Accessed by loading database/vectors directly from primary workspace's indexes
- Example: `~/source/coa-mcp-framework` is indexed, but data lives in `~/source/julie/.julie/indexes/coa-mcp-framework_c77f81e4/`

**Key Implication:** All workspace data (primary + references) lives under ONE `.julie/` directory in the primary workspace. Reference workspaces are "annexed" into the primary workspace's storage.

### Workspace Storage Location
**CRITICAL**: Julie stores workspace data at **project level**, not user home:
- **Primary workspace data**: `<project>/.julie/` (e.g., `/Users/murphy/source/julie/.julie/`)
- **NOT** at `~/.julie/` (this is a common mistake!)

### 🚨 LOG LOCATION (STOP LOOKING IN THE WRONG PLACE!)
**Logs are PROJECT-LEVEL, not user-level!**

**CORRECT log location:**
```
/Users/murphy/source/julie/.julie/logs/julie.log.2025-10-17
```

**WRONG** (don't look here):
```
~/.julie/logs/  ← THIS DOES NOT EXIST!
```

**When checking logs, ALWAYS use:**
```bash
# Primary workspace logs
ls -lh /Users/murphy/source/julie/.julie/logs/

# Or use relative path from project root
ls -lh .julie/logs/

# Tail latest log
tail -f .julie/logs/julie.log.$(date +%Y-%m-%d)
```

**Directory Structure (Actual Example from System):**
```
/Users/murphy/source/julie/.julie/
├── indexes/
│   ├── julie_316c0b08/              # Primary workspace indexes
│   │   ├── vectors/                 # HNSW semantic vectors
│   │   │   ├── hnsw_index.hnsw.graph
│   │   │   └── hnsw_index.hnsw.data
│   │   └── db/
│   │       └── symbols.db           # SQLite database (includes FTS5 index)
│   └── coa-mcp-framework_c77f81e4/  # Reference workspace indexes
│       ├── vectors/                 # HNSW semantic vectors
│       │   ├── hnsw_index.hnsw.graph
│       │   └── hnsw_index.hnsw.data
│       └── db/
│           └── symbols.db           # SQLite database (includes FTS5 index)
├── cache/
│   └── embeddings/                  # ONNX model cache (~128MB, shared)
├── models/                          # ML model files (shared)
├── logs/                            # Debug logs (shared)
└── workspace_registry.json          # Workspace metadata

Note: ~/source/coa-mcp-framework/ has NO .julie/ directory!
      Its indexes live in the primary workspace above.
```

**Key Benefits:**
- ✅ **Complete workspace isolation** - Each workspace has own db/vectors
- ✅ **Centralized storage** - All indexes in one location (primary workspace)
- ✅ **Multi-word AND/OR search** - SQLite FTS5 supports boolean operators natively
- ✅ **Trivial deletion** - `rm -rf indexes/{workspace_id}/` removes everything
- ✅ **Smaller, faster indexes** - Simpler 2-tier architecture, less disk space

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
- **[SEARCH_FLOW.md](docs/SEARCH_FLOW.md)** - CASCADE architecture and search flow (★ UPDATED 2025-10-12)
- **TODO.md** - Current observations and ideas
- **ARCHITECTURE_DEBT.md** - Known issues and technical debt

### Core Design Decisions
1. **CASCADE Architecture (2-Tier)**: SQLite FTS5 single source of truth → HNSW Semantic (background)
2. **Per-Workspace Isolation**: Each workspace gets own db/vectors in `indexes/{workspace_id}/`
3. **Native Rust**: No FFI, no CGO, no external dependencies
4. **Tree-sitter Native**: Direct Rust bindings for all language parsers
5. **SQLite FTS5 Search**: BM25 ranking, <5ms queries, multi-word AND/OR logic built-in
6. **ONNX Embeddings**: ort crate for semantic understanding
7. **Single Binary**: Deploy anywhere, no runtime required
8. **Graceful Degradation**: Search works immediately (SQLite FTS5), progressive enhancement to Semantic
9. **Relative Unix-Style Path Storage**: All file paths stored as relative with `/` separators for token efficiency and cross-platform compatibility

### 🔴 CRITICAL: Path Handling Contract (Cross-Platform Compatibility)

**All file paths in the database are stored as RELATIVE Unix-style paths.**

#### The Contract
- **Storage Format**: `src/tools/search.rs` (relative, Unix `/` separators)
- **NOT**: `/Users/murphy/project/src/tools/search.rs` (absolute)
- **NOT**: `src\tools\search.rs` (Windows `\` separators)

#### Why Relative Unix-Style?
1. **Token Efficiency**: `src/main.rs` (13 chars) vs `/Users/murphy/source/julie/src/main.rs` (42 chars) = 70% savings
2. **Cross-Platform**: Works on Windows, macOS, Linux without path conversion
3. **Portability**: Database can be moved between machines/OS
4. **No JSON Escaping**: Forward slashes don't need escaping in JSON

#### Implementation Pattern (MANDATORY)

**File Discovery** → Returns canonicalized absolute paths (resolves symlinks like macOS /var → /private/var)
**File Reading** → Uses absolute paths (for metadata, content reading)
**Database Storage** → Converts to relative Unix-style before storing
**Queries** → Accept both absolute and relative, normalize to relative for lookup

```rust
// ✅ CORRECT: Discovery canonicalizes for reading
let canonical_path = path.canonicalize().unwrap_or(path);  // /private/var/folders/.../test.rs
indexable_files.push(canonical_path);

// ✅ CORRECT: Processor handles both absolute and relative
let relative_path = if file_path.is_absolute() {
    to_relative_unix_style(file_path, workspace_root)?  // → "src/test.rs"
} else {
    file_path.to_string_lossy().replace('\\', "/")
};

// ✅ CORRECT: create_file_info needs absolute for reading metadata
let mut file_info = create_file_info(file_path, language)?;  // Reads with absolute path
file_info.path = relative_path;  // Overrides with relative for storage

// ❌ WRONG: Passing relative path to create_file_info
let file_info = create_file_info(&relative_path, language)?;  // FAILS: can't read "src/test.rs"
```

#### Key Locations Fixed (2025-10-27)
- `src/tools/workspace/discovery.rs:91` - Canonicalize paths before returning
- `src/tools/workspace/indexing/processor.rs` - 6 locations handling absolute vs relative
- `src/extractors/base.rs:437` - Handle both absolute and relative in BaseExtractor
- `src/utils/paths.rs:40` - Graceful canonicalization with fallback

### 🔴 CRITICAL: Single-Workspace Search Policy (Non-Negotiable)

**Search operations ALWAYS target ONE workspace at a time. No exceptions.**

#### The Rule
- **Search tools** (fast_search, fast_goto, fast_refs, etc.) → Search **ONE workspace only**
- **Default**: Primary workspace
- **Optional**: Specific reference workspace ID
- **FORBIDDEN**: "all workspaces" option - rejected as unnecessary complexity

#### Why Single-Workspace Only?
1. **Simplicity**: Reduces code complexity and potential bugs
2. **Performance**: No need to coordinate multi-workspace searches
3. **User Intent**: Users typically know which workspace they're searching
4. **Clarity**: Clear scope reduces confusion about search results

#### What About Multi-Workspace Operations?
- **Management tools** (ManageWorkspaceTool) → CAN list/view/manage all workspaces
- **Workspace registry** → MUST track all workspaces for management
- **Stats/Health checks** → CAN report on all workspaces

**The distinction:**
- **Search/Navigation** → Single workspace (where's this code?)
- **Management/Administration** → All workspaces (what do I have?)

#### Implementation Pattern
```rust
// ✅ CORRECT: Single workspace search
match workspace_param {
    "primary" | None => search_primary_workspace(),
    workspace_id => {
        validate_workspace_exists(workspace_id)?;
        search_reference_workspace(workspace_id)
    }
}

// ❌ WRONG: Multi-workspace search with loop
"all" => {
    for workspace_id in all_workspace_ids {
        search_workspace(workspace_id)?; // DON'T DO THIS
    }
}
```

#### Tool Documentation Requirements
All search/navigation tools MUST document:
- "Workspace filter (optional): 'primary' (default) or specific workspace ID"
- NO mention of "all workspaces" option
- Clear error message if "all" is attempted

**This is an architectural decision. Do not implement multi-workspace search without explicit approval.**

### Module Structure
```
src/
├── main.rs              # MCP server entry point
├── extractors/          # Language-specific symbol extraction
│   ├── mod.rs          # Extractor management
│   ├── base.rs         # BaseExtractor trait and common types
│   ├── typescript.rs   # TypeScript/JavaScript extractor
│   └── ...             # All other language extractors (25 total)
├── embeddings/          # ONNX-based semantic search
├── database/            # SQLite symbol storage (includes FTS5 search)
├── tools/               # MCP tool implementations
│   ├── mod.rs          # Tool registration and management
│   ├── search.rs       # Fast search, goto, refs tools (SQLite FTS5 + semantic)
│   ├── fuzzy_replace.rs # FuzzyReplaceTool (Levenshtein-based fuzzy matching)
│   ├── trace_call_path.rs # TraceCallPathTool (cross-language call tracing)
│   ├── refactoring.rs  # RenameSymbolTool, EditSymbolTool (semantic refactoring)
│   └── workspace/      # Workspace management tools
├── workspace/           # Multi-workspace registry
├── tracing/             # Logging and telemetry
├── utils/               # Shared utilities
└── tests/               # Comprehensive test infrastructure (see below)
```

---

## 🧪 Testing Standards

### Test Coverage Requirements
- **Extractors**: 100% comprehensive test coverage
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
│   └── ...                       # All 25 language extractors
│
├── tools/                          # Tool-specific tests
│   ├── fuzzy_replace_tests.rs     # FuzzyReplaceTool tests (18 tests, all passing)
│   ├── trace_call_path_tests.rs   # TraceCallPathTool tests (15 tests, all passing)
│   ├── refactoring_tests.rs       # RenameSymbolTool, EditSymbolTool tests
│   ├── search_tools_tests.rs      # Search/navigation tests
│   └── [7 disabled SafeEditTool test modules need migration to FuzzyReplaceTool]
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
- ✅ FuzzyReplaceTool (18 unit tests - Levenshtein similarity, UTF-8 safety, validation)
- ✅ TraceCallPathTool (15 unit tests - parameters, naming variants, filtering)
- ✅ RenameSymbolTool (workspace-wide symbol renaming)
- ✅ EditSymbolTool (semantic code editing - replace bodies, insert code, extract to file)
- ⚠️  7 SafeEditTool test modules disabled (need migration to FuzzyReplaceTool)

### Running Tests
```bash
# All tests
cargo test

# Specific test suites
cargo test fuzzy_replace          # FuzzyReplaceTool unit tests (18 tests)
cargo test trace_call_path        # TraceCallPathTool unit tests (15 tests)
cargo test typescript_extractor   # Language extractor tests

# With output
cargo test -- --nocapture

# Coverage analysis
cargo tarpaulin

# Performance tests
cargo test --release
```

### Test Performance Strategy

**Fast Tests vs Slow Tests:**

Julie's test suite is designed to run quickly during development, with slow integration tests marked as `#[ignore]`:

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
- After modifying FTS5 tokenization or query logic
- When validating CASCADE architecture changes
- Weekly regression checks

**Test Categories (all marked `#[ignore]`):**
1. Multi-word AND Logic (3 tests) - Validates boolean search operators
2. Hyphenated Terms (3 tests) - Tests tokenizer separator handling
3. Symbol Definitions (2 tests) - Verifies goto definition accuracy
4. FTS5 Internals (3 tests) - SQL query and ranking validation
5. Ranking Quality (1 test) - Source files rank above tests
6. Special Characters (3 tests) - Dots, colons, underscores
7. Tokenizer Consistency (1 test) - FTS5 tables use same tokenizer

**Rationale:** Compiling Rust is already slow; running 16 slow integration tests during every `cargo test` would cripple the development cycle. Fast feedback loops are essential for productivity.

### 🚨 URGENT: Test Organization Tasks

1. **Consolidate scattered tests** into `src/tests/` structure
2. **Clean up `.backup` files** and temporary test artifacts
3. **Integrate `debug/` test files** into real-world validation
4. **Complete SOURCE/CONTROL** for all editing tools
5. **Standardize test naming** and module organization
6. **Document test-running procedures** for contributors

---

## 🎯 Performance Targets (Non-Negotiable)

Julie must meet performance targets:

### Benchmarks
- **Search Latency**: <5ms SQLite FTS5, <50ms Semantic (target: 50ms)
- **Parsing Speed**: 5-10x optimized performance
- **Memory Usage**: <100MB typical (target: ~500MB)
- **Startup Time**: <2s (CASCADE SQLite only), 30-60x faster than old blocking approach
- **Background Indexing**: HNSW Semantic 20-30s (non-blocking, no intermediate layers)
- **Indexing Speed**: Process 1000 files in <2s (SQLite with FTS5)

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
2. **Test Parity**: All tests must pass in Julie
3. **Performance**: 5-10x performance improvement
4. **Memory Safety**: No unsafe code unless absolutely necessary
5. **Error Handling**: Comprehensive error handling with proper error types

### Deal Breakers
- CGO/FFI dependencies (breaks Windows compatibility)
- External runtime requirements (breaks single binary goal)
- Slow performance (defeats the purpose of rewrite)
- Test failures (indicates incomplete migration)

---

## 🎪 Test Migration Strategy

### Extractor Porting Process
1. **Copy reference tests exactly** - Don't change test logic
2. **Create Rust extractor structure** - Following base extractor pattern
3. **Port logic incrementally** - Function by function with tests
4. **Verify 100% test pass rate** - No compromises

### Test Suite Validation
```bash
# Reference test files - all must pass in Julie:
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
- `rusqlite`: SQLite database with FTS5 full-text search
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

## 🎮 GPU Acceleration Setup

### Platform-Specific Requirements

**Windows (DirectML):**
- ✅ **Works out of the box** with pre-built binaries
- Supports NVIDIA, AMD, and Intel GPUs
- No additional setup required

**Linux (CUDA):**
- ⚠️ **Requires CUDA 12.x + cuDNN 9**
- **CRITICAL**: CUDA 13.x is NOT compatible due to symbol versioning differences
- Pre-built ONNX Runtime binaries are compiled against CUDA 12.x libraries

**Setup Instructions for Linux:**
```bash
# 1. Install CUDA 12.6 (latest 12.x)
wget https://developer.download.nvidia.com/compute/cuda/12.6.2/local_installers/cuda_12.6.2_560.35.03_linux.run
sudo sh cuda_12.6.2_560.35.03_linux.run --toolkit --toolkitpath=/usr/local/cuda-12.6 --no-drm --no-man-page --override

# 2. Create symlink for compatibility
sudo ln -sf /usr/local/cuda-12.6 /usr/local/cuda-12

# 3. Download and install cuDNN 9
# Visit: https://developer.nvidia.com/cudnn-downloads
# Extract to /usr/local/cuda-12.6/

# 4. Add to library path (add to ~/.bashrc for persistence)
export LD_LIBRARY_PATH=/usr/local/cuda-12/lib64:$LD_LIBRARY_PATH
```

**macOS (CPU-optimized):**
- CPU mode is **faster than CoreML** for BERT/transformer models
- CoreML only accelerates ~25% of operations, rest falls back to CPU with overhead
- No GPU setup needed - optimized CPU inference is default

### Troubleshooting

**Check if GPU is being used:**
```bash
# Linux - watch GPU utilization during embedding generation
watch -n 0.5 nvidia-smi

# Check Julie logs
tail -f .julie/logs/julie.log.$(date +%Y-%m-%d) | grep -i "cuda\|gpu\|acceleration"
```

**Force CPU mode:**
```bash
# Useful for debugging or if GPU libraries are incompatible
export JULIE_FORCE_CPU=1
```

**Common Issues:**
- **"version `libcublas.so.12' not found"**: You have CUDA 13.x, need CUDA 12.x
- **"libcudnn.so.9 not found"**: cuDNN not installed or not in library path
- **CPU fallback automatic**: Julie will use CPU if GPU initialization fails (check logs)

### Why CUDA 12.x Requirement?

ONNX Runtime's pre-built binaries (from `download-binaries` feature) are compiled against CUDA 12.x. CUDA has **symbol versioning** in shared libraries - even though CUDA 13 is API-compatible, the internal symbol versions differ (`libcublas.so.12` vs `libcublas.so.13`), causing dynamic linking failures.

**Alternatives:**
1. Install CUDA 12.x (recommended - ~10min setup, 10-100x speedup)
2. Use CPU mode (works now, still fast with ONNX optimizations)
3. Build ONNX Runtime from source against CUDA 13 (complex, takes hours)

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
1. **All tests pass** in Rust
2. **Performance targets exceeded** (5-10x improvement)
3. **Windows deployment works** (single binary)
4. **Dogfooding successful** (can analyze its own code)
5. **Production ready** (stable, fast, reliable)

### TODO
Read the TODO.md file. Your user updates this file to track observations and ideas that come up during coding sessions.
---

*This document should be updated as the project evolves. All contributors must follow these guidelines without exception.*

**Project Status**: Phase 7 - 2-Tier CASCADE Architecture (Tantivy Removed) ✅
**Current Achievements**:
- ✅ All 25 Language Extractors Operational (Complete)
- ✅ **CASCADE Architecture Simplified**: SQLite FTS5 → HNSW Semantic (2-tier)
- ✅ **Tantivy Removed**: Eliminated Arc<RwLock> deadlocks, simpler architecture
- ✅ **Per-Workspace Isolation**: Complete workspace separation in `indexes/{workspace_id}/`
- ✅ **SQLite FTS5 Search**: <5ms BM25 ranking, AND/OR boolean logic, no locking issues
- ✅ <2s Startup Time with Background Indexing (30-60x improvement)
- ✅ **Tool Redesign Complete**: SafeEditTool → FuzzyReplaceTool + TraceCallPathTool
- ✅ **Critical Deadlock Fix**: Background embedding task now runs (was completely blocked)
- ✅ **Production Dogfooding Successful**: Found & fixed 6 critical bugs
  - UTF-8 crash (byte slicing → char iteration)
  - Query logic error (trace_call_path upstream)
  - Validation false positives (absolute → delta balance)
  - String mutation index corruption
  - **Registry deadlock** (background task waiting indefinitely)
  - **Tantivy Arc<RwLock> deadlocks** (5-10s commits causing contention)
- ✅ **Comprehensive Test Coverage**: 636 tests passing
  - 18 fuzzy_replace unit tests (Levenshtein, UTF-8, validation)
  - 15 trace_call_path unit tests (parameters, cross-language)
  - Per-workspace architecture verified
  - All Tantivy-dependent tests removed
- ✅ Real-World Validation Against GitHub Repositories
- ✅ Professional Error Detection (File Corruption Prevention)
- ✅ **GPU Acceleration Complete**:
  - Windows: DirectML (NVIDIA/AMD/Intel) - works out of box
  - Linux: CUDA support (requires CUDA 12.x + cuDNN 9 - documented)
  - macOS: CPU-optimized (faster than CoreML for transformers)
  - Automatic CPU fallback on initialization failure
  - TensorRT disabled on Linux (CUDA version mismatch simplified)

**Recent Updates (2025-10-27)**:
- ✅ **Atomic Database Transactions**: Incremental update corruption eliminated
  - FK constraint handling fixed (disable on connection, not in transaction)
  - `incremental_update_atomic()` wraps cleanup + insert in single transaction
  - Zero data loss window between cleanup and re-indexing
- ✅ **Path Handling Contract**: Relative Unix-style storage implemented
  - File discovery canonicalizes paths (resolves macOS symlinks)
  - Processor handles both absolute and relative paths (6 locations)
  - `create_file_info` uses absolute for reading, stores relative
  - 70% token savings, full cross-platform compatibility
- ✅ **Reference Workspace Indexing**: Fixed and tested
  - Correct database routing (primary vs reference)
  - Per-workspace isolation maintained
  - Critical tests passing

**Next Milestone**: Windows path testing + remaining test interdependency fixes
**Last Updated**: 2025-10-27 - Atomic Transactions and Path Handling Complete