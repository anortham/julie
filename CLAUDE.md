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

**Julie** is a cross-platform code intelligence server built in Rust. LSP-quality features across 31 languages via tree-sitter, Tantivy full-text search, and instant search availability.

### Key Project Facts
- **Language**: Rust (native performance, cross-platform)
- **Purpose**: Code intelligence MCP server (search, navigation, editing)
- **Architecture**: Tantivy full-text search + SQLite structured storage
- **Mode**: Stdio-only MCP server (JSON-RPC over stdin/stdout)
- **Origin**: Native Rust implementation for true cross-platform compatibility
- **Crown Jewels**: 31 tree-sitter extractors with comprehensive test suites

### 🏆 Current Language Support (31 - Complete)

**Core Languages:** Rust, TypeScript, JavaScript, Python, Java, C#, PHP, Ruby, Swift, Kotlin
**Systems Languages:** C, C++, Go, Lua, Zig
**Specialized:** GDScript, Vue, Razor, QML, R, SQL, HTML, CSS, Regex, Bash, PowerShell, Dart
**Documentation:** Markdown, JSON, JSONL, TOML, YAML

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

See: **docs/TESTING_GUIDE.md** for comprehensive testing standards and SOURCE/CONTROL methodology.

---

## 🚨 RUNNING TESTS (USE THE XTASK RUNNER)

**The full suite is still too expensive to run after every small change.** Use the xtask runner as the canonical interface so the same calibrated buckets show up everywhere.

### Canonical Test Tiers

| Tier | Command | What it covers | When to use |
|------|---------|----------------|-------------|
| **Smoke** | `cargo xtask test smoke` | Small confidence slice of the fastest buckets | Quick sanity check when you want a tiny run |
| **Dev** | `cargo xtask test dev` | Default local tier for normal code changes | After the usual change-set |
| **System** | `cargo xtask test system` | `workspace_init` + integration buckets | Use when touching startup/workspace/system behavior |
| **Dogfood** | `cargo xtask test dogfood` | `search_quality` bucket | Use after search/scoring/tokenization changes |
| **Full** | `cargo xtask test full` | Dev + system + dogfood buckets | Use for broad branch-level confidence |

### Default Workflow

1. **After normal changes**: run `cargo xtask test dev`
2. **If you changed startup/workspace/system flows**: add `cargo xtask test system`
3. **If you changed search/scoring/tokenization**: add `cargo xtask test dogfood`
4. **For a broad pre-merge pass**: run `cargo xtask test full`
5. **To inspect the calibrated buckets**: run `cargo xtask test list`

### Current Caveat

`system` and `full` are the right commands for those tiers, but do **not** describe them as green-by-default right now. Calibration uncovered a pre-existing `workspace_init` failure/outlier in the `system` bucket, so those tiers remain the canonical runner entry points while that bucket is being fixed. That issue blocks both `system` and `full` from being treated as green-by-default until the bucket is fixed.

### Why Dogfood Is Slow

The `search_quality` bucket loads a **100MB SQLite fixture**, backfills a Tantivy index from it, and runs real searches. It is a regression guard, not a quick unit-tier pass.

### The Rules

1. **Default to `cargo xtask test dev` after normal changes.**
2. **Escalate with xtask tiers instead of inventing ad hoc canonical commands.**
3. **Use raw cargo filters only to narrow failures** after an xtask tier fails or when iterating on one known area.
4. **Do not casually run `cargo test --lib` as the default workflow.**

### Narrowing Failures With Raw Cargo Filters

When an xtask tier fails and you need to zoom in, use targeted cargo filters like these:

```bash
# By module area
cargo test --lib tests::core              # database, workspace init (~30s)
cargo test --lib tests::tools::search     # search engine tests (~20s)
cargo test --lib tests::tools::get_context # get_context tests (~15s)
cargo test --lib tests::tools::deep_dive  # deep_dive tests (~10s)
cargo test --lib tests::integration       # integration tests (~15s)
cargo test --lib tests::tools::editing    # editing tools (~5s)

# By specific test name
cargo test --lib test_stemming            # all stemming tests
cargo test --lib test_centrality          # all centrality tests
cargo test --lib test_namespace           # namespace de-boost tests
```

### Rebuilding Fixture Database

Only needed when the test fixture schema changes or after adding new source to the fixture:
```bash
cargo test --lib build_julie_fixture -- --ignored --nocapture
```

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
   - User restarts Claude Code (MCP client spawns new stdio server)
   - Test features in live MCP session
3. **Backward Compatibility**: We don't need it (stdio MCP server, not a public API)
4. **Target User**: YOU (Claude) and other AI coding agents are the target user
   - Review code from standpoint of you being the user
   - Optimize tool output for YOU
   - Optimize functionality for YOU

---

## 🐛 Debugging & Monitoring

### 🚨 LOG LOCATION

Julie logs to the project-level `.julie/logs/` directory.

**When checking logs, ALWAYS use:**
```bash
# Julie logs (project-level)
tail -f .julie/logs/julie.log.$(date +%Y-%m-%d)

# Check indexing progress
tail -50 .julie/logs/julie.log.$(date +%Y-%m-%d) | grep -E "Tantivy|indexing|Background"

# View recent errors
tail -100 .julie/logs/julie.log.$(date +%Y-%m-%d) | grep -i error

# List all log files
ls -lh .julie/logs/
```

---

## 🏗️ Architecture Principles (Brief)

### Core Design Decisions
1. **Tantivy Search**: Code-aware full-text search with CamelCase/snake_case tokenization + English stemming
2. **Graph Centrality Ranking**: Pre-computed reference scores boost well-connected symbols in search results
3. **Per-Workspace Isolation**: Each workspace gets own db/tantivy in `indexes/{workspace_id}/`
4. **Native Rust Core**: No FFI, no CGO — core indexing/search has zero external dependencies
5. **Tree-sitter Native**: Direct Rust bindings for all language parsers
6. **SQLite Storage**: Symbols, identifiers, relationships, types, files
7. **Single Binary + Optional Sidecar**: Core features work standalone; GPU-accelerated embeddings use a managed Python sidecar (auto-provisioned via `uv`)
8. **Instant Search**: Tantivy index available immediately after indexing
9. **Relative Unix-Style Path Storage**: All file paths stored as relative with `/` separators
10. **Language-Agnostic Everything**: See below — this is critical

### 🔴 CRITICAL: Language-Agnostic Design (Non-Negotiable)

**Julie supports 31 languages and indexes ANY codebase.** All scoring, ranking, filtering, path analysis, and heuristics MUST work across all project layouts — not just Rust or Julie's own directory structure.

**The rule is simple: if you're writing code that checks a file path, symbol kind, project structure, or naming convention, it MUST work for ALL of these:**

| Layout | Source Code | Tests | Docs |
|--------|-------------|-------|------|
| Rust | `src/` | `src/tests/`, `tests/` | `docs/` |
| C# / .NET | `MyProject/` | `MyProject.Tests/` | `docs/` |
| Python | `mypackage/` | `tests/`, `test_*.py` | `docs/` |
| Java/Kotlin | `src/main/java/` | `src/test/java/` | `docs/` |
| Go | `pkg/`, `internal/`, `cmd/` | `*_test.go` | `docs/` |
| JavaScript/TS | `src/`, `lib/` | `__tests__/`, `*.test.ts`, `*.spec.ts` | `docs/` |
| Ruby | `lib/` | `test/`, `spec/` | `docs/` |
| Swift | `Sources/` | `Tests/` | `docs/` |

**Common violations to watch for:**
- ❌ `path.starts_with("src/tests/")` — only matches Rust layout
- ❌ `path.starts_with("src/")` — doesn't match Python, C#, Java, Go, etc.
- ❌ Checking for `mod.rs` or `Cargo.toml` to detect project root
- ✅ Use generic heuristics: path contains `test`, `tests`, `.Tests`, `_test`, `spec`, `__tests__`
- ✅ Use generic heuristics: path contains `docs/`, `doc/`, `documentation/`
- ✅ Use file metadata (symbol kind, centrality score) over path assumptions

**Before writing ANY path-based heuristic, ask: "Does this work for a C# project? A Python monorepo? A Java Gradle project?"** If the answer is no, make it generic.

For detailed architecture info, see: **docs/SEARCH_FLOW.md** and **docs/ARCHITECTURE.md**

---

## 📚 Documentation

Use Julie's code intelligence tools to find detailed docs on-demand: `fast_search(query="...", file_pattern="docs/**")`

Key docs: `WORKSPACE_ARCHITECTURE.md`, `TESTING_GUIDE.md`, `SEARCH_FLOW.md`, `ARCHITECTURE.md`, `INTELLIGENCE_LAYER.md`, `DEVELOPMENT.md`, `PERFORMANCE.md`, `DEPENDENCIES.md`

---

## 📝 Source-Controlled Artifacts

**Always commit these with your work:**
- `.memories/` — Goldfish checkpoints (developer memory across sessions)
- `docs/plans/` — Implementation plans

These are project knowledge, not ephemeral. If you create a checkpoint or plan, include it in your commit.

---

**Last Updated:** 2026-03-11 | **Status:** Production Ready (v5.0.0)
