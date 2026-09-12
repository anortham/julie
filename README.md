# Julie

**[Website](https://anortham.github.io/julie/)** · **[Installation](#installation)** · **[Tools](#tools-12)** · **[External Extract](#external-extract-host-integration)** · **[Skills](#skills)** · **[36 Languages](#supported-languages-36)**

A cross-platform code intelligence server built in Rust, providing LSP-quality features across 36 programming languages via the Model Context Protocol (MCP).

## Retired

Julie is retired as of 2026-07-28. No further development ships here, and the open items in
[TODO.md](TODO.md) are wontfix by policy.
[Miller](https://github.com/anortham/miller) (v1.14.0+) is the supported replacement; its
[migration guide](https://github.com/anortham/miller/blob/main/docs/migration-from-julie.md) covers the
tool-by-tool mapping, verification steps, and rollback.

**v7.18.1 is a maintenance release, not a resumption of development.** The pinned extractor is now
julie-extractors v2.34.3 so the final release line picks up upstream extraction work, including
narrower Python, Scala, and Elixir test-role evidence. Nothing further is planned.

Support window: existing releases and this repository remain available as-is, indefinitely, with no
support. Keep Julie configured as a rollback option only for as long as your own Miller verification
needs it. Extraction development continues upstream in
[julie-extractors](https://github.com/anortham/julie-extractors).

## Why Julie?

Without code intelligence, AI agents waste most of their context window just *reading* code. A 500-line file costs ~2,000 tokens. Understanding a module means reading every file in it. Tracing a function's callers means grepping the entire codebase. Every token spent reading raw files is a token unavailable for reasoning, planning, and generating code — and once the context window fills up, the session is over.

Julie parses your codebase once with tree-sitter, builds a searchable index with a full reference graph, and returns only what the agent actually needs. Fewer tokens per operation means more operations before hitting context limits — and longer, more productive coding sessions:

| Task | Without Julie | With Julie | Savings |
|------|--------------|------------|---------|
| Understand a file's API | Read whole file (~3,000 tokens) | `get_symbols` structure mode (~300 tokens) | ~90% |
| Find a function definition | Grep + read matching files (~4,000+ tokens) | `fast_search` definitions mode (~100 tokens) | ~97% |
| Investigate before modifying | Read file + grep refs (~5,000+ tokens) | `deep_dive` overview (~200 tokens) | ~96% |
| Orient on a new area | Read 5-10 files (~10,000+ tokens) | `get_context` with token budgeting (~800 tokens) | ~92% |

The key difference from simpler code indexing tools: Julie doesn't just extract symbols — it builds a **reference graph** so agents can navigate code relationships (who calls this function? what does it call? what types flow through it?) without reading files at all.

## Features

- **Fast symbol search** with code-aware tokenization (CamelCase/snake_case splitting, stemming, <5ms)
- **Cross-language code navigation** (go-to-definition, find-references) across 36 languages
- **Test-aware search** — automatic test detection across all 36 languages with smart filtering (`exclude_tests`)
- **AST-aware refactoring** with workspace-wide rename and dry-run preview
- **Operational metrics** — per-tool timing, context efficiency tracking, "bytes NOT injected" headline metric
- **Multi-workspace support** for indexing and searching related codebases
- **Machine service + stdio shim** — single background service per machine serving Streamable HTTP at `/mcp`, JSON API at `/api/<tool>`, and dashboard at `/`, with a zero-config stdio shim
- **One writer per checkout** — the service's handler for a checkout is its only index writer; sessions share the service's memory and caches
- **Shared registry and indexes** — `$JULIE_HOME/registry.db` plus `$JULIE_HOME/indexes/` keep related workspaces available across sessions
- **Unified RequestEngine & Complete CLI** — All 12 tools operate as first-class CLI subcommands and via generic `tool <name>` with identical access classification, workspace binding, and safety checks as MCP. Includes zero-warmup discovery (`tools list`, `tools schema`) and serial batch replay (`tools replay`).
- **Date-Versioned MCP Protocol `2026-07-28`** — Supports modern Model Context Protocol date-versioned revision `2026-07-28` as primary with direct first-message `tools/call` without handshake, alongside full backward compatibility for `2025-11-25`.

### Performance Characteristics

- Search latency: <5ms (Tantivy full-text search)
- Memory usage: <100MB typical workload
- Startup time: <2s (database + Tantivy indexing)
- Single binary server; semantics run through the native `julie-semantic-sidecar` (no Python runtime)

**Incremental Updates**: Only changed files are re-indexed, typically completing in 3-15 seconds.

### Embeddings

Julie uses embeddings for semantic search, related symbol discovery, and intelligent code navigation. Semantics run through the native `julie-semantic-sidecar` binary; there is no Python runtime. When the sidecar binary is not found, keyword search and code navigation still work and embedding-backed features stay disabled.

#### Advanced configuration

- `JULIE_HOME`: relocate shared registry state and workspace indexes (default: `~/.julie`). Must be an absolute path; empty or relative values are rejected. Existing installs upgrade in place — set this only if you want to move Julie's storage to another drive or path. See `docs/OPERATIONS.md` for the migration checklist.
- `JULIE_NATIVE_SIDECAR_PROGRAM`: explicit path to the `julie-semantic-sidecar` binary (default: next to `julie-server`, then `PATH`).
- `JULIE_NATIVE_SIDECAR_MODEL`: native sidecar model id.

## Supported Languages (36)

**Core:** Rust, TypeScript, JavaScript, Python, Java, C#, VB.NET, PHP, Ruby, Swift, Kotlin, Scala

**Systems:** C, C++, Go, Lua, Zig

**Functional:** Elixir, Erlang

**Specialized:** GDScript, Vue, QML, R, Razor, SQL, HTML, CSS, Regex, Bash, PowerShell, Dart

**Documentation:** Markdown, JSON, TOML, YAML, XML

## Installation

Every install path runs the same launcher and the same machine service. The [`julie-plugin`](https://github.com/anortham/julie-plugin) repo carries the release archives, the skills, and one install path per harness. `hooks/run.cjs` extracts the archive on first use and starts `julie-server`, which is the stdio shim that auto-starts the machine service. You need Node.js 22.5 or newer for the launcher. You do not need a Rust toolchain.

| Harness | Install |
|---------|---------|
| **Claude Code** | `/plugin marketplace add anortham/julie-plugin` then `/plugin install julie@julie-plugin` |
| **Codex** | `codex plugin marketplace add anortham/julie-plugin` then `codex plugin add julie@julie-plugin` |
| **Antigravity** | `agy plugin install https://github.com/anortham/julie-plugin` |
| **OpenCode** | Clone the plugin repo, run `node bin/install-opencode.cjs`, paste the printed `opencode.json` block |
| **Hermes** | Clone the plugin repo, add the `mcp_servers.julie` block to `~/.hermes/config.yaml` |
| **Cursor** | Clone the plugin repo, add the `mcpServers.julie` block to `~/.cursor/mcp.json` |

### Claude Code

```bash
/plugin marketplace add anortham/julie-plugin
/plugin install julie@julie-plugin
```

The plugin registers the MCP server and the skills. Do not also run `claude mcp add`.

### Codex

```bash
codex plugin marketplace add anortham/julie-plugin
codex plugin add julie@julie-plugin
```

The plugin adds the server, skills, and session hooks. Run `codex`, open `/hooks`, and trust the two Julie hooks. The launch directory does not select a workspace. Pass an absolute project path or registered workspace ID as `workspace` on every search, navigation, and editing call.

### Antigravity

```bash
agy plugin install https://github.com/anortham/julie-plugin
```

The plugin adds the server and skills. The launch directory does not select a workspace. Pass an absolute project path or registered workspace ID as `workspace` on every search, navigation, and editing call.

### OpenCode

```bash
git clone https://github.com/anortham/julie-plugin.git
node julie-plugin/bin/install-opencode.cjs
```

The installer adds the skills and prints this block for `~/.config/opencode/opencode.json` (global) or `<repo>/opencode.json` (project):

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "julie": {
      "type": "local",
      "command": ["node", "/absolute/path/to/julie-plugin/hooks/run.cjs"],
      "enabled": true
    }
  }
}
```

OpenCode expects `command` as an array. The env key is `environment`, not `env`.

### Hermes

Clone the plugin repo, then add this block to `~/.hermes/config.yaml`:

```yaml
mcp_servers:
  julie:
    command: node
    args: ["/absolute/path/to/julie-plugin/hooks/run.cjs"]
```

### Cursor

Clone the plugin repo, then add this block to `~/.cursor/mcp.json` (global) or `.cursor/mcp.json` (project):

```json
{
  "mcpServers": {
    "julie": {
      "command": "node",
      "args": ["/absolute/path/to/julie-plugin/hooks/run.cjs"]
    }
  }
}
```

### Optional: Web Research

To enable the `/web-research` skill for fetching and indexing web content, download the latest binary from [browser39 releases](https://github.com/alejandroqh/browser39/releases) and add it to your PATH.

This is optional. All other Julie features work without it.

<a id="manual-install"></a>
### Manual

Use this path for any other MCP client, or when you do not want the plugin.

1. Download the archive for your platform from [GitHub releases](https://github.com/anortham/julie/releases). Targets are macOS Apple Silicon, macOS Intel, Linux x86_64, and Windows x86_64.
2. Extract it. Each archive holds `julie-server` and `julie-semantic-sidecar` side by side. Keep them together so semantic search works.
3. Register `julie-server` with no arguments as a stdio MCP server:

```json
{
  "mcpServers": {
    "julie": {
      "type": "stdio",
      "command": "/absolute/path/to/julie-server",
      "args": []
    }
  }
}
```

4. Check the service:

```bash
julie-server service status
```

`julie-server` with no arguments is a byte-forwarding shim. It starts the machine service if it is not running. On Windows, use backslashes in the command path: `"command": "C:\\path\\to\\julie-server.exe"`.

To build from source instead of downloading:

```bash
git clone https://github.com/anortham/julie.git
cd julie
cargo build --release
```

**Workspace targeting:** the shim only forwards MCP traffic; its working directory and `JULIE_WORKSPACE` do not select a checkout. Every search, navigation, and editing call must pass `workspace` as an absolute project path or registered workspace ID. Call `manage_workspace(operation="open", path="/absolute/project")` once to register a checkout and use the returned ID. Other `manage_workspace` operations use `workspace_id`; global `list` and `status` need neither selector.

<a id="available-env-options"></a>
**Available env options:**

| Variable | Values | Default | Notes |
|----------|--------|---------|-------|
| `JULIE_WORKSPACE` | Absolute project path | CLI working directory | CLI startup hint only; the MCP shim ignores it. |
| `JULIE_EMBEDDING_PROVIDER` | `auto`, `native`, `none` | `auto` | Selects embedding backend. `auto` resolves to `native` when the `julie-semantic-sidecar` binary is found, else no embeddings. |
| `JULIE_NATIVE_SIDECAR_PROGRAM` | Path to `julie-semantic-sidecar` | next to `julie-server`, then `PATH` | Explicit native sidecar binary. |
| `JULIE_NATIVE_SIDECAR_MODEL` | Native sidecar model id | sidecar default | Native sidecar model. |
| `JULIE_EMBEDDING_STRICT_ACCEL` | `1` | unset | Disable embeddings entirely when no GPU is available. |

**First Use / Verify:**

Julie indexes a checkout when you open it by absolute path. Run `manage_workspace(operation="open", path="/absolute/project")`, then pass the returned ID as `workspace` on search, navigation, and editing calls, or as `workspace_id` on other `manage_workspace` operations. First indexing may take a few seconds on small projects and longer on large repos; later sessions reuse the cached index and file watcher updates.

## Tools (10)

### Search & Navigation

- `fast_search` - Full-text code search with code-aware tokenization
  - Content search (grep-style line matches) or definition search (symbol names with signatures)
  - `regions="comment,doc_comment"` limits content results to extractor-provided source regions; accepted kinds are `comment`, `doc_comment` (alias `docstring`), `string_literal`, and `embedded`
  - Definition search promotes exact symbol matches with kind, visibility, and signature
  - <5ms search latency with CamelCase/snake_case splitting, English stemming
  - Automatic OR-fallback when strict AND returns zero results
  - `exclude_tests` parameter for filtering test symbols from results
  - Language and file pattern filtering
- `get_context` - Token-budgeted context for a concept or task
  - Returns relevant code subgraph with pivots (full code) and neighbors (signatures)
  - Pipeline: search → centrality ranking → graph expansion → adaptive token allocation → formatted output
  - Adaptive budget: few results → deep context, many results → broad overview
  - Use at the start of a task for area-level orientation
- `deep_dive` - Progressive-depth symbol investigation
  - Overview (~200 tokens), context (~600 tokens), or full (~1500 tokens) detail levels
  - Kind-aware: functions show callers/callees/types, traits show implementations, structs show fields/methods
  - Prints persisted extractor complexity counts (`decisions`, `loops`, `nesting`, `params`, `lines`) when the selected symbol has a metric
  - Includes test locations with quality tiers and centrality scores
  - Identifier fallback for references that relationships miss
- `patterns` - Query typed structural facts maintained by `julie-extractors`
  - List observed IDs: `julie-server patterns --workspace . --standalone --json`
  - Search by exact pattern or substring: `julie-server patterns --operation search --pattern-id http.client_request.v1 --workspace . --standalone --json`
  - Summarize with `--operation summary`, `--group-by language_pattern_capture|file|directory`, and optional `--facet`
  - Filter with `--path`, `--language`, and repeatable `--where key=value`; results are bounded by `--limit`

Julie persists the upstream `source_regions`, `structural_facts`, and
`complexity_metrics` domains in the same atomic file write as symbols and
relationships. Those typed tables power region search, `patterns`, and
`deep_dive` complexity output respectively.
- `fast_refs` - Find all references to a symbol with structured output
- `call_path` - Trace one shortest call-graph path between two symbols
  - Answers "how does A reach B?" in a single call
  - Walks calls, instantiations, and overrides only, returns the hop chain with edge kinds
  - Handles disconnected pairs with a clear "no path" result
  - Supports `from_file_path` / `to_file_path` disambiguation for shared names
  - CLI: `julie-server call-path "LoginButton::onClick" "insert_session"`
  - CLI with file hints: `julie-server call-path handle_request write_response --from-file src/server.rs --to-file src/response.rs`
- `get_symbols` - Smart file reading with 70-90% token savings
  - View file structure without reading full content
  - Extract specific symbols with complete code bodies
  - Structure/minimal/full reading modes
- `blast_radius` - Deterministic impact analysis for changed files, internal symbol IDs, or revision ranges
  - Returns ranked impacted symbols, likely tests, deleted files, and a truncation line when more rows exist than `limit`
  - Seed with `file_paths`, internal `symbol_ids`, or Julie revision numbers
  - Prefer `file_paths` when you know a symbol name or file path
  - Use before refactoring or after a change to see affected callers and tests
  - CLI: `julie-server blast-radius --files src/auth/login_flow.rs`

### Editing

- `edit_file` - Edit files without reading them first
  - Three-phase matching: exact substring, trimmed-line (whitespace/indent tolerance), DMP fuzzy (typo tolerance)
  - Supports first, last, or all occurrence replacement
  - Dry-run preview with unified diff output (standard `@@` hunk headers)
  - Bracket balance validation for code files
  - CRLF-aware matching preserves line ending style

### Workspace Management

- `manage_workspace` - Index, open, remove, refresh, list, rebuild, report status, health-check workspaces, recover interrupted edits, and launch the dashboard
  - Operations: `index`, `list`, `open`, `remove`, `refresh`, `health`, `rebuild`, `status`, `recover_edit`, `dashboard`
  - `rebuild` deletes a checkout's index and indexes it again; `status` reports every checkout (root, watcher, counts, database size, Tantivy age, last write)
  - Cross-workspace work: call `open` first, then pass the returned `workspace_id` to other tools

> Operational and session metrics are surfaced through the dashboard. Start it from a shell with `julie-server dashboard`, or from an MCP session with `manage_workspace(operation="dashboard")`.

**Default Ignore Patterns** - Julie automatically excludes common build artifacts and dependencies to prevent indexing noise:

- **Build outputs**: `target/`, `build/`, `dist/`, `out/`, `obj/`, `bin/`
- **Language-specific caches**: `.gradle/`, `.dart_tool/`, `cmake-build-*/`
- **Framework caches**: `.next/`, `.nuxt/`
- **Dependencies**: `node_modules/`, `vendor/`
- **Version control**: `.git/`
- **Test coverage**: `coverage/`, `.nyc_output/`
- **Python bytecode**: `__pycache__/`, `*.pyc`
- **Minified files**: `*.min.js`, `*.bundle.js`, `*.map`

**Custom Ignore Patterns** - Create a `.julieignore` file in your workspace root for project-specific exclusions:

```
# .julieignore example
experimental/
legacy-code/
third-party/
*.generated.ts
```

Patterns use glob syntax (`**/` for recursive, `*` for wildcard). Default patterns cover 99% of use cases - only use `.julieignore` for project-specific needs.

## External Extract (Host Integration)

Beyond the MCP server, Julie ships a process-facing extractor for hosts written in Go, C#, or any runtime that owns its own process management and file watching. `julie-server extract` parses a project root and writes the canonical SQLite schema into a caller-owned database file. It does not use MCP transport, Tantivy, shared registry state, or embeddings.

```bash
# Full scan (incremental — only changed files are re-extracted)
julie-server extract scan --root /repo --db /var/lib/code.sqlite --json

# Rebuild from scratch in one transaction
julie-server extract scan --root /repo --db /var/lib/code.sqlite --force --json

# Single-file updates from a watcher
julie-server extract update --root /repo --db /var/lib/code.sqlite --file src/lib.rs --json
julie-server extract delete --root /repo --db /var/lib/code.sqlite --file src/lib.rs --json

# Recompute reference scores and test linkage after mutations
julie-server extract analyze --db /var/lib/code.sqlite --json

# Read schema version, totals, and analysis state without taking the write lock
julie-server extract info --db /var/lib/code.sqlite --json
```

Key properties:

- **Idempotent.** Repeated calls with unchanged inputs return `unchanged` and commit nothing.
- **Caller-owned DB.** Julie owns the schema; hosts own the file, backups, and lifecycle.
- **Per-DB exclusive write lock** at `<db_path>.julie-extract.lock` (30s default timeout). `extract info` is read-only.
- **Single canonical root per DB.** Pointing the same DB at a different root fails unless you use `scan --force`.
- **Hard-coded ignore policy** (`.gitignore`, `.julieignore`, hard blacklist, 1 MiB per-file cap). Repeatable `--ignore-file` narrows the indexable set but cannot override the blacklist.
- **No silent data loss.** A parser failure that would erase known-good rows exits non-zero and preserves existing data.

See **[docs/EXTERNAL_EXTRACT.md](docs/EXTERNAL_EXTRACT.md)** for the full report schema, exit codes, watcher integration recipe, and SQLite contract.

## Test Detection

Julie automatically detects tests during indexing across all 36 languages, with no configuration required. It recognizes `#[test]`, `@Test`, `pytest`, `describe`/`it`, and other language-specific test patterns.

- **Search filtering** — `fast_search` supports `exclude_tests` to keep test symbols out of production code results
- **Test navigation** — `deep_dive` shows which test functions reference a symbol, so agents can find relevant tests without grepping

For test *coverage measurement* (which lines executed, branch coverage), use your language's coverage tool (`cargo llvm-cov`, `pytest --cov`, `istanbul`, etc.). Julie handles test navigation and filtering; runtime tools handle coverage.

## Skills

Julie ships with a focused set of pre-built skills, reusable prompt workflows that combine Julie's tools into higher-level capabilities. Skills are invoked as slash commands (e.g., `/explore-area`) in harnesses that support them, or used as system prompt instructions.

The plugin distributes four user-facing skills (`/editing`, `/explore-area`, `/impact-analysis`, `/web-research`); this repo also ships `/search-debug` for Julie development.

### Editing Skills

| Skill | Description |
|-------|-------------|
| `/editing` | Zero-read editing: understand and modify code using `edit_file` without reading files first |

### Navigation & Analysis Skills

| Skill | Description |
|-------|-------------|
| `/explore-area` | Orient on an unfamiliar area with token-budgeted exploration via `get_context` |
| `/impact-analysis` | Analyze blast radius of changing a symbol — callers grouped by risk |

### Research Skills

| Skill | Description |
|-------|-------------|
| `/web-research` | Fetch web pages via browser39, index locally, and read selectively with Julie tools |

Web research applies Julie's token-efficiency model to web content. Instead of dumping an entire documentation page into context (often 10,000+ tokens), `/web-research` fetches the page as clean markdown, saves it locally where Julie's filewatcher indexes it, then uses `fast_search` and `get_symbols` to read just the sections you need. Requires [browser39](https://github.com/alejandroqh/browser39/releases) (download a binary release).

### Development Skills

| Skill | Description |
|-------|-------------|
| `/search-debug` | Diagnose why a search returns unexpected results (for Julie development; not distributed in the plugin) |

### Installing Skills

Skills ship as `SKILL.md` files in `.claude/skills/`. Claude Code, Codex, and Antigravity get the skills from the plugin install. OpenCode gets them from `node bin/install-opencode.cjs` in the plugin clone. Other harnesses read the same `SKILL.md` format from their own directory:

| Harness | Skills Directory | Notes |
|---------|-----------------|-------|
| **Claude Code** | `~/.claude/plugins/cache/julie-plugin/julie/<version>/skills/` | Installed by the plugin |
| **Codex** | `~/.codex/plugins/cache/julie-plugin/julie/<version>/skills/` | Installed by the plugin |
| **Antigravity** | `~/.gemini/config/plugins/julie/skills/` (agy 1.2.1) | Installed by the plugin |
| **OpenCode** | `~/.config/opencode/skills/` or `.opencode/skills/` | Run `node bin/install-opencode.cjs` from the plugin clone |
| **VS Code / GitHub Copilot** | `.claude/skills/` or `.github/skills/` | Reads `.claude/skills/` natively from a clone of the julie repo |
| **Gemini CLI** | `.gemini/skills/` or `.agents/skills/` | Copy skill directories |
| **Windsurf** | `.windsurf/skills/` | Copy skill directories |
| **Cursor** | `.cursor/rules/` | Copy `SKILL.md` content into `.mdc` files in the rules directory |
| **Hermes** | `~/.hermes/skills/` | Copy skill directories |

**For other harnesses:** copy `.claude/skills/*/` directories to the harness-specific skills directory listed above. Each skill is a self-contained directory with a `SKILL.md` file.
## Architecture

- **Tree-sitter parsers** for accurate symbol extraction across all languages
- **Tantivy full-text search** with code-aware tokenization (CamelCase/snake_case splitting, English stemming)
- **Graph centrality ranking** using pre-computed reference scores from the relationship graph
- **SQLite storage** for symbols, identifiers, relationships, types, and file metadata
- **Per-workspace isolation** with separate databases and indexes
- **Machine service** serving Streamable HTTP MCP at `/mcp` with a stdio shim for stdio-only clients
- **One writer per checkout** inside the service; index writes serialize through an in-process mutation gate, and a schema or engine mismatch deletes and reindexes instead of migrating
- **Embedding pipeline** through the native `julie-semantic-sidecar`; there is no Python runtime

## Development

### Prerequisites

- **Rust** — [rustup](https://rustup.rs) installs the repository-pinned Rust 1.97.0 toolchain automatically

### Building

```bash
git clone https://github.com/anortham/julie.git
cd julie
cargo build
```

### Running Locally

The no-args `julie-server` is a stdio shim. It starts the machine service
(`julie-server service`) if none is running and forwards MCP requests to it. The
service owns every workspace index:

```bash
cargo run -- --workspace /path/to/your/project
```

To test with an MCP client, point it at your debug build:

```bash
claude mcp add julie-dev -- /path/to/julie/target/debug/julie-server
```

After rebuilding (`cargo build`), run `julie-server service restart` so the
service loads the new binary.

## Command-Line Interface (CLI)

Julie exposes all 12 tools directly to the terminal through named subcommands, a generic tool runner, instant schema discovery, and serial request replay. Every CLI invocation routes through `RequestEngine::execute`, guaranteeing identical parameter validation and execution semantics as MCP sessions.

### 12 Named Tool Subcommands

Every tool is directly accessible as a named subcommand (with ergonomic aliases):

```bash
# Search & Navigation
julie-server fast-search "query" --workspace . --standalone --json       # alias: search
julie-server search "TODO" --regions comment,doc_comment --json                   # content hits in comments only
julie-server fast-refs "SymbolName" --workspace . --standalone --json     # alias: refs
julie-server get-symbols --file src/lib.rs --workspace . --json          # alias: symbols
julie-server get-context --concept "authentication" --workspace . --json   # alias: context
julie-server call-path "fn_a" "fn_b" --workspace . --json
julie-server blast-radius --files src/lib.rs --workspace . --json
julie-server deep-dive "SymbolName" --workspace . --json
julie-server patterns --operation search --pattern-id http.request --workspace . --json

# Editing & Refactoring (Preview dry-run by default; execute mutation without --dry-run)
julie-server edit-file --file src/lib.rs --find "old" --replace "new" --dry-run     # alias: edit
julie-server rewrite-symbol --symbol "MyStruct" --action replace_body --content "..." # alias: rewrite
julie-server rename-symbol --old-name "old" --new-name "new" --dry-run             # alias: rename

# Workspace Management & Dashboard
julie-server manage-workspace --operation health --workspace . --json    # alias: workspace
julie-server dashboard --foreground                                      # Launches browser UI
```

### Generic Tool Runner (`tool`)

Invoke any tool dynamically with structured JSON parameters. Parameter sources (`--params`, `--params-file`, `--params-stdin`) are mutually exclusive via `clap::ArgGroup`:

```bash
# Inline JSON params
julie-server tool fast_search --params '{"query":"UserSession","limit":5}' --workspace . --json

# Trace one call path with JSON params
julie-server tool call_path --params '{"from":"...","to":"..."}' --json

# File-based params (ideal for multi-line edits or complex AST payloads)
julie-server tool edit_file --params-file edit_req.json --json

# Standard input streaming (up to 16 MiB payload ceiling)
cat edit_req.json | julie-server tool edit_file --params-stdin --json
```

[External Extract](#external-extract-host-integration) documents the `julie-server extract` subcommand.
### Zero-Warmup Discovery (`tools list` & `tools schema`)

Instant catalog inspection answering in <15ms without starting file watchers, compiling indexes, or warming embedding models:

```bash
# List all 12 registered tools with descriptions and schema availability
julie-server tools list --json

# Print the complete JSON Schema for a specific tool
julie-server tools schema fast_search --json
```

### Serial Request Replay (`tools replay`)

Execute a recorded stream of requests from a newline-delimited JSON (JSONL) file serially:

```bash
julie-server tools replay --input trace.jsonl --json
```

- Validates JSON framing and auto-assigns line-numbered `request_id` values if omitted.
- Halts on malformed framing.
- Outputs standard JSON envelopes to stdout and returns the first non-zero exit code.

### Semantic Modes (`--semantics`)

Control semantic vector readiness requirements across CLI operations:

```bash
julie-server fast-search "auth flow" --semantics auto     # Default: use vectors if ready, degrade to lexical
julie-server fast-search "auth flow" --semantics off      # Fast: zero model/vector work, pure lexical Tantivy search
julie-server fast-search "auth flow" --semantics required # Strict: fails with exit 4 if vectors missing/stale/incompatible
```

### Exit Codes and Output Envelope

When `--json` is supplied, stdout outputs a single machine-readable JSON envelope:

```json
{
  "schema_version": 1,
  "ok": true,
  "request_id": "req-1",
  "tool": "fast_search",
  "workspace_id": "target_abc",
  "result": { ... },
  "readiness": { "mode": "ready" }
}
```

All logging, progress bars, and diagnostics are emitted strictly to `stderr`. Exit codes conform to standard semantics:
- `0`: Success (`ok: true`)
- `2`: Invalid CLI arguments, syntax errors, unknown tools, or conflicting parameter sources
- `3`: Tool execution domain error (`isError: true` in envelope)
- `4`: Readiness failure (`SEMANTICS_NOT_READY` in required mode)
- `5`: Stale index or unindexed workspace
- `124`: Request deadline / timeout exceeded
- `130`: Process cancellation / SIGINT
- `1`: Internal application failure or unhandled panic

### Date-Versioned MCP Protocol `2026-07-28`

Julie is built on `rmcp` 3.0.1:
- Primary protocol revision: `2026-07-28`
- Legacy protocol revision: `2025-11-25`
- **Direct first-message tool call**: Modern clients can send `tools/call` as the first message over stdio with `_meta` (`io.modelcontextprotocol/protocolVersion`: `"2026-07-28"`). Julie automatically initializes workspace context on demand without requiring preceding `initialize` or `initialized` handshakes.


### Testing

Julie has three fixed test tiers. Each tier runs one `cargo nextest run --workspace` pass:

```bash
# One exact test during the edit loop
cargo nextest run --lib <name>

# Batch gate: the whole workspace except the dogfood set (about 20 s warm)
cargo xtask test dev

# Search-quality gate after search, scoring, ranking, or graph changes
cargo xtask test dogfood

# Dev, then dogfood, before merge
cargo xtask test full
```

Run `cargo nextest run --lib <name>` during the edit loop. Run `cargo xtask test dev` once per completed batch, not after every edit.

Use raw `cargo nextest run --lib <filter>` only to narrow a failure that a tier reported. The dogfood tier is heavier because it loads the large search-quality fixture and runs real searches.

All tiers are currently green. If a test fails, it is a real regression — investigate it.

## Project Structure

```
src/
├── main.rs          # Entry point: stdio shim, service, or subcommand dispatch
├── handler.rs       # MCP tool handler (rmcp ServerHandler)
├── cli.rs           # CLI argument parsing and workspace resolution
├── startup.rs       # Workspace initialization and staleness detection
├── cli_tools/       # Standalone CLI command bootstrap
├── registry/        # Registry DB and project logging
├── dashboard/       # Standalone read-only dashboard (htmx + Tera templates)
├── extractors/      # Thin re-export of the external 36-language extractor crate
├── external_extract/ # Process-facing extractor commands
├── health/          # Health report and diagnostics
├── indexing_core/   # Shared indexing orchestration
├── embeddings/      # Embedding pipeline, native sidecar client and protocol
├── tools/           # MCP tool implementations
│   ├── deep_dive/   # Progressive-depth symbol investigation
│   ├── editing/     # edit_file
│   ├── get_context/ # Token-budgeted context retrieval
│   ├── impact/      # blast_radius
│   ├── metrics/     # Session metrics for the dashboard
│   ├── navigation/  # fast_refs, call_path
│   ├── patterns/    # patterns
│   ├── search/      # fast_search
│   ├── symbols/     # get_symbols
│   └── workspace/   # manage_workspace
├── workspace/       # Multi-workspace management and registry
└── tests/           # Test infrastructure

fixtures/            # Test data (SOURCE/CONTROL files, real-world samples)
```

## License

MIT License - see [LICENSE](LICENSE) file for details

## Contributing

See [CLAUDE.md](CLAUDE.md) for development guidelines and architecture documentation.
