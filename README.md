# Julie

**[Website](https://anortham.github.io/julie/)** · **[Installation](#installation)** · **[Tools](#tools-10)** · **[Skills](#skills)** · **[34 Languages](#supported-languages-34)**

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
- **Background daemon** — shared indexes, shared embedding provider, multi-session support with automatic lifecycle management
- **Stale binary auto-restart** — daemon detects when the binary has been rebuilt and restarts on next session cycle
- **Stdio MCP server** — single binary, zero configuration, works with any MCP client

### Performance Characteristics

- Search latency: <5ms (Tantivy full-text search)
- Memory usage: <100MB typical workload
- Startup time: <2s (database + Tantivy indexing)
- Single binary server with GPU-accelerated embedding sidecar (auto-provisioned via `uv`)

**Incremental Updates**: Only changed files are re-indexed, typically completing in 3-15 seconds.

### Embeddings and GPU Acceleration

Julie uses embeddings for semantic search, related symbol discovery, and intelligent code navigation. These features are powered by a managed Python sidecar (sentence-transformers + PyTorch) with automatic GPU acceleration.

#### Step 1: install `uv`

[`uv`](https://docs.astral.sh/uv/) is the only prerequisite. Julie uses it to install Python 3.12 and all sidecar dependencies automatically. You do not need to install Python yourself.

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

Install Julie as a [Claude Code plugin](https://github.com/anortham/julie-plugin) for the full experience: MCP tools, skills, and a SessionStart hook that teaches Claude how to use Julie effectively. No Rust toolchain required.

```bash
# Add the plugin marketplace
/plugin marketplace add anortham/julie-plugin

# Install (user scope, available across all projects)
/plugin install julie@julie-plugin
```

### Build from Source

```bash
git clone https://github.com/anortham/julie.git
cd julie
cargo build --release
```

### Optional: Web Research

To enable the `/web-research` skill for fetching and indexing web content, download the latest binary from [browser39 releases](https://github.com/alejandroqh/browser39/releases) and add it to your PATH.

This is optional. All other Julie features work without it.

### Connect Your AI Tool

**Client workspace-resolution support:**

| Client | Sends MCP roots? | Needs `JULIE_WORKSPACE`? |
|--------|------------------|--------------------------|
| Claude Code | Yes (on first request) | No (uses cwd / roots) |
| VS Code + GitHub Copilot | Yes | No |
| Codex CLI | No — uses cwd | Only if cwd is wrong |
| Codex Desktop | **No** | **Yes — required** |
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

**Codex CLI** (`~/.codex/config.toml` or `.codex/config.toml`):

```toml
[mcp_servers.julie]
command = "/path/to/julie-server"
```

> Codex CLI usually starts Julie from your current project directory, so `JULIE_WORKSPACE` is not needed in the normal case.
> If you built Julie from source, `command` will usually be `/path/to/julie/target/release/julie-server`.

**Codex Desktop** (`.codex/config.toml` in the repo root):

```toml
[mcp_servers.julie]
command = "/path/to/julie-server"
env = { JULIE_WORKSPACE = "/absolute/path/to/your/project" }
```

> **Codex Desktop does not implement MCP roots** (as of this writing), so Julie cannot resolve the project from client-side workspace folders. You must set `JULIE_WORKSPACE` explicitly or Julie will fall back to whatever `cwd` the Desktop app happens to launch the server with — often `/` or the app bundle. Use an absolute path; `${workspaceFolder}`-style interpolation is not documented for Codex.
>
> Put this file at `.codex/config.toml` in the project root if you want a project-scoped Codex Desktop setup. Codex loads project config only for trusted projects, and project config overrides `~/.codex/config.toml`.

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
| `JULIE_WORKSPACE` | Absolute path to project root | Client roots (if supported), else `cwd` | Overrides workspace detection. Required for clients that don't send MCP roots and launch Julie with an unreliable `cwd` (e.g., Codex Desktop). |
| `JULIE_EMBEDDING_PROVIDER` | `auto`, `sidecar` | `auto` | Selects embedding backend. `auto` resolves to `sidecar` on all platforms. |
| `JULIE_EMBEDDING_SIDECAR_MODEL_ID` | Any HuggingFace model ID | `nomic-ai/CodeRankEmbed` | Sidecar model. CodeRankEmbed (768d) is code-optimized. |
| `JULIE_EMBEDDING_STRICT_ACCEL` | `1` | unset | Disable embeddings entirely when no GPU is available. |

**First Use:**

Julie indexes your workspace automatically on first connection (~2-5s for most projects). All search capabilities are available immediately after indexing completes.

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
- `get_symbols` - Smart file reading with 70-90% token savings
  - View file structure without reading full content
  - Extract specific symbols with complete code bodies
  - Structure/minimal/full reading modes
- `blast_radius` - Deterministic impact analysis for changed files, symbols, or revision ranges
  - Returns ranked impacted symbols, likely tests, deleted files, and spillover handles for long lists
  - Seed with `file_paths`, `symbol_ids`, or Julie revision numbers
  - Use before refactoring or after a change to see affected callers and tests
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

- `manage_workspace` - Index, register, open, remove, refresh, list, stat, clean, and health-check workspaces
  - Operations: `index`, `register`, `open`, `remove`, `list`, `refresh`, `stats`, `clean`, `health`
  - Cross-workspace work: call `open` first, then pass the returned `workspace_id` to other tools

> Operational and session metrics are surfaced through the dashboard (`julie-server dashboard`) rather than an MCP tool — see the **bytes NOT injected** headline metric on the Metrics page.

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
| **Codex CLI** | `.agents/skills/` | Copy skill directories; reads `SKILL.md` format |
| **OpenCode** | Via `instructions` config | Add skill file paths to the `instructions` array in `opencode.json` |

**For harnesses that read `.claude/skills/` natively** (Claude Code, VS Code/Copilot): skills work out of the box when Julie's repo is cloned.

**For other harnesses:** copy `.claude/skills/*/` directories to the harness-specific skills directory listed above. Each skill is a self-contained directory with a `SKILL.md` file.

## Architecture

- **Tree-sitter parsers** for accurate symbol extraction across all languages
- **Tantivy full-text search** with code-aware tokenization (CamelCase/snake_case splitting, English stemming)
- **Graph centrality ranking** using pre-computed reference scores from the relationship graph
- **SQLite storage** for symbols, identifiers, relationships, types, and file metadata
- **Per-workspace isolation** with separate databases and indexes
- **Daemon + adapter pattern** — `julie-server` auto-starts a background daemon and forwards stdio to it via IPC (Unix socket). Multiple MCP clients share one daemon process with shared indexes, embedding provider, and file watchers
- **MCP protocol** over stdio (JSON-RPC)
- **Embedding pipeline** with GPU-accelerated Python sidecar (CUDA/DirectML/MPS/CPU), shared across sessions in daemon mode

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

Julie runs as a stdio MCP server with an automatic background daemon. When an MCP client starts `julie-server`, it auto-launches a daemon process that shares indexes and the embedding provider across sessions:

```bash
cargo run -- --workspace /path/to/your/project
```

To test with an MCP client, point it at your debug build:

```bash
claude mcp add julie-dev -- /path/to/julie/target/debug/julie-server
```

After rebuilding (`cargo build`), restart your MCP client. The daemon detects the stale binary and auto-restarts with the new build.

**Daemon management** (optional, the adapter handles this automatically):

```bash
julie-server daemon      # Start daemon manually
julie-server status      # Check if daemon is running
julie-server stop        # Stop daemon
julie-server restart     # Stop daemon; auto-restarts on next connection
julie-server dashboard   # Open the web dashboard in your browser
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
├── main.rs          # Entry point: adapter mode or subcommand dispatch
├── handler.rs       # MCP tool handler (rmcp ServerHandler)
├── cli.rs           # CLI argument parsing and workspace resolution
├── startup.rs       # Workspace initialization and staleness detection
├── adapter/         # Thin stdio adapter: auto-starts daemon, forwards bytes via IPC
├── daemon/          # Background daemon: shared indexes, IPC server, lifecycle management
├── dashboard/       # Web dashboard (htmx + Tera templates, served by daemon)
├── extractors/      # Language-specific symbol extraction (34 languages)
├── analysis/        # Post-indexing analysis (test quality metrics)
├── database/        # SQLite structured storage
├── search/          # Tantivy search engine and tokenizer
├── embeddings/      # Embedding pipeline, sidecar supervisor and protocol
├── watcher/         # File watcher for incremental re-indexing
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
