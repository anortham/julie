# Julie — Development Guidelines

All AI coding agents (Claude Code, Copilot, Cursor, Windsurf, Cody, Gemini CLI, aider, etc.) must follow these guidelines.

---

## Project Overview

**Julie** is a cross-platform code intelligence server built in Rust. LSP-quality features across 36 languages via tree-sitter, Tantivy full-text search, and instant search availability.

### Key Project Facts
- **Language**: Rust (native performance, cross-platform)
- **Purpose**: Code intelligence MCP server (search, navigation, editing)
- **Architecture**: Tantivy full-text search + SQLite structured storage + KNN vector search (embeddings)
- **Mode**: Machine service (Streamable HTTP MCP + JSON API + dashboard) with stdio shim. The no-args `julie-server` serves as a lightweight stdio shim, auto-starting the background service if missing. Service control: `julie-server service status|stop|restart`.
- **Origin**: Native Rust implementation for true cross-platform compatibility
- **Crown Jewels**: 36 tree-sitter extractors with comprehensive test suites, now maintained in the external [`anortham/julie-extractors`](https://github.com/anortham/julie-extractors) repo and consumed here as a pinned git dependency

### 🏆 Current Language Support (36 - Complete)

The 36 extractors live upstream in [`anortham/julie-extractors`](https://github.com/anortham/julie-extractors) (consumed as a pinned git dep). Language and parser work — adding languages, upgrading parsers, golden fixtures, capability tests — happens there. Re-pin julie's `julie-extractors` git-dep in `Cargo.toml` and sync `SEMANTIC_INDEX_ENGINE_VERSION` (in `src/tools/workspace/indexing/engine_version.rs`) to the new tag's `EXTRACTION_CONTRACT_VERSION` to pick up a new release.

**Core Languages:** Rust, TypeScript, JavaScript, Python, Java, C#, VB.NET, PHP, Ruby, Swift, Kotlin, Scala
**Systems Languages:** C, C++, Go, Lua, Zig
**Functional:** Elixir, Erlang
**Specialized:** GDScript, Vue, Razor, QML, R, SQL, HTML, CSS, Regex, Bash, PowerShell, Dart
**Documentation:** Markdown, JSON, TOML, YAML, XML

---

## Quick Reference

```bash
cargo check                    # Type-check only (fastest compilation, no binary)
cargo build                    # Debug build
cargo build --release          # Release build (for live MCP testing)
cargo nextest run --lib <name>       # Default: one exact test first
cargo xtask test dev           # Batch gate: whole workspace, once per batch (about 20 s warm)
cargo xtask test dogfood       # Search-quality gate after search, scoring, ranking, or graph changes
cargo xtask test full          # Dev then dogfood, before merge
cargo xtask-eval search-matrix|eval  # Product-linked harnesses (Cargo alias → xtask-eval)
cargo xtask sync-plugin        # Mirror skills source → ~/source/julie-plugin (`--dry-run` to preview)
cargo xtask dev-link           # (maintainer-only) Symlink installed plugin binaries → target/release (`--dry-run` to preview)
julie-server service status    # Check running service status JSON
julie-server service restart   # Restart machine service with fresh binary
julie-server service stop      # Stop running machine service
cargo fmt                      # Format code
cargo clippy                  # Lint
```

**After cloning:** `git config core.hooksPath hooks/` — enables pre-commit hook that keeps CLAUDE.md and AGENTS.md in sync.

**Commit messages:** Use conventional commits — `feat(scope): ...`, `fix(scope): ...`, `refactor(scope): ...`

**Version bumps:** When changing the version in `Cargo.toml`, also update the version displayed on the gh-pages site (`docs/` or the site source). All three must stay in sync: `Cargo.toml`, `plugin.json` (via CI), and the gh-pages site.

**Release notes:** Every tagged release needs substantive GitHub release notes. Do not rely on the workflow's boilerplate body. Cover the user-visible changes, important fixes, dogfood and test evidence, upgrade or operational notes, and any honest caveats. If the workflow creates a generic release page, replace it with real notes using `gh release edit <tag> --notes-file <file>` before calling the release done.

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
3. **Verify the test fails** — run ONLY your specific test: `cargo nextest run --lib <test_name> 2>&1 | tail -10`
4. **Fix the bug** with minimal changes
5. **Verify the test passes** — same narrow command as step 3
6. **Ensure no regressions**: if you are the main session, run `cargo xtask test dev` once per completed batch. **If you are a subagent, SKIP this step**; the lead session handles it

See: **docs/TESTING_GUIDE.md** for comprehensive testing standards and SOURCE/CONTROL methodology.

---

## 🚨 RUNNING TESTS (USE THE XTASK RUNNER)

Three tiers exist. Each tier runs one `cargo nextest run --workspace` pass. `xtask/src/runner.rs` holds the exact command lists.

| Tier | Command | What it runs | When to use |
|------|---------|--------------|-------------|
| **Dev** | `cargo xtask test dev` | Builds `julie-server`. Runs the whole workspace except the dogfood set. Then runs the ignored `tests::cli::` tests. About 20 s warm. | Once per completed batch, and before handoff |
| **Dogfood** | `cargo xtask test dogfood` | Ensures the search-quality fixture. Then runs the dogfood set: `search_quality`, `fixtures::julie_db`, `dogfood`. About 15 s, plus 42 s when the fixture rebuilds. | After search, scoring, ranking, or graph changes |
| **Full** | `cargo xtask test full` | Dev, then dogfood. | Before merge |

The dogfood set runs at most six tests at a time. `.config/nextest.toml` sets test group `dogfood` to `max-threads = 6`. No environment variable is needed.

### 🔥 Edit Loop (Edit → Verify)

1. Run `cargo check` after each code change.
2. Run `cargo nextest run --lib <exact_test_name>` for the test you wrote. The incremental rebuild takes about 3.5 s.
3. Batch three to five edits. Then run `cargo xtask test dev` once.

The extractor dependency re-pin gate is `cargo xtask test dev`. It contains the three contract tests: `test_semantic_index_engine_version_includes_extraction_contract`, `real_world_parser_upgrade_contracts_assert_expected_outputs`, and `current_parser_release_contracts_parse_without_diagnostics`.

`cargo xtask-eval search-matrix|eval` runs the product-linked harnesses. They are investigation tools, not a substitute for `dogfood`.

### The Rules

1. **Run the narrowest test first.** `cargo nextest run --lib <name>` for one function.
2. **Run `cargo xtask test dev` once per completed batch,** not after every edit.
3. **Use raw cargo filters only to narrow a failure** that a tier reported. Example: `cargo nextest run --lib tests::tools::search`.
4. **Do not run `cargo nextest run --lib` without a filter.** That runs the whole suite.
5. **Run one cargo test command at a time.** On Windows, parallel `cargo nextest` runs fight over the same output binary (`LNK1104` linker lock error).

### 🚨 Subagent & Worker Agent Test Rules (CRITICAL)

**When running as a subagent, worker, or dispatched agent** (via subagent-driven development, worktree agents, or any delegated task):

**YOU MUST:**
- Run only the exact test you wrote: `cargo nextest run --lib <exact_test_name> 2>&1 | tail -10`
- Limit yourself to **2 test runs per change**: one to verify RED, one to verify GREEN

**YOU MUST NOT:**
- ❌ Run `cargo xtask test dev`, `dogfood`, or `full`; **the lead session runs `dev` once per batch**
- ❌ Run `cargo nextest run --lib` without a specific test filter
- ❌ Sleep, poll, or retry test commands. If a test fails, diagnose and fix, or report back
- ❌ Run tests more than twice per change cycle (red → green, done)

**Why this exists:** Six subagents that each run a broad suite start six parallel build and test processes. That grinds the machine to a halt. A targeted test takes seconds.

### Verification Ledger Contract

Plan docs use `docs/plans/verification-ledger-template.md` for verification evidence. Scope labels are `worker-red-green`, `dev`, `dogfood`, `full`, and `live`.

Reuse evidence only when the scope label matches and the commit SHA matches the current HEAD exactly. If either value differs, run the command again and record a new ledger row.

### Known Failures

**All tiers are green. A failure is a regression.** Investigate it.

(#33, resolved 2026-05-30): the workspace rebind tests (`tests::tools::workspace::global_targeting::rebind_index`, `test_manage_workspace_index_*`) used to fail **only on a polluted dev box**, never on clean CI. Root cause was **test non-hermeticity, not a product bug**: the fixtures created marker-less temp workspaces under `$TMPDIR`, so `find_workspace_root` walked up past them to a stray `/private/tmp/Cargo.toml` and resolved every workspace to `tmp_*` instead of `target_*`. Product rebind code was correct and untouched. The resolution-critical fixtures now drop a `.git` marker via `make_isolated_workspace_root` / `mark_workspace_root` (`src/tests/helpers/workspace.rs`) so resolution stops at the temp workspace. Most other temp-workspace tests still assume a clean `$TMPDIR` (as CI always has) — **do not leave stray workspace markers (`Cargo.toml`, `.git`, `.julie`) in your system temp root**, or you will get spurious local-only failures.

(The per-language extractor unit suite — every one of the 36 extractors — lives and runs in the external [`anortham/julie-extractors`](https://github.com/anortham/julie-extractors) repo.)

### Rebuilding Fixture Database

The dogfood tier indexes this repository into a git-ignored snapshot (`fixtures/databases/julie-snapshot/`, about 300 MB). The snapshot is never migrated. The tier rebuilds it when it is missing or was built for another schema or engine version. To force a rebuild:
```bash
cargo test --lib build_julie_fixture -- --ignored --nocapture
```

---

## 🚨 PROJECT ORGANIZATION STANDARDS (NON-NEGOTIABLE)

### File Size
There is no line limit on implementation or test files. The old 500-line limit (and the 600-line limit before it) existed only to keep whole-file reads small for AI agents. Julie's tools read symbols, not files, so the limit is gone. Split a file only when it mixes responsibilities, never to hit a line count.

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

### sccache (Optional, Recommended)

Install [sccache](https://github.com/mozilla/sccache) to cache compiled
artifacts across branches and clean builds:

```bash
brew install sccache
```

Then set the environment variable or add to `.cargo/config.toml`:
```bash
export RUSTC_WRAPPER=sccache
```

This helps most when switching branches (common for AI agents) and on clean
builds after `cargo clean`.

### Development Workflow
1. **Development Mode**: Always work in `debug` mode for fast iteration
2. **CLI-First Tool Testing**: Julie's CLI provides first-class autonomous dogfooding without rebuilding release binaries or restarting live MCP clients:
   - Run `cargo build` for a debug binary
   - Direct named subcommands:
     - Navigation & Search: `fast-search` (alias `search`), `fast-refs` (alias `refs`), `get-symbols` (alias `symbols`), `get-context` (alias `context`), `call-path`, `blast-radius`, `deep-dive`, `patterns`
     - Safe Editing: `edit-file` (alias `edit`)
     - Workspace: `manage-workspace` (alias `workspace`), `dashboard --foreground`
   - Generic tool runner: `./target/debug/julie-server tool <name> --params '{"key":"value"}' --json` (supports `--params`, `--params-file`, and `--params-stdin` with 16 MiB ceiling)
   - Zero-warmup discovery: `./target/debug/julie-server tools list --json` and `./target/debug/julie-server tools schema <name> --json` (instant <15ms)
   - Serial batch replay: `./target/debug/julie-server tools replay --input trace.jsonl --json`
   - Fast lexical mode: add `--semantics off` to skip all embedding checks and run purely in Tantivy/SQLite lexical mode
   - Strict semantic verification: add `--semantics required` to verify vector health (fails with exit code 4 if vectors are missing, stale, or incompatible)
   - Predictable exit codes: 0 = ok, 2 = arg error, 3 = tool error, 4 = semantics not ready, 124 = timeout, 130 = cancel
   - Clean stdout discipline: stdout is strictly JSON envelopes when `--json` is passed; all diagnostics and tracing go to stderr
   - Standalone CLI does not prove MCP serving or handler binding. Use named wrappers such as `julie-server call-path FROM TO` for quick checks before live MCP; capture stderr `julie: mode=...` for execution-path evidence.
3. **Live MCP Testing**: When ready to test the full MCP integration:
   - Agent asks user to exit Claude Code
   - User runs: `cargo build --release`
   - User restarts Claude Code (MCP client spawns new stdio server)
   - Test features in live MCP session
4. **Maintainer dev loop (`dev-link` + `dev-restart`)**: One-time and per-iteration commands that make the dev loop survive plugin installs:
   - **One-time setup**: `cargo build --release && cargo xtask dev-link` — replaces the bundled `julie-server` inside `~/.claude/plugins/cache/julie-plugin/julie/<v>/bin/<arch>/` with a symlink to `target/release/julie-server`. After this, every harness that points at the plugin (Claude Code) and every harness configured to point at `target/release/julie-server` directly (Codex CLI, OpenCode per their `~/.codex/config.toml` / `~/.config/opencode/opencode.json`) all run the same dev binary.
   - **Per-edit loop**: `cargo build --release`, then `target/release/julie-server service restart`. The machine service restarts detached with the new binary and stale discovery files are cleaned up. Running stdio shims reconnect automatically on their next request.
   - **Idempotent**: re-run `dev-link` after any plugin update; it'll relink in place. Reports `already-linked` for entries that are still pointing at the dev binary.
   - **Maintainer-only**: regular users install the plugin and never run these. Codex CLI and OpenCode users typically point their MCP `command` at a chosen path themselves; this just means dev-link mostly affects the Claude Code plugin cache. (`xtask/src/dev_workflow.rs`)
5. **🔴 Windows Binary Lock**: On Windows, the running `julie-server.exe` process (spawned by the MCP client) holds an exclusive file lock on the release binary. **Do NOT attempt `cargo build --release` while a session is active** — it will fail with "Access is denied" (os error 5). Only `cargo build` (debug) works while the release binary is running. The user must exit their MCP client (Claude Code, VS Code, etc.) before rebuilding release. On Windows, `dev-restart` is advisory and cannot help (and the file lock prevents the rebuild itself anyway); close the MCP client, rebuild, then reopen it to load the new binary.
6. **Backward Compatibility**: We don't need it (stdio MCP server, not a public API)
7. **Target User**: YOU (Claude) and other AI coding agents are the target user
   - Review code from standpoint of you being the user
   - Optimize tool output for YOU
   - Optimize functionality for YOU

---

## 🐛 Debugging & Monitoring

### 🚨 LOG LOCATIONS

Julie writes per-project logs for the machine service and standalone CLI runs:
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
- MCP/server storage: `$JULIE_HOME/indexes/{workspace_id}/facts.sqlite` + `tantivy/`
- Standalone CLI storage: `.julie/indexes/{workspace_id}/facts.sqlite` + `tantivy/`

**WORKSPACE ISOLATION HAPPENS AT FILE LEVEL, NOT QUERY LEVEL:**
- Tool receives workspace param → Routes to that checkout's `facts.sqlite` and snapshot
- The checkout store is locked to that workspace
- Tools read `ToolContext::snapshot` only

**For detailed architecture info**, use Julie's code intelligence tools:
```
fast_search(query="workspace routing", file_pattern="docs/**")
```

See: **docs/WORKSPACE_ARCHITECTURE.md** for complete details.

### Filewatcher Mutation Gate

All workspace mutations serialize through a per-workspace async mutex defined in `crates/julie-core/src/workspace/mutation_gate.rs`. Eight canonical writers — watcher event-processor, watcher repair scan, watcher repair-replay, watcher Tantivy retry, startup catch-up, force-reindex, `refresh`, and `rebuild` — all acquire `mutation_gate::acquire_gate(workspace_id)` before mutating, and threading is enforced at compile time via the `MutationGuard<'_>` proof token (gated functions take `_guard: &MutationGuard<'_>`).

**Operator signals to watch in project logs:**
- `Waited Nms for mutation gate on workspace <id>` — fires only when gate-wait exceeds 100ms. Steady-state should rarely log; long catch-ups on fresh clones may log briefly. Sustained waits >1s for steady-state operation indicate a writer holding the gate too long; investigate whether a repair scan, catch-up, or force-reindex is leaking the guard.
- `Watcher: extracted N symbols, M identifiers, K relationships from <path> (<lang>)` — successful index of a watcher event.
- `Watcher: <path> unchanged (hash match), skipping re-index` — file written but content identical; expected on save-without-change.

The previous lossy `pause()` / `resume()` mechanism that silently dropped events while catch-up ran has been removed; events now queue unconditionally and the queue processor blocks on the gate. See `docs/plans/2026-05-06-filewatcher-pause-architecture.md` for the full design.

---

## 🏗️ Architecture Principles (Brief)

### Core Design Decisions
1. **Tantivy Search**: Code-aware full-text search with CamelCase/snake_case tokenization + English stemming
2. **Graph Centrality Ranking**: Pre-computed reference scores boost well-connected symbols in search results
3. **Per-Workspace Isolation**: Each workspace gets its own db/tantivy in `indexes/{workspace_id}/`. MCP sessions share `$JULIE_HOME/indexes/` and `$JULIE_HOME/registry.db`; standalone CLI runs use project-local `.julie/indexes/`.
   - The machine service serves Streamable HTTP at `/mcp`, JSON API at `/api/<tool>`, and dashboard at `/`.
   - The no-args `julie-server` runs the stdio shim, forwarding JSON-RPC to the service over localhost.
   - One machine service process (`julie-server service`) owns every workspace index. Its handler for a checkout is the only writer for that checkout: it runs the watcher, startup catch-up, and Tantivy writes. There is no leader election, no per-workspace lock file, and no read-only session. `RuntimeFactory` binds one handler per `(root, index_root)`.
   - Durable files per checkout: `$JULIE_HOME/indexes/<id>/facts.sqlite` (plus `-wal`/`-shm`) and `$JULIE_HOME/indexes/<id>/tantivy/`. Per machine: `$JULIE_HOME/registry.db`, runtime discovery `service.json`, and the OS-held singleton `service.lock`. Nothing else (no `embedding-host.*` files).
   - `facts.sqlite` is never migrated. A schema or `SEMANTIC_INDEX_ENGINE_VERSION` mismatch deletes `indexes/<id>/` and reindexes. `registry.db` keeps its own small migrations.
   - `manage_workspace open` on a checkout whose `git rev-parse --git-common-dir` matches a registered workspace seeds from that sibling: it copies blobs and fact rows by hash, extracts missing blobs, rebuilds `tantivy/`, and runs the incremental scan.
   - `registry.db` tracks known workspaces, cleanup events, codehealth snapshots, and tool calls.
4. **Native Rust Core**: No FFI, no CGO — core indexing/search has zero external dependencies
5. **Tree-sitter Native**: Direct Rust bindings for all language parsers
6. **SQLite Storage**: Blob-keyed facts in `facts.sqlite` (symbols, identifiers, relationships, types, vectors)
7. **Single Binary + Native Sidecar**: Core features work standalone; semantics run through the native `julie-semantic-sidecar` binary. There is no Python runtime.
8. **Semantic Embeddings + brute-force vector scan**: Symbol embeddings from the native sidecar stored as a `vectors` table in `facts.sqlite`, enabling semantic similarity for `deep_dive` (related symbols) and `fast_refs` (zero-reference fallback). Two threshold tiers: symbol-to-symbol (0.5) and query-to-symbol (0.2). The machine service spawns the sidecar directly as a child process speaking NDJSON over stdio with no broker. `JULIE_EMBEDDING_PROVIDER` accepts `auto` (native when the sidecar binary is found, else none), `native`, or `none`; cargo sets `none` for every test binary via `.cargo/config.toml`.
9. **Instant Search**: Tantivy index available immediately after indexing
10. **Relative Unix-Style Path Storage**: All file paths stored as relative with `/` separators
11. **Language-Agnostic Everything**: See below — this is critical

### 🔴 CRITICAL: Language-Agnostic Design (Non-Negotiable)

**Julie supports 36 languages and indexes ANY codebase.** All scoring, ranking, filtering, path analysis, and heuristics MUST work across all project layouts — not just Rust or Julie's own directory structure.

#### Feature parity: every feature, every language that supports the concept

**The 36 languages are first-class peers. We do NOT pick a favorite language — or a convenient 3 — and make them better than the rest.** When you build any extraction or intelligence feature (a new symbol kind, an annotation class, type-argument capture, literal capture, a relationship edge, an enrichment), the target is **every language whose grammar expresses the underlying concept** — not "the language I'm testing with," not "the consumer who asked for it," not "the easy ones first and the rest never."

**A feature is not "done" when only some languages have it.** "Done for C#/TS/Python" is not done — it is a partial rollout that must be tracked to completion across all applicable languages. Before declaring any cross-language feature complete, **enumerate all 36 languages and classify each: implemented, or verified-not-applicable.** There is no third "we'll get to it" bucket masquerading as done.

**The only legitimate exclusion is genuine absence of the concept** (e.g. generic type arguments in Bash, inheritance in JSON). That exclusion is a **positive claim that must be verified** — check the grammar's `node-types.json` or the extractor source, cite the evidence, and record it. Never assume a language lacks a feature; that is exactly how breadth silently rots. (See the global rule "Negative claims need positive verification.")

- ❌ "I implemented it for C#/TS/Python; the others can come later" — that is silently shrinking a 36-language feature to 3.
- ❌ "Language X probably doesn't have this" — verify against the grammar before excluding.
- ❌ Scoping a broad feature to one consumer's corpus and calling the feature finished.
- ✅ Build the language-agnostic infrastructure once, then add every applicable language (a per-grammar reader + a test), driving the "implemented vs verified-n/a" ledger to 100%.
- ✅ When effort forces phasing, phase **explicitly and visibly** (a tracked checklist of remaining languages), never by quietly stopping at the convenient subset.

If the full set is genuinely too large for one change, say so out loud and keep a checklist of the remaining languages — do not let "the 3 that mattered to today's task" become the permanent state. Scope reduction across languages is the owner's call, surfaced explicitly, never a silent default.


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
| Hooks | `.claude/hooks/hooks.json` (dev-only); its session-start hook prints `JULIE_AGENT_INSTRUCTIONS.md` | `hooks/hooks.json` (distributed); its session-start hook prints the same file |
| Binaries | `cargo build --release` | `bin/archives/*.tar.gz\|*.zip` |
| MCP server | `src/` (Rust source) | `hooks/run.cjs` (launch script) |
| Agent instructions | `JULIE_AGENT_INSTRUCTIONS.md` | `JULIE_AGENT_INSTRUCTIONS.md` (mirrored by `cargo xtask sync-plugin`) |

### How distribution works

On release, a GitHub Actions workflow in julie-plugin (`update-binaries.yml`):
1. Downloads release binaries from `anortham/julie` 
2. Clones the julie repo at the release tag
3. Copies skills from `.claude/skills/` (hardcoded list in the workflow) and `JULIE_AGENT_INSTRUCTIONS.md`
4. Updates version in `plugin.json`, `package.json`, `marketplace.json`
5. Commits and tags

### When you add/modify plugin content

**Adding a new skill:**
1. Create `.claude/skills/<name>/SKILL.md` here in julie
2. Run `cargo xtask sync-plugin` to mirror skills and `JULIE_AGENT_INSTRUCTIONS.md` source → plugin (full skill mirror; removes plugin-only skill files). `--dry-run` previews changes.
3. Add `<name>` to the `for skill in ...` list in `julie-plugin/.github/workflows/update-binaries.yml`
4. Update the skill count check in the same workflow

**Modifying hooks:**
- `.claude/hooks/hooks.json` in julie is dev-only (applies when working IN the julie repo)
- `hooks/hooks.json` in julie-plugin is what gets distributed to users
- These are intentionally separate; edit the plugin repo's copy for distribution changes
- `cargo xtask sync-plugin` mirrors skills and `JULIE_AGENT_INSTRUCTIONS.md`; hooks stay separate and it reports their divergence.

**Modifying agent instructions:**
- Edit `JULIE_AGENT_INSTRUCTIONS.md` here in julie (source of truth)
- `cargo xtask sync-plugin` mirrors it to the plugin; both session-start hooks print it at session startup

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

**Last Updated:** 2026-09-11 | **Status:** Phase 6a (compact output, paging, guidance hook)
