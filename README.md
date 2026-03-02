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

- **Fast symbol search** with code-aware tokenization
- **Cross-language code navigation** (go-to-definition, find-references)
- **AST-aware refactoring** with workspace-wide rename
- **Multi-workspace support** for indexing and searching related codebases (one workspace at a time)

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

### Download Pre-built Binaries (Recommended)

Download the latest release for your platform from the [Releases page](https://github.com/anortham/julie/releases):

**Windows:**

```bash
# Download julie-v1.5.0-x86_64-pc-windows-msvc.zip
# Extract julie-server.exe
# Add to MCP configuration (see below)
```

**macOS (Intel):**

```bash
# Download julie-v1.5.0-x86_64-apple-darwin.tar.gz
tar -xzf julie-v1.5.0-x86_64-apple-darwin.tar.gz
# Add to MCP configuration (see below)
```

**macOS (Apple Silicon):**

```bash
# Download julie-v1.5.0-aarch64-apple-darwin.tar.gz
tar -xzf julie-v1.5.0-aarch64-apple-darwin.tar.gz
# Add to MCP configuration (see below)
```

**Linux:**

```bash
# Download julie-v1.5.0-x86_64-unknown-linux-gnu.tar.gz
tar -xzf julie-v1.5.0-x86_64-unknown-linux-gnu.tar.gz
# Add to MCP configuration (see below)
```

### Configure as MCP Server

Once downloaded, add Julie to your MCP client:

**Claude Code (Recommended):**

```bash
# Windows
claude mcp add julie C:\path\to\julie-server.exe

# macOS/Linux
claude mcp add julie /path/to/julie-server
```

Claude Code automatically detects your workspace and creates `.julie/` folders in the correct location.

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

**Other MCP Clients:**

Add to your MCP client settings (e.g., `claude_desktop_config.json`):

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

If your MCP client doesn't support environment variables, Julie will use the current working directory as the workspace root.

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

## Tools (7)

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

## Architecture

- **Tree-sitter parsers** for accurate symbol extraction across all languages
- **Tantivy full-text search** with code-aware tokenization (CamelCase/snake_case splitting, English stemming)
- **Graph centrality ranking** using pre-computed reference scores from the relationship graph
- **SQLite storage** for symbols, identifiers, relationships, types, and file metadata
- **Per-workspace isolation** with separate databases and indexes
- **MCP protocol** for AI agent integration

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
├── extractors/      # Language-specific symbol extraction (31 languages)
├── database/        # SQLite structured storage
├── search/          # Tantivy search engine and tokenizer
├── embeddings/      # Embedding pipeline, sidecar supervisor and protocol
├── tools/           # MCP tool implementations
│   ├── deep_dive/   # Progressive-depth symbol investigation
│   ├── navigation/  # fast_refs
│   ├── refactoring/ # rename_symbol
│   ├── search/      # fast_search
│   ├── symbols/     # get_symbols
│   └── workspace/   # Workspace management
├── workspace/       # Multi-workspace management
└── tests/           # Test infrastructure

python/
└── embeddings_sidecar/  # GPU-accelerated embedding sidecar (PyTorch + sentence-transformers)

fixtures/            # Test data (SOURCE/CONTROL files, real-world samples)
```

## License

MIT License - see [LICENSE](LICENSE) file for details

## Contributing

See [CLAUDE.md](CLAUDE.md) for development guidelines and architecture documentation.
