# Julie

**[Website](https://anortham.github.io/julie/)** · **[Installation](#installation)** · **[Tools](#tools-8)** · **[Skills](#skills)** · **[33 Languages](#supported-languages-33)**

A cross-platform code intelligence server built in Rust, providing LSP-quality features across 33 programming languages via the Model Context Protocol (MCP).

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
- **Cross-language code navigation** (go-to-definition, find-references) across 33 languages
- **Test intelligence** — automatic test detection, test quality metrics, and test-to-code linkage across all 33 languages
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

## Supported Languages (33)

**Core:** Rust, TypeScript, JavaScript, Python, Java, C#, PHP, Ruby, Swift, Kotlin, Scala

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

### Connect Your AI Tool

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
      "command": "/path/to/julie/target/release/julie-server",
      "env": {
        "JULIE_WORKSPACE": "${workspaceFolder}"
      }
    }
  }
}
```

> **Windows?** Use backslashes in the command path: `"command": "C:\\path\\to\\julie-server.exe"`
>
> `${workspaceFolder}` is a VS Code variable that resolves to the root of your open project.
> All `env` values are optional — see the [env options table](#available-env-options) below for defaults.

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
| `JULIE_WORKSPACE` | Absolute path to project root | Current working directory | Tells Julie which project to index. Set explicitly if cwd is unreliable. |
| `JULIE_EMBEDDING_PROVIDER` | `auto`, `sidecar` | `auto` | Selects embedding backend. `auto` resolves to `sidecar` on all platforms. |
| `JULIE_EMBEDDING_SIDECAR_MODEL_ID` | Any HuggingFace model ID | `nomic-ai/CodeRankEmbed` | Sidecar model. CodeRankEmbed (768d) is code-optimized. |
| `JULIE_EMBEDDING_STRICT_ACCEL` | `1` | unset | Disable embeddings entirely when no GPU is available. |

**First Use:**

Julie indexes your workspace automatically on first connection (~2-5s for most projects). All search capabilities are available immediately after indexing completes.

## Tools (8)

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
- `get_symbols` - Smart file reading with 70-90% token savings
  - View file structure without reading full content
  - Extract specific symbols with complete code bodies
  - Structure/minimal/full reading modes

### Refactoring

- `rename_symbol` - Rename symbols across entire workspace
  - Updates all references atomically
  - Preview mode with dry_run parameter

### Workspace Management

- `manage_workspace` - Index, add, remove, refresh, and clean workspaces

### Metrics

- `query_metrics` - Session performance and operational metrics
  - `category: "session"` (default) — per-tool call counts, average latency, output bytes, and context efficiency (source bytes examined vs output returned)
  - `category: "history"` — cross-session trends: total calls, p95 latencies, cumulative context efficiency
  - Headline metric: **bytes NOT injected into context** (source_bytes - output_bytes)

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

## Test Intelligence

Julie automatically detects and analyzes tests during indexing across all 33 languages, with no configuration required.

- **Test detection** — recognizes `#[test]`, `@Test`, `pytest`, `describe`/`it`, and language-specific test patterns
- **Test quality metrics** — assertion density, mock usage, error path coverage, classified as thorough/adequate/thin/stub
- **Test-to-code linkage** — maps which tests exercise each production function via call graph and identifier analysis
- **Smart test filtering** — `fast_search` supports `exclude_tests` parameter to filter test symbols from results

These signals appear in `deep_dive` output, giving agents immediate awareness of test coverage without extra tool calls.

## Skills

Julie ships with 9 pre-built skills, reusable prompt workflows that combine Julie's tools into higher-level capabilities. Skills are invoked as slash commands (e.g., `/architecture`) in harnesses that support them, or used as system prompt instructions.

### Report Skills

| Skill | Description |
|-------|-------------|
| `/architecture` | Architecture overview: entry points, module map, dependency flow, suggested reading order |
| `/metrics` | Session performance report: tool usage, timing, and context efficiency (bytes NOT injected) |

### Navigation & Analysis Skills

| Skill | Description |
|-------|-------------|
| `/explore-area` | Orient on an unfamiliar area with token-budgeted exploration |
| `/call-trace` | Trace the call path between two functions |
| `/logic-flow` | Step-by-step explanation of a function's logic and control flow |
| `/impact-analysis` | Analyze blast radius of changing a symbol — callers grouped by risk |
| `/dependency-graph` | Show module dependencies by analyzing imports and cross-references |
| `/type-flow` | Trace how types flow through a function — parameters, transforms, returns |

### Debugging Skills

| Skill | Description |
|-------|-------------|
| `/search-debug` | Diagnose why a search returns unexpected results (for Julie development) |

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
# Tiny smoke pass
cargo xtask test smoke

# Default local development tier
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

Use raw `cargo test --lib <filter>` only when narrowing a failure after an xtask tier points you at the right area. The dogfood tier is intentionally heavier because it loads the large search-quality fixture and runs real searches.

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
├── extractors/      # Language-specific symbol extraction (33 languages)
├── analysis/        # Post-indexing analysis (test quality metrics, risk scoring)
├── database/        # SQLite structured storage
├── search/          # Tantivy search engine and tokenizer
├── embeddings/      # Embedding pipeline, sidecar supervisor and protocol
├── watcher/         # File watcher for incremental re-indexing
├── tools/           # MCP tool implementations
│   ├── deep_dive/   # Progressive-depth symbol investigation
│   ├── get_context/ # Token-budgeted context retrieval
│   ├── navigation/  # fast_refs
│   ├── refactoring/ # rename_symbol
│   ├── search/      # fast_search
│   ├── symbols/     # get_symbols
│   └── workspace/   # Workspace management and indexing
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
