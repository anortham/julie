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
- **Stdio MCP server** — single binary, zero configuration, works with any MCP client

### Performance Characteristics

- Search latency: <5ms (Tantivy full-text search)
- Memory usage: <100MB typical workload
- Startup time: <2s (database + Tantivy indexing)
- Single binary server deployment with optional GPU-accelerated embedding sidecar

**Incremental Updates**: Only changed files are re-indexed, typically completing in 3-15 seconds.

### Embeddings Runtime

Julie uses a managed Python sidecar for GPU-accelerated semantic embeddings on macOS and Linux, and ONNX Runtime with DirectML on Windows (BGE-small-en-v1.5, 384 dimensions).

- **Auto-provisioning**: If `uv` is available and no compatible Python 3.10-3.13 is found, Julie installs one via `uv python install` and creates a managed venv with `uv venv`
- **GPU acceleration**: Uses CUDA via the Python sidecar on Linux, MPS via the Python sidecar on macOS, and DirectML via ONNX Runtime on Windows — falls back to CPU if no GPU is available
- **Fallback**: If the preferred accelerated runtime fails to initialize, Julie falls back to an available CPU path — keyword search always remains available
- **Zero configuration**: Works out of the box on systems with `uv` or a compatible Python on PATH

**Runtime controls:**
- `JULIE_EMBEDDING_PROVIDER`: `auto|sidecar|ort` (default: `auto`; Windows resolves to `ort`, macOS/Linux prefer `sidecar`)
- `JULIE_EMBEDDING_STRICT_ACCEL`: `1` to disable embeddings when no GPU is available
- See `docs/operations/embedding-sidecar.md` for all env vars and troubleshooting

## Supported Languages (31)

**Core:** Rust, TypeScript, JavaScript, Python, Java, C#, PHP, Ruby, Swift, Kotlin

**Systems:** C, C++, Go, Lua, Zig

**Specialized:** GDScript, Vue, QML, R, Razor, SQL, HTML, CSS, Regex, Bash, PowerShell, Dart

**Documentation:** Markdown, JSON, JSONL, TOML, YAML

## Installation

### Build from Source

```bash
git clone https://github.com/anortham/julie.git
cd julie
cargo build --release
```

### Connect Your AI Tool

**Claude Code:**

```bash
claude mcp add julie -- /path/to/julie/target/release/julie-server
```

**VS Code with GitHub Copilot** (`.vscode/mcp.json`):

```json
{
  "servers": {
    "Julie": {
      "type": "stdio",
      "command": "/path/to/julie/target/release/julie-server"
    }
  }
}
```

**Cursor / Windsurf / Other MCP Clients:**

```json
{
  "mcpServers": {
    "julie": {
      "command": "/path/to/julie/target/release/julie-server"
    }
  }
}
```

**First Use:**

Julie indexes your workspace automatically on first connection (~2-5s for most projects). All search capabilities are available immediately after indexing completes.

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
- **MCP protocol** over stdio (JSON-RPC)
- **Embedding pipeline** with GPU-accelerated Python sidecar + ORT CPU fallback

## Development

### Prerequisites

- **Rust** (stable, 1.80+) — [rustup.rs](https://rustup.rs)
- **Python 3.10-3.13** + **uv** (optional) — only needed for GPU-accelerated embeddings; keyword search works without it

### Building

```bash
git clone https://github.com/anortham/julie.git
cd julie
cargo build
```

### Running Locally

Julie is a stdio MCP server — it communicates via JSON-RPC over stdin/stdout:

```bash
cargo run -- --workspace /path/to/your/project
```

To test with an MCP client, point it at your debug build:

```bash
claude mcp add julie-dev -- /path/to/julie/target/debug/julie-server
```

After rebuilding (`cargo build`), restart Claude Code to pick up the new binary.

### Testing

Julie has a tiered test strategy to keep iteration fast:

| Tier | Command | Time | When to use |
|------|---------|------|-------------|
| **Fast** | `cargo test --lib -- --skip search_quality` | ~15s | After every change |
| **Dogfood** | `cargo test --lib search_quality` | ~250s | After search/scoring changes |
| **Full** | `cargo test --lib` | ~265s | Before merging |

```bash
# Fast tier (recommended during development)
cargo test --lib -- --skip search_quality

# Run specific test modules
cargo test --lib tests::tools::deep_dive     # deep_dive tests
cargo test --lib tests::tools::search        # search tests
cargo test --lib tests::core::database       # database tests

# Run extractor tests (separate crate)
cargo test -p julie-extractors
```

The dogfood tests load a 100MB SQLite fixture and run real searches — they're regression guards, not unit tests.

## Project Structure

```
src/
├── main.rs          # Stdio MCP entry point
├── handler.rs       # MCP tool handler (rmcp ServerHandler)
├── startup.rs       # Workspace initialization and staleness detection
├── cli.rs           # CLI argument parsing
├── extractors/      # Language-specific symbol extraction (31 languages)
├── database/        # SQLite structured storage
├── search/          # Tantivy search engine and tokenizer
├── embeddings/      # Embedding pipeline, sidecar supervisor and protocol
├── tools/           # MCP tool implementations
│   ├── deep_dive/   # Progressive-depth symbol investigation
│   ├── get_context/ # Token-budgeted context retrieval
│   ├── navigation/  # fast_refs
│   ├── refactoring/ # rename_symbol
│   ├── search/      # fast_search
│   ├── symbols/     # get_symbols
│   └── workspace/   # Workspace management and indexing
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
