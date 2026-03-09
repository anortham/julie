# Julie

A cross-platform code intelligence server built in Rust, providing LSP-quality features across 31 programming languages via the Model Context Protocol (MCP).

## Why Julie?

Without code intelligence, AI agents waste most of their context window just *reading* code. A 500-line file costs ~2,000 tokens. Understanding a module means reading every file in it. Tracing a function's callers means grepping the entire codebase. Every token spent reading raw files is a token unavailable for reasoning, planning, and generating code — and once the context window fills up, the session is over.

Julie parses your codebase once with tree-sitter, builds a searchable index with a full reference graph, and returns only what the agent actually needs. Fewer tokens per operation means more operations before hitting context limits — and longer, more productive coding sessions:

| Task | Without Julie | With Julie | Savings |
|------|--------------|------------|---------|
| Understand a file's API | Read whole file (~2,000 tokens) | `get_symbols` structure mode (~200 tokens) | ~90% |
| Find a function | Grep + read matching files (~5,000+ tokens) | `fast_search` with ranked results (~300 tokens) | ~94% |
| Understand a symbol before modifying it | Read file + grep for references (~4,000+ tokens) | `deep_dive` overview (~200 tokens) | ~95% |
| Orient on a new area of the codebase | Read 5-10 files (~10,000+ tokens) | `get_context` with token budgeting (~2,000 tokens) | ~80% |

The key difference from simpler code indexing tools: Julie doesn't just extract symbols — it builds a **reference graph** so agents can navigate code relationships (who calls this function? what does it call? what types flow through it?) without reading files at all.

## Features

- **Fast symbol search** with code-aware tokenization (CamelCase/snake_case splitting, stemming, <5ms)
- **Cross-language code navigation** (go-to-definition, find-references) across 31 languages
- **AST-aware refactoring** with workspace-wide rename and dry-run preview
- **Multi-workspace support** for indexing and searching related codebases
- **Persistent daemon mode** with HTTP API and web dashboard at `/ui/`
- **Multi-agent dispatch** — run tasks through Claude Code, Codex, Gemini CLI, or Copilot CLI from the dashboard
- **Web dashboard** — project management, search exploration, agent dispatch, memory browser, embedding status
- **Developer memory** — checkpoint progress, recall context across sessions, manage persistent plans
- **Auto-start daemon** via `julie-server connect` (stdio bridge with automatic daemon lifecycle)
- **OpenAPI documentation** with interactive Scalar docs at `/api/docs`

### Performance Characteristics

- Search latency: <5ms (Tantivy full-text search)
- Memory usage: <100MB typical workload
- Startup time: <2s (database + Tantivy indexing)
- Single binary server deployment with optional GPU-accelerated embedding sidecar

**Incremental Updates**: Only changed files are re-indexed, typically completing in 3-15 seconds.

### Embeddings Runtime

Julie uses a managed Python sidecar for GPU-accelerated semantic embeddings (BGE-small-en-v1.5, 384 dimensions). The sidecar is fully automated:

- **Auto-provisioning**: If `uv` is available and no compatible Python 3.10-3.13 is found, Julie installs one via `uv python install` and creates a managed venv with `uv venv`
- **GPU acceleration**: Automatically detects and uses CUDA (Linux/Windows), MPS (macOS), or DirectML (Windows) — falls back to CPU if no GPU is available
- **Fallback**: If the sidecar fails to initialize, Julie falls back to in-process ONNX Runtime (CPU-only) — keyword search always remains available
- **Zero configuration**: Works out of the box on systems with `uv` or a compatible Python on PATH

**Runtime controls:**
- `JULIE_EMBEDDING_PROVIDER`: `auto|sidecar|ort` (default: `auto`, tries sidecar first)
- `JULIE_EMBEDDING_STRICT_ACCEL`: `1` to disable embeddings when no GPU is available
- See `docs/operations/embedding-sidecar.md` for all env vars and troubleshooting

## Supported Languages (31)

**Core:** Rust, TypeScript, JavaScript, Python, Java, C#, PHP, Ruby, Swift, Kotlin

**Systems:** C, C++, Go, Lua, Zig

**Specialized:** GDScript, Vue, QML, R, Razor, SQL, HTML, CSS, Regex, Bash, PowerShell, Dart

**Documentation:** Markdown, JSON, JSONL, TOML, YAML

## Installation

### Step 1: Install the Binary

Download the latest release for your platform from the [Releases page](https://github.com/anortham/julie/releases):

| Platform | Archive |
|----------|---------|
| macOS (Apple Silicon) | `julie-v4.0.0-aarch64-apple-darwin.tar.gz` |
| macOS (Intel) | `julie-v4.0.0-x86_64-apple-darwin.tar.gz` |
| Linux (x86_64) | `julie-v4.0.0-x86_64-unknown-linux-gnu.tar.gz` |
| Windows (x86_64) | `julie-v4.0.0-x86_64-pc-windows-msvc.zip` |

```bash
# Example: macOS Apple Silicon
tar -xzf julie-v4.0.0-aarch64-apple-darwin.tar.gz
sudo mv julie-server /usr/local/bin/
```

### Step 2: Configure as MCP Server

**Claude Code — Plugin (Recommended):**

The Julie plugin gives you the full experience: MCP tools + skills (`/checkpoint`, `/recall`, `/plan`, `/standup`, `/plan-status`) + hooks (auto-recall on session start, auto-checkpoint before compaction, auto-save plans).

```bash
# Start the daemon
julie-server daemon start

# Install the plugin
/plugin marketplace add anortham/julie
/plugin install julie@julie
```

The plugin connects to the Julie daemon via HTTP. A web dashboard is available at `http://localhost:7890/ui/` and API docs at `http://localhost:7890/api/docs`.

**Claude Code — Standalone MCP (No Plugin):**

If you prefer a simpler setup without hooks and skills:

```bash
# Auto-starts daemon, bridges stdio↔HTTP
claude mcp add julie -- /path/to/julie-server connect
```

The `connect` command auto-starts a persistent daemon on first use, registers your workspace, and bridges stdio↔HTTP. The daemon survives session exits, so subsequent sessions connect instantly.

For direct stdio mode (no daemon), omit the `connect` argument:

```bash
claude mcp add julie /path/to/julie-server
```

**VS Code with GitHub Copilot:**

Create a workspace-level `.vscode/mcp.json` file in your project:

```json
{
  "servers": {
    "Julie": {
      "type": "stdio",
      "command": "/path/to/julie-server",
      "args": [],
      "env": {
        "JULIE_WORKSPACE": "${workspaceFolder}"
      }
    }
  }
}
```

**Important:** The `JULIE_WORKSPACE` environment variable is **required** for VS Code to ensure Julie creates its `.julie` folder in your workspace directory (not your home directory). VS Code automatically substitutes `${workspaceFolder}` with the actual workspace path.

**Cursor / Other MCP Clients:**

Daemon mode (recommended — shared index, background indexing, dashboard):

```json
{
  "mcpServers": {
    "julie": {
      "type": "http",
      "url": "http://localhost:7890/mcp"
    }
  }
}
```

Stdio mode (no daemon required):

```json
{
  "mcpServers": {
    "julie": {
      "command": "/path/to/julie-server",
      "args": [],
      "env": {
        "JULIE_WORKSPACE": "/path/to/your/workspace"
      }
    }
  }
}
```

**First Use:**

Julie indexes your workspace automatically on first connection:

1. **Indexing (~2-5s)**: Extracts symbols via tree-sitter, stores in SQLite, builds Tantivy search index
   - All search capabilities available immediately after indexing completes
   - Duration scales with workspace size (10K files ≈ 2-5s)

**To check indexing status:**

```
manage_workspace(operation="health", detailed=true)
```

**Workspace Detection:**

Julie determines where to create the `.julie/` folder using this priority order:

1. **CLI argument** (if passed): `--workspace /path/to/workspace`
2. **JULIE_WORKSPACE environment variable** (VS Code `${workspaceFolder}`)
3. **Current working directory** (fallback for Claude Code and other clients)

If you see a `.julie/` folder in an unexpected location, check your `JULIE_WORKSPACE` environment variable setting.

### Build from Source

If you prefer to build from source:

```bash
git clone https://github.com/anortham/julie.git
cd julie
cargo build --release
# Binary will be at: target/release/julie-server[.exe]
```

## Tools (10)

### Search & Navigation

- `fast_search` - Full-text code search with code-aware tokenization
  - Content search (grep-style line matches) or definition search (symbol names with signatures)
  - Definition search promotes exact symbol matches with kind, visibility, and signature
  - <5ms search latency with CamelCase/snake_case splitting, English stemming
  - Automatic OR-fallback when strict AND returns zero results
  - Language and file pattern filtering
- `get_context` - Token-budgeted context for a concept or task
  - Returns relevant code subgraph with pivots (full code) and neighbors (signatures)
  - Pipeline: search → centrality ranking → graph expansion → adaptive token allocation → formatted output
  - Adaptive budget: few results → deep context, many results → broad overview
  - Use at the start of a task for area-level orientation
- `deep_dive` - Progressive-depth symbol investigation
  - Overview (~200 tokens), context (~600 tokens), or full (~1500 tokens) detail levels
  - Kind-aware: functions show callers/callees/types, traits show implementations, structs show fields/methods
  - Includes identifier fallback and test file locations at full depth
- `fast_refs` - Find all references to a symbol with structured output
- `get_symbols` - Smart file reading with 70-90% token savings
  - View file structure without reading full content
  - Extract specific symbols with complete code bodies
  - Structure/minimal/full reading modes

### Refactoring

- `rename_symbol` - Rename symbols across entire workspace
  - Updates all references atomically
  - Preview mode with dry_run parameter

### Developer Memory

- `checkpoint` - Save development milestones with git context, tags, and structured fields
- `recall` - Restore context from previous sessions with BM25 full-text search
- `plan` - Manage persistent plans that survive context compaction and guide multi-session work

### Workspace Management

- `manage_workspace` - Index, add, remove, refresh, and clean workspaces

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

## Web Dashboard

The daemon serves a built-in web dashboard at `http://localhost:7890/ui/` with:

- **Dashboard** — project health, memory stats, agent activity, backend availability, embedding status with on-demand initialization
- **Projects** — register/remove projects, view stats (language breakdown, symbol counts by kind), quick-launch actions (copy path, open in editor, open in terminal)
- **Search** — interactive search with debug mode for inspecting scoring and tokenization
- **Agents** — dispatch tasks to any detected CLI agent (Claude Code, Codex, Gemini CLI, Copilot CLI), view dispatch history with streaming output
- **Memories** — browse checkpoints and plans across projects, filter by type and tags

All features work in both light and dark mode, with responsive layouts for mobile.

## Architecture

- **Tree-sitter parsers** for accurate symbol extraction across all languages
- **Tantivy full-text search** with code-aware tokenization (CamelCase/snake_case splitting, English stemming)
- **Graph centrality ranking** using pre-computed reference scores from the relationship graph
- **SQLite storage** for symbols, identifiers, relationships, types, and file metadata
- **Per-workspace isolation** with separate databases and indexes
- **MCP protocol** for AI agent integration (stdio and Streamable HTTP transports)
- **Persistent daemon** with HTTP API, file watchers, and background indexing
- **Web dashboard** (Vue/TypeScript SPA) embedded in binary via rust-embed
- **Multi-agent dispatch** with backend auto-detection and streaming output parsing
- **Embedding pipeline** with GPU-accelerated Python sidecar + ORT CPU fallback
- **OpenAPI 3.1** spec with interactive Scalar docs

## Development

```bash
# Clone repository
git clone https://github.com/anortham/julie.git
cd julie

# Development build
cargo build

# Run tests
cargo test

# Production build
cargo build --release
```

## Testing

Julie uses a comprehensive testing methodology:

- **Unit tests** for all 31 language extractors
- **Real-world validation** against GitHub repositories
- **SOURCE/CONTROL methodology** for editing tools (original files vs expected results)
- **Coverage targets**: 80% general, 90% for editing tools

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Coverage analysis
cargo tarpaulin
```

## Project Structure

```
src/
├── main.rs          # CLI entry point (stdio / daemon / connect dispatch)
├── connect.rs       # Auto-start daemon + stdio↔HTTP bridge
├── daemon.rs        # Daemon lifecycle (start/stop/status, PID file)
├── server.rs        # HTTP server (axum router, startup, shutdown)
├── api/             # REST API modules (health, projects, search, memories, agents, dashboard)
├── extractors/      # Language-specific symbol extraction (31 languages)
├── database/        # SQLite structured storage
├── search/          # Tantivy search engine and tokenizer
├── embeddings/      # Embedding pipeline, sidecar supervisor and protocol
├── tools/           # MCP tool implementations
│   ├── deep_dive/   # Progressive-depth symbol investigation
│   ├── get_context/ # Token-budgeted context retrieval
│   ├── memory/      # checkpoint, recall, plan
│   ├── navigation/  # fast_refs
│   ├── refactoring/ # rename_symbol
│   ├── search/      # fast_search
│   ├── symbols/     # get_symbols
│   └── workspace/   # Workspace management and indexing
├── workspace/       # Multi-workspace management
└── tests/           # Test infrastructure

ui/                  # Vue/TypeScript dashboard (built assets embedded in binary)
python/
└── embeddings_sidecar/  # GPU-accelerated embedding sidecar (PyTorch + sentence-transformers)

fixtures/            # Test data (SOURCE/CONTROL files, real-world samples)
```

## License

MIT License - see [LICENSE](LICENSE) file for details

## Contributing

See [CLAUDE.md](CLAUDE.md) for development guidelines and architecture documentation.
