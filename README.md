# Julie

**[Website](https://anortham.github.io/julie/)** · **[Installation](#installation)** · **[Tools](#tools-12)** · **[External Extract](#external-extract-host-integration)** · **[Skills](#skills)** · **[34 Languages](#supported-languages-34)**

A cross-platform code intelligence server built in Rust, providing LSP-quality features across 34 programming languages via the Model Context Protocol (MCP).

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
- **Cross-language code navigation** (go-to-definition, find-references) across 34 languages
- **Test-aware search** — automatic test detection across all 34 languages with smart filtering (`exclude_tests`)
- **AST-aware refactoring** with workspace-wide rename and dry-run preview
- **Operational metrics** — per-tool timing, context efficiency tracking, "bytes NOT injected" headline metric
- **Multi-workspace support** for indexing and searching related codebases
- **In-process stdio MCP server** — single binary, zero configuration, works with any MCP client
- **Multi-session coordination** — per-workspace leader locks allow one writer and read-only followers over shared indexes
- **Shared registry and indexes** — `$JULIE_HOME/registry.db` plus `$JULIE_HOME/indexes/` keep related workspaces available across sessions

### Performance Characteristics

- Search latency: <5ms (Tantivy full-text search)
- Memory usage: <100MB typical workload
- Startup time: <2s (database + Tantivy indexing)
- Single binary server with GPU-accelerated embedding sidecar (auto-provisioned via `uv`)

**Incremental Updates**: Only changed files are re-indexed, typically completing in 3-15 seconds.

### Embeddings and GPU Acceleration

Julie uses embeddings for semantic search, related symbol discovery, and intelligent code navigation. These features are powered by a managed Python sidecar (sentence-transformers + PyTorch) with automatic GPU acceleration.

#### Recommended: install `uv`

[`uv`](https://docs.astral.sh/uv/) lets Julie install Python 3.12 and all sidecar dependencies automatically. You do not need to install Python yourself. If `uv` or the sidecar is unavailable, keyword search and code navigation still work; embedding-backed features stay disabled until the sidecar is available.

**macOS:**
```bash
brew install uv
```

**Windows** (open PowerShell):
```powershell
winget install --id=astral-sh.uv -e
```

If `winget` is not available, use the standalone installer instead:
```powershell
powershell -ExecutionPolicy ByPass -c "irm https://astral.sh/uv/install.ps1 | iex"
```

**Linux:**
```bash
curl -LsSf https://astral.sh/uv/install.sh | sh
```

After installing, open a new terminal and verify with `uv --version`.

#### Step 2: there is no step 2

On first launch, Julie automatically creates a Python 3.12 environment, installs PyTorch and the embedding model, and detects your GPU. Everything is cached, so subsequent launches are instant.

#### GPU acceleration

Julie auto-detects your GPU and uses it for faster embeddings:

- **NVIDIA (CUDA)**: auto-detected; the correct torch+CUDA variant is installed automatically
- **AMD/Intel (DirectML)**: auto-detected on Windows via `torch-directml`
- **Apple Silicon (MPS)**: auto-detected by PyTorch
- **CPU**: used when no GPU is available (slower, but fully functional)

Python 3.12 is used because it has the best PyTorch hardware acceleration compatibility across all GPU backends.

#### Advanced configuration

- `JULIE_HOME`: relocate shared registry state and workspace indexes (default: `~/.julie`). Must be an absolute path; empty or relative values are rejected. Existing installs upgrade in place — set this only if you want to move Julie's storage to another drive or path. See `docs/OPERATIONS.md` for the migration checklist.
- `JULIE_EMBEDDING_SIDECAR_MODEL_ID`: any HuggingFace model ID (default: `nomic-ai/CodeRankEmbed`, 768d code-optimized). Changing models automatically wipes and re-embeds all vectors on the next indexing run.
- See `docs/operations/embedding-sidecar.md` for all environment variables and troubleshooting

## Supported Languages (34)

**Core:** Rust, TypeScript, JavaScript, Python, Java, C#, VB.NET, PHP, Ruby, Swift, Kotlin, Scala

**Systems:** C, C++, Go, Lua, Zig

**Functional:** Elixir

**Specialized:** GDScript, Vue, QML, R, Razor, SQL, HTML, CSS, Regex, Bash, PowerShell, Dart

**Documentation:** Markdown, JSON, TOML, YAML

## Installation

### Claude Code Plugin (Recommended)

The [`julie-plugin`](https://github.com/anortham/julie-plugin) bundles pre-built binaries, Julie skills, and the MCP server registration. No Rust toolchain required.

```bash
# Add the plugin marketplace
/plugin marketplace add anortham/julie-plugin

# Install (user scope, available across all projects)
/plugin install julie@julie-plugin
```

The Claude Code plugin starts Julie automatically. Do not also run `claude mcp add` unless you are doing a manual binary install.

### Codex CLI / Codex Desktop Helper

Clone the plugin repo, then run the installer:

```bash
git clone https://github.com/anortham/julie-plugin.git
node julie-plugin/bin/install-codex.cjs
```

The installer adds Julie skills, hooks, and AGENTS.md guidance. It prints the exact MCP command for your checkout. The normal user-level shape is:

```bash
codex mcp add julie -- node /absolute/path/to/julie-plugin/hooks/run.cjs
```

Codex CLI and Codex Desktop do not send MCP roots, so Julie uses the process cwd/startup hint. In normal CLI use this is the repo you launched from. If a desktop app starts Julie from the wrong directory, set `JULIE_WORKSPACE` in that app's MCP config or launch from the repo root.

### OpenCode Helper

Clone the plugin repo, then run the installer:

```bash
git clone https://github.com/anortham/julie-plugin.git
node julie-plugin/bin/install-opencode.cjs
```

The installer adds Julie skills, a precedence plugin, and AGENTS.md guidance. It prints an `opencode.json` MCP block using the plugin launcher:

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

Both helper installers are idempotent and support `--uninstall` for clean removal.

### Build from Source

```bash
git clone https://github.com/anortham/julie.git
cd julie
cargo build --release
```

### Optional: Web Research

To enable the `/web-research` skill for fetching and indexing web content, download the latest binary from [browser39 releases](https://github.com/alejandroqh/browser39/releases) and add it to your PATH.

This is optional. All other Julie features work without it.

<a id="manual-install"></a>
### Manual MCP Install

Use this path when you are not using the Claude Code plugin or the Codex/OpenCode helper installers.

Download a release archive for your platform from [GitHub releases](https://github.com/anortham/julie/releases), extract `julie-server`, and use its absolute path in your MCP config. Supported release targets are macOS Apple Silicon, macOS Intel, Linux x86_64, and Windows x86_64.

**Upgrading from an older split-daemon install?** `julie-adapter` and `julie-daemon` are gone from the current MCP runtime. Every MCP client config must launch `julie-server` directly. If an old config still points at `julie-adapter`, change the command to the `julie-server` path, then restart the MCP client. Plugin users should update or reinstall the Julie plugin so its launcher also starts `julie-server`.

Stale adapter configs usually show up as old processes named `julie-adapter` or `julie-daemon`, or config files whose command path still ends in one of those names. Stop those old processes after updating the config; a fresh session should only start `julie-server`.

**Client workspace-resolution support:**

| Client | Sends MCP roots? | Needs `JULIE_WORKSPACE`? |
|--------|------------------|--------------------------|
| Claude Code | Yes (on first request) | No (uses cwd / roots) |
| VS Code + GitHub Copilot | Yes | No |
| Codex CLI | No — uses cwd | Only if cwd is wrong |
| Codex Desktop | No — uses cwd/startup hint | Only if cwd is wrong |
| OpenCode | No — uses cwd | Optional; useful for project configs |
| Cursor / Windsurf / others | Varies | Safe default: set it |

Julie prefers client roots when the startup hint is weak (`cwd`). Explicit CLI `--workspace` or `JULIE_WORKSPACE` always wins regardless of client support.

**Claude Code** (user-level, available in all projects):

```bash
claude mcp add --scope user julie -- /path/to/julie-server
```

Or edit `~/.claude.json` directly for more control (e.g., env vars, model override):

```json
{
  "mcpServers": {
    "julie": {
      "type": "stdio",
      "command": "/path/to/julie-server",
      "args": [],
      "env": {
        "JULIE_WORKSPACE": "/path/to/your/project"
      }
    }
  }
}
```

For project-level only, use `--scope project` or omit the scope flag. When using `claude mcp add`, Julie uses the current directory as the workspace root — `JULIE_WORKSPACE` is only needed if you want to override that.

**VS Code with GitHub Copilot** (`.vscode/mcp.json`):

```json
{
  "servers": {
    "julie": {
      "type": "stdio",
      "command": "/path/to/julie/target/release/julie-server"
    }
  }
}
```

> VS Code's MCP client sends workspace folders as [MCP roots](https://modelcontextprotocol.io/specification/server/utilities/roots), so Julie resolves the project root automatically on the first tool call. No `JULIE_WORKSPACE` env is needed — set one only to override VS Code's open folder.
>
> **Windows?** Use backslashes in the command path: `"command": "C:\\path\\to\\julie-server.exe"`
>
> All `env` values are optional — see the [env options table](#available-env-options) below for defaults.

**Codex CLI / Codex Desktop** (`~/.codex/config.toml`):

```toml
[mcp_servers.julie]
command = "/path/to/julie-server"
```

> Prefer `codex mcp add julie -- /path/to/julie-server` for manual registration. When using the plugin launcher, use `codex mcp add julie -- node /absolute/path/to/julie-plugin/hooks/run.cjs`.

**OpenCode** (`~/.config/opencode/opencode.json` for global, or `<repo>/opencode.json` for project):

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "julie": {
      "type": "local",
      "command": ["/path/to/julie-server"],
      "enabled": true,
      "environment": {
        "JULIE_WORKSPACE": "/absolute/path/to/your/project"
      }
    }
  }
}
```

> OpenCode expects `command` as an **array** and the env key is `environment` (not `env`). `JULIE_WORKSPACE` is optional when OpenCode starts from the repo root; set it when OpenCode launches Julie from an unreliable cwd. The [`julie-plugin`](https://github.com/anortham/julie-plugin) installer (`node bin/install-opencode.cjs`) wires up skills, the precedence plugin, and AGENTS.md but leaves this MCP block to you.

**Cursor / Windsurf / Other MCP Clients:**

```json
{
  "mcpServers": {
    "julie": {
      "command": "/path/to/julie-server",
      "env": {
        "JULIE_WORKSPACE": "/path/to/your/project"
      }
    }
  }
}
```

All `env` values are optional — see the table below for defaults.

<a id="available-env-options"></a>
**Available env options:**

| Variable | Values | Default | Notes |
|----------|--------|---------|-------|
| `JULIE_WORKSPACE` | Absolute path to project root | Client roots (if supported), else `cwd` | Overrides workspace detection. Set this when a no-roots client launches Julie from the wrong directory. |
| `JULIE_EMBEDDING_PROVIDER` | `auto`, `sidecar` | `auto` | Selects embedding backend. `auto` resolves to `sidecar` on all platforms. |
| `JULIE_EMBEDDING_SIDECAR_MODEL_ID` | Any HuggingFace model ID | `nomic-ai/CodeRankEmbed` | Sidecar model. CodeRankEmbed (768d) is code-optimized. |
| `JULIE_EMBEDDING_STRICT_ACCEL` | `1` | unset | Disable embeddings entirely when no GPU is available. |

**First Use / Verify:**

Julie indexes your workspace automatically on first connection or first primary tool call. Ask your agent to run `manage_workspace(operation="health")` if you want to confirm which workspace is bound. First indexing may take a few seconds on small projects and longer on large repos; later sessions reuse the cached index and file watcher updates.

## Tools (12)

### Search & Navigation

- `fast_search` - Full-text code search with code-aware tokenization
  - Content search (grep-style line matches) or definition search (symbol names with signatures)
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
  - Includes test locations with quality tiers and centrality scores
  - Identifier fallback for references that relationships miss
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
  - Returns ranked impacted symbols, likely tests, deleted files, and spillover handles for long lists
  - Seed with `file_paths`, internal `symbol_ids`, or Julie revision numbers
  - Prefer `file_paths` when you know a symbol name or file path
  - Use before refactoring or after a change to see affected callers and tests
  - CLI: `julie-server blast-radius --files src/auth/login_flow.rs`
- `spillover_get` - Fetch the next page for large `get_context` or `blast_radius` results
  - Reuses the stored spillover handle instead of rerunning the underlying query

### Editing

- `edit_file` - Edit files without reading them first
  - Three-phase matching: exact substring, trimmed-line (whitespace/indent tolerance), DMP fuzzy (typo tolerance)
  - Supports first, last, or all occurrence replacement
  - Dry-run preview with unified diff output (standard `@@` hunk headers)
  - Bracket balance validation for code files
  - CRLF-aware matching preserves line ending style
- `rewrite_symbol` - Edit a symbol by name without reading the file first
  - Operations: `replace_full`, `replace_body`, `replace_signature`, `insert_before`, `insert_after`, `add_doc`
  - Symbol lookup by qualified name (e.g., `MyClass::method`); use `file_path` to disambiguate
  - Combine with `deep_dive` for zero-read editing workflows
  - Dry-run preview with unified diff output

### Refactoring

- `rename_symbol` - Rename symbols across the workspace
  - Updates all references atomically
  - Scope control: `workspace` (default), `all`, or `file:<path>` to disambiguate shared names
  - Preview mode with `dry_run` parameter

### Workspace Management

- `manage_workspace` - Index, register, open, remove, refresh, list, stat, clean, health-check workspaces, and launch the dashboard
  - Operations: `index`, `register`, `open`, `remove`, `list`, `refresh`, `stats`, `clean`, `health`, `dashboard`
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

Julie automatically detects tests during indexing across all 34 languages, with no configuration required. It recognizes `#[test]`, `@Test`, `pytest`, `describe`/`it`, and other language-specific test patterns.

- **Search filtering** — `fast_search` supports `exclude_tests` to keep test symbols out of production code results
- **Test navigation** — `deep_dive` shows which test functions reference a symbol, so agents can find relevant tests without grepping

For test *coverage measurement* (which lines executed, branch coverage), use your language's coverage tool (`cargo llvm-cov`, `pytest --cov`, `istanbul`, etc.). Julie handles test navigation and filtering; runtime tools handle coverage.

## Skills

Julie ships with a focused set of pre-built skills, reusable prompt workflows that combine Julie's tools into higher-level capabilities. Skills are invoked as slash commands (e.g., `/explore-area`) in harnesses that support them, or used as system prompt instructions.

The plugin distributes four user-facing skills (`/editing`, `/explore-area`, `/impact-analysis`, `/web-research`); this repo also ships `/search-debug` for Julie development.

### Editing Skills

| Skill | Description |
|-------|-------------|
| `/editing` | Zero-read editing: understand and modify code using `edit_file`, `rewrite_symbol`, and `rename_symbol` without reading files first |

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

Skills ship as `SKILL.md` files in `.claude/skills/`. Most modern AI coding harnesses now support the same skill format — just copy the skill directories to the right location:

| Harness | Skills Directory | Notes |
|---------|-----------------|-------|
| **Claude Code** | `.claude/skills/` | Works automatically — skills are already here |
| **VS Code / GitHub Copilot** | `.claude/skills/` or `.github/skills/` | Reads `.claude/skills/` natively — no copying needed |
| **Gemini CLI** | `.gemini/skills/` or `.agents/skills/` | Copy skill directories; same `SKILL.md` format |
| **Windsurf** | `.windsurf/skills/` | Copy skill directories to `.windsurf/skills/` |
| **Cursor** | `.cursor/rules/` | Copy `SKILL.md` content into `.mdc` files in the rules directory |
| **Codex CLI** | `~/.codex/skills/` or `.agents/skills/` | Copy skill directories, or use `node bin/install-codex.cjs` from `julie-plugin` |
| **OpenCode** | `~/.config/opencode/skills/` or `.opencode/skills/` | Auto-discovered (also reads `~/.claude/skills/` natively); use `node bin/install-opencode.cjs` from `julie-plugin` for symlinks + precedence plugin |

**For harnesses that read `.claude/skills/` natively** (Claude Code, VS Code/Copilot): skills work out of the box when Julie's repo is cloned.

**For other harnesses:** copy `.claude/skills/*/` directories to the harness-specific skills directory listed above. Each skill is a self-contained directory with a `SKILL.md` file.

## Architecture

- **Tree-sitter parsers** for accurate symbol extraction across all languages
- **Tantivy full-text search** with code-aware tokenization (CamelCase/snake_case splitting, English stemming)
- **Graph centrality ranking** using pre-computed reference scores from the relationship graph
- **SQLite storage** for symbols, identifiers, relationships, types, and file metadata
- **Per-workspace isolation** with separate databases and indexes
- **In-process MCP protocol** over stdio (JSON-RPC), with no background daemon or HTTP bridge
- **Per-workspace leader locks** so one session owns writes while other sessions serve read-only requests from SQLite WAL and Tantivy mmap
- **Embedding pipeline** with GPU-accelerated Python sidecar (CUDA/DirectML/MPS/CPU), shared through a resident embedding host per `$JULIE_HOME`

## Development

### Prerequisites

- **Rust** (stable, 1.85+) — [rustup.rs](https://rustup.rs)
- **[uv](https://docs.astral.sh/uv/)** — auto-provisions Python 3.12 and the embedding sidecar (see [Embeddings and GPU Acceleration](#embeddings-and-gpu-acceleration))

### Building

```bash
git clone https://github.com/anortham/julie.git
cd julie
cargo build
```

### Running Locally

Julie runs as an in-process stdio MCP server. When an MCP client starts
`julie-server`, that process serves MCP directly and coordinates shared indexes
through per-workspace leader locks:

```bash
cargo run -- --workspace /path/to/your/project
```

To test with an MCP client, point it at your debug build:

```bash
claude mcp add julie-dev -- /path/to/julie/target/debug/julie-server
```

After rebuilding (`cargo build`), restart your MCP client or start a new
session so the client launches the new binary.

Useful local commands:

```bash
julie-server dashboard   # Open the web dashboard in your browser
julie-server search "query" --workspace . --standalone --json
```

### Testing

Julie has a tiered xtask runner so the documented commands stay aligned with the checked-in manifest:

```bash
# Default local loop from the current diff
cargo xtask test changed

# Tiny smoke pass
cargo xtask test smoke

# Batch-level regression gate
cargo xtask test dev

# System / startup coverage
cargo xtask test system

# Search-quality / dogfood tier
cargo xtask test dogfood

# Broad branch-level pass
cargo xtask test full

# Inspect available tiers and buckets
cargo xtask test list
```

Use `cargo xtask test changed` for the local loop. It maps the current git diff to the smallest matching bucket set, then falls back to `dev` if shared infrastructure moved. Run `cargo xtask test dev` once per completed batch, not after every edit.

Use raw `cargo test --lib <filter>` only when narrowing a failure after `changed` or an xtask tier points you at the right area. The dogfood tier is intentionally heavier because it loads the large search-quality fixture and runs real searches.

All tiers are currently green. If a test fails, it is a real regression — investigate it.

## Project Structure

```
src/
├── main.rs          # Entry point: in-process MCP serve or subcommand dispatch
├── handler.rs       # MCP tool handler (rmcp ServerHandler)
├── cli.rs           # CLI argument parsing and workspace resolution
├── startup.rs       # Workspace initialization and staleness detection
├── cli_tools/       # Standalone CLI command bootstrap
├── daemon/          # Registry DB, leader-lock compatibility, project logging
├── dashboard/       # Standalone read-only dashboard (htmx + Tera templates)
├── extractors/      # Language-specific symbol extraction (34 languages)
├── external_extract/ # Process-facing extractor commands
├── health/          # Health report and diagnostics
├── indexing_core/   # Shared indexing orchestration
├── embeddings/      # Embedding pipeline, sidecar supervisor and protocol
├── tools/           # MCP tool implementations
│   ├── deep_dive/   # Progressive-depth symbol investigation
│   ├── editing/     # edit_file, rewrite_symbol
│   ├── get_context/ # Token-budgeted context retrieval
│   ├── impact/      # blast_radius
│   ├── metrics/     # Session metrics for the dashboard
│   ├── navigation/  # fast_refs, call_path
│   ├── refactoring/ # rename_symbol
│   ├── search/      # fast_search
│   ├── spillover/   # spillover_get
│   ├── symbols/     # get_symbols
│   └── workspace/   # manage_workspace
├── workspace/       # Multi-workspace management and registry
└── tests/           # Test infrastructure

python/
└── embeddings_sidecar/  # GPU-accelerated embedding sidecar (PyTorch + sentence-transformers)

fixtures/            # Test data (SOURCE/CONTROL files, real-world samples)
```

## License

MIT License - see [LICENSE](LICENSE) file for details

## Contributing

See [CLAUDE.md](CLAUDE.md) for development guidelines and architecture documentation.
