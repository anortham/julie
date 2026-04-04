# Julie — Development Guidelines

All AI coding agents (Claude Code, Copilot, Cursor, Windsurf, Cody, Gemini CLI, aider, etc.) must follow these guidelines.

---

## Project Overview

**Julie** is a cross-platform code intelligence server built in Rust. LSP-quality features across 33 languages via tree-sitter, Tantivy full-text search, and instant search availability.

### Key Project Facts
- **Language**: Rust (native performance, cross-platform)
- **Purpose**: Code intelligence MCP server (search, navigation, editing)
- **Architecture**: Tantivy full-text search + SQLite structured storage + KNN vector search (embeddings)
- **Mode**: Stdio MCP server (JSON-RPC over stdin/stdout) + optional background daemon for shared workspaces
- **Origin**: Native Rust implementation for true cross-platform compatibility
- **Crown Jewels**: 33 tree-sitter extractors with comprehensive test suites

### 🏆 Current Language Support (33 - Complete)

**Core Languages:** Rust, TypeScript, JavaScript, Python, Java, C#, PHP, Ruby, Swift, Kotlin, Scala
**Systems Languages:** C, C++, Go, Lua, Zig
**Functional:** Elixir
**Specialized:** GDScript, Vue, Razor, QML, R, SQL, HTML, CSS, Regex, Bash, PowerShell, Dart
**Documentation:** Markdown, JSON, TOML, YAML

---

## Quick Reference

```bash
cargo build                    # Debug build (fast iteration)
cargo build --release          # Release build (for live MCP testing)
cargo xtask test dev           # Default test tier — run after every change
cargo fmt                      # Format code
cargo clippy                   # Lint
```

**After cloning:** `git config core.hooksPath hooks/` — enables pre-commit hook that keeps CLAUDE.md and AGENTS.md in sync.

**Commit messages:** Use conventional commits — `feat(scope): ...`, `fix(scope): ...`, `refactor(scope): ...`

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
3. **Verify the test fails** — run ONLY your specific test: `cargo test --lib <test_name> 2>&1 | tail -10`
4. **Fix the bug** with minimal changes
5. **Verify the test passes** — same narrow command as step 3
6. **Ensure no regressions** — if you're the main session, run `cargo xtask test dev`. **If you're a subagent, SKIP this step** — the orchestrator handles it

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

### Known Pre-Existing Failures

**All tiers are currently green.** If a test fails, it's a real regression, not a known issue. Investigate it.

(Previous known failures in `core-embeddings` and `workspace_init` were resolved as of 2026-03-19.)

### Why Dogfood Is Slow

The `search_quality` bucket loads a **100MB SQLite fixture**, backfills a Tantivy index from it, and runs real searches. It is a regression guard, not a quick unit-tier pass.

### The Rules

1. **Default to `cargo xtask test dev` after normal changes.** This is the ONLY default. No exceptions.
2. **Escalate with xtask tiers instead of inventing ad hoc canonical commands.**
3. **Use raw cargo filters only to narrow failures** after an xtask tier reports a *new* failure. Not as a shortcut to avoid running xtask.
4. **Do not casually run `cargo test --lib` as the default workflow.** Even if you "know" which tests are affected — run xtask first, then narrow if needed.
5. **Run one test command at a time.** On Windows, parallel `cargo test` invocations fight over the same output binary (`LNK1104` linker lock error). Never launch multiple test runs concurrently.

### 🚨 Subagent & Worker Agent Test Rules (CRITICAL)

**When running as a subagent, worker, or dispatched agent** (e.g., via subagent-driven development, worktree agents, or any delegated task):

**YOU MUST:**
- Run ONLY the specific test you wrote: `cargo test --lib <exact_test_name> 2>&1 | tail -10`
- Use the narrowest possible test filter for your changed area
- Limit yourself to **2 test runs per fix**: once to verify RED, once to verify GREEN

**YOU MUST NOT:**
- ❌ Run `cargo xtask test dev` or any xtask tier — **the orchestrating session handles regression checks**
- ❌ Run `cargo test --lib` without a specific test filter — this runs the ENTIRE suite
- ❌ Run `cargo test` with broad module filters when a specific test name will do
- ❌ Sleep, poll, or retry test commands — if a test fails, diagnose and fix or report back
- ❌ Run tests more than twice per change cycle (red → green, done)

**Why this exists:** Multiple subagents each running the full suite creates 6+ parallel compilation/test processes that grind the machine to a halt. A targeted test takes ~3 seconds. `cargo xtask test dev` takes ~90 seconds. Six of them in parallel = unusable system for 10+ minutes.

**The contract:** Subagents run narrow targeted tests. The orchestrating session runs `cargo xtask test dev` once per batch of completed changes. This is not optional.

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

## 🚨 PROJECT ORGANIZATION STANDARDS (NON-NEGOTIABLE)

### File Size Limits
**MANDATORY**: No implementation file shall exceed **500 lines**.

- Implementation files: **≤ 500 lines** (target; some legacy files exceed this — refactor when touching them)
- Test files: **≤ 1000 lines** (acceptable for comprehensive test suites)
- **New files MUST respect these limits; existing violations should be refactored opportunistically**

**Rationale**: Files larger than 500 lines:
- Cannot be fully read by AI agents (token limits)
- Are difficult to understand and maintain
- Violate single responsibility principle

### Test Organization
**All tests in `src/tests/`, all fixture data in `fixtures/`**

```
src/tests/              # ALL test code (.rs files with #[test] functions)
├── fixtures/           # Test fixture builder code (e.g., julie_db.rs)
├── core/               # Core module tests
├── tools/              # Tool-specific tests
├── integration/        # Integration tests
└── ...

fixtures/               # ALL test data files (SOURCE/CONTROL files, samples)
├── editing/           # SOURCE/CONTROL for editing tools
└── real-world/        # Real-world code samples
```

**Rules:**
- ✅ ALL test code goes in `src/tests/`
- ✅ ALL test data/fixture files go in `fixtures/`
- ✅ Test fixture *builder code* (Rust helpers) goes in `src/tests/fixtures/`
- ⚠️ PREFER no inline `#[cfg(test)] mod tests` in implementation files (some legacy exceptions exist)

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
3. **🔴 Windows Binary Lock**: On Windows, the running `julie-server.exe` process (spawned by the MCP client) holds an exclusive file lock on the release binary. **Do NOT attempt `cargo build --release` while a session is active** — it will fail with "Access is denied" (os error 5). Only `cargo build` (debug) works while the release binary is running. The user must exit their MCP client (Claude Code, VS Code, etc.) before rebuilding release.
3. **Backward Compatibility**: We don't need it (stdio MCP server, not a public API)
4. **Target User**: YOU (Claude) and other AI coding agents are the target user
   - Review code from standpoint of you being the user
   - Optimize tool output for YOU
   - Optimize functionality for YOU

---

## 🐛 Debugging & Monitoring

### 🚨 LOG LOCATIONS

Julie has TWO log locations depending on mode:

**Daemon mode logs** (the daemon process, shared across sessions):
```bash
# Daemon log (rotated daily)
tail -f ~/.julie/daemon.log.$(date +%Y-%m-%d)

# Check watcher activity
grep "Background task processing" ~/.julie/daemon.log.$(date +%Y-%m-%d)

# Check watcher creation
grep "File watcher created" ~/.julie/daemon.log.$(date +%Y-%m-%d)

# View recent errors
tail -100 ~/.julie/daemon.log.$(date +%Y-%m-%d) | grep -i error

# List all daemon log files
ls -lh ~/.julie/daemon.log.*
```

**Project-level logs** (per-project, stdio mode or session-specific):
```bash
# Project logs
tail -f .julie/logs/julie.log.$(date +%Y-%m-%d)

# Check indexing progress
tail -50 .julie/logs/julie.log.$(date +%Y-%m-%d) | grep -E "Tantivy|indexing|Background"

# List all log files
ls -lh .julie/logs/
```

---

## 🔥 WORKSPACE ARCHITECTURE (Overview)

**Each workspace has SEPARATE PHYSICAL FILES:**
- Primary workspace: `.julie/indexes/{workspace_id}/db/symbols.db` + `tantivy/`
- Reference workspace: `.julie/indexes/{ref_workspace_id}/db/symbols.db` + `tantivy/`

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

## 🏗️ Architecture Principles (Brief)

### Core Design Decisions
1. **Tantivy Search**: Code-aware full-text search with CamelCase/snake_case tokenization + English stemming
2. **Graph Centrality Ranking**: Pre-computed reference scores boost well-connected symbols in search results
3. **Per-Workspace Isolation**: Each workspace gets own db/tantivy in `indexes/{workspace_id}/`. In stdio mode: under `{project}/.julie/indexes/`. In daemon mode: under `~/.julie/indexes/` (shared across all sessions).
   - **Daemon mode** (`julie daemon`): starts a background process that shares workspace indexes and a single embedding provider across MCP sessions. Enables reference workspaces, symbol/file count snapshots, and tool call history. Registry lives in `~/.julie/daemon.db` (DaemonDatabase). The shared `EmbeddingService` ensures one sidecar process serves all sessions. Workspace operations (add, refresh, stats) require daemon mode; they return helpful errors in stdio mode.
   - **Adapter mode** (default): when `julie-server` is run without arguments, it auto-starts the daemon (if not already running) and forwards stdio JSON-RPC to the daemon via IPC. This is the standard MCP client integration path.
   - **Stale binary auto-restart**: the daemon captures its binary's mtime at startup. On each new connection and session disconnect, it compares the current binary mtime. If the daemon is idle (0 sessions) when a stale binary is detected, it shuts down immediately before accepting the connection. If sessions are active, it sets `restart_pending` and exits after the last session disconnects. The adapter restarts it automatically with the new binary.
   - **Catch-up indexing on session connect**: when a session connects and the workspace is already indexed, a background staleness check runs (mtime comparison, then blake3 hash comparison via `filter_changed_files`). Files that changed while the daemon was down are incrementally re-indexed without requiring `force: true`. This closes the gap between the file watcher (which only sees live events) and daemon restarts.
   - **Stdio mode**: single session, per-project indexes in `.julie/`, no registry persistence. Still available but not the default path.
4. **Native Rust Core**: No FFI, no CGO — core indexing/search has zero external dependencies
5. **Tree-sitter Native**: Direct Rust bindings for all language parsers
6. **SQLite Storage**: Symbols, identifiers, relationships, types, files
7. **Single Binary + Optional Sidecar**: Core features work standalone; GPU-accelerated embeddings use a managed Python sidecar (auto-provisioned via `uv`)
8. **Semantic Embeddings + KNN Vector Search**: Symbol embeddings via Python sidecar (sentence-transformers + PyTorch) stored in SQLite, enabling semantic similarity for `deep_dive` (related symbols) and `fast_refs` (zero-reference fallback). GPU support: CUDA (NVIDIA, auto-detected), DirectML (AMD/Intel via torch-directml), MPS (Apple Silicon). Default model: CodeRankEmbed (768d). Two threshold tiers: symbol-to-symbol (0.5) and query-to-symbol (0.2). In daemon mode, the embedding provider is shared via `EmbeddingService` (not per-workspace).
9. **Instant Search**: Tantivy index available immediately after indexing
10. **Relative Unix-Style Path Storage**: All file paths stored as relative with `/` separators
11. **Language-Agnostic Everything**: See below — this is critical

### 🔴 CRITICAL: Language-Agnostic Design (Non-Negotiable)

**Julie supports 33 languages and indexes ANY codebase.** All scoring, ranking, filtering, path analysis, and heuristics MUST work across all project layouts — not just Rust or Julie's own directory structure.

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

Key docs: `WORKSPACE_ARCHITECTURE.md`, `TESTING_GUIDE.md`, `SEARCH_FLOW.md`, `ARCHITECTURE.md`, `INTELLIGENCE_LAYER.md`, `DEVELOPMENT.md`, `DEPENDENCIES.md`

### 🔴 CRITICAL: Web Search Before Writing Harness Documentation

**ALWAYS use web search to verify current paths, formats, and configuration before writing documentation about AI coding harnesses** (VS Code/Copilot, Cursor, Windsurf, Gemini CLI, Codex CLI, OpenCode, etc.). The ecosystem changes rapidly — skill directories, config file formats, and settings locations shift between versions. Never rely on training data or prior knowledge for harness-specific instructions. Verify first, write second.

Key things that change frequently:
- Skill/rules directory paths (e.g., `.cursor/rules/` vs `.cursorrules`)
- Config file formats (e.g., Cursor's `.mdc` vs standard Markdown)
- MCP server configuration syntax per harness
- Which harnesses read `.claude/skills/` natively vs requiring copies

---

## 🔌 Plugin Distribution (`julie-plugin`)

Julie is distributed as a Claude Code plugin via a separate repo: `~/source/julie-plugin` (GitHub: `anortham/julie-plugin`). The plugin repo is a **pure distribution artifact**; all authoritative source lives here in the julie repo.

### What lives where

| Content | Source (julie) | Distribution (julie-plugin) |
|---------|---------------|---------------------------|
| Skills | `.claude/skills/<name>/SKILL.md` | `skills/<name>/SKILL.md` |
| Hooks | `.claude/hooks/hooks.json` (dev-only) | `hooks/hooks.json` (distributed) |
| Binaries | `cargo build --release` | `bin/archives/*.tar.gz\|*.zip` |
| MCP server | `src/` (Rust source) | `hooks/run.cjs` (launch script) |
| Agent instructions | `JULIE_AGENT_INSTRUCTIONS.md` | `hooks/session-start.cjs` (injected at startup) |

### How distribution works

On release, a GitHub Actions workflow in julie-plugin (`update-binaries.yml`):
1. Downloads release binaries from `anortham/julie` 
2. Clones the julie repo at the release tag
3. Copies skills from `.claude/skills/` (hardcoded list in the workflow)
4. Updates version in `plugin.json`, `package.json`, `marketplace.json`
5. Commits and tags

### When you add/modify plugin content

**Adding a new skill:**
1. Create `.claude/skills/<name>/SKILL.md` here in julie
2. Copy it manually to `~/source/julie-plugin/skills/<name>/SKILL.md` for immediate availability
3. Add `<name>` to the `for skill in ...` list in `julie-plugin/.github/workflows/update-binaries.yml`
4. Update the skill count check in the same workflow

**Modifying hooks:**
- `.claude/hooks/hooks.json` in julie is dev-only (applies when working IN the julie repo)
- `hooks/hooks.json` in julie-plugin is what gets distributed to users
- These are intentionally separate; edit the plugin repo's copy for distribution changes

**Modifying agent instructions:**
- Edit `JULIE_AGENT_INSTRUCTIONS.md` here in julie (source of truth)
- The plugin's `hooks/session-start.cjs` reads and injects this content at session startup

**Adding a new MCP tool:**
1. Implement in `src/tools/` and register in `src/handler.rs`
2. Update `JULIE_AGENT_INSTRUCTIONS.md` with the tool description
3. Update `.claude/settings.local.json` to allowlist the tool
4. If the tool needs a skill, create one (see above)
5. On release, the new binary is automatically distributed

---

## 📝 Source-Controlled Artifacts

**Always commit these with your work:**
- `.memories/` — Goldfish checkpoints (developer memory across sessions)

These are project knowledge, not ephemeral. If you create a checkpoint or plan, include it in your commit.

---

**Last Updated:** 2026-04-04 | **Status:** Production Ready (v6.6.0 — filewatcher gitignore leak fix, edit_file and edit_symbol tools with trimmed-line fuzzy matching, DMP length correction, unified diff hunk headers; markdown section line range fix, plugin distribution docs, daemon log location documented)
