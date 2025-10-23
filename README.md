# Julie

A cross-platform code intelligence server built in Rust, providing LSP-quality features across 25 programming languages via the Model Context Protocol (MCP).

## Features

- **Fast symbol search** with text and semantic modes
- **Cross-language code navigation** (go-to-definition, find-references)
- **Intelligent code editing** with fuzzy matching and AST-aware refactoring
- **Multi-workspace support** for searching across related codebases
- **Call path tracing** across language boundaries

### Performance Characteristics

- Search latency: <10ms (text), <100ms (semantic)
- Memory usage: <100MB typical workload
- Startup time: <2s (database indexing - text search available immediately)
- Semantic indexing: 2-5 minutes per 10,000 symbols (background, non-blocking)
  - **GPU-accelerated** on Windows (DirectML) and Linux (CUDA/TensorRT)
  - Faster on Apple Silicon (~30-60s per 10,000 symbols, CPU-optimized)
  - Text search works instantly while semantic search builds
- Single binary deployment with no external dependencies

### GPU Acceleration

Julie automatically uses GPU acceleration for semantic embeddings when available:

- **Windows**: DirectML (supports NVIDIA, AMD, Intel GPUs)
- **Linux**: CUDA/TensorRT (NVIDIA GPUs)
- **macOS**: CPU-optimized (faster than CoreML for BERT models)

**Automatic CPU Fallback**: If GPU initialization or inference fails, Julie automatically detects the failure and reinitializes in CPU-only mode. This handles:
- Incompatible GPU drivers
- DirectML/CUDA crashes
- Remote Desktop sessions (GPU unavailable)
- Specific GPU/model incompatibilities

The fallback happens once at runtime with clear logging - no manual intervention needed. Machines with working GPUs continue using acceleration; machines with GPU issues fall back to stable CPU mode.

**Manual CPU Override**: Set the environment variable `JULIE_FORCE_CPU=1` to skip GPU entirely and use CPU-only mode from startup.

## Supported Languages (25)

**Core:** Rust, TypeScript, JavaScript, Python, Java, C#, PHP, Ruby, Swift, Kotlin

**Systems:** C, C++, Go, Lua

**Specialized:** GDScript, Vue, Razor, SQL, HTML, CSS, Regex, Bash, PowerShell, Zig, Dart

## Tools

### Search & Navigation
- `fast_search` - Unified text and semantic code search with multiple output modes (symbols/lines)
  - Search full file content or symbol definitions only
  - Text mode (<10ms), semantic mode (<100ms), or hybrid
  - Language and file pattern filtering
- `fast_goto` - Jump directly to symbol definitions across the workspace
- `fast_refs` - Find all references to a symbol with structured output
- `get_symbols` - Smart file reading with 70-90% token savings
  - View file structure without reading full content
  - Extract specific symbols with complete code bodies
  - Structure/minimal/full reading modes
- `trace_call_path` - Cross-language execution flow tracing
  - Upstream (who calls this) and downstream (what does this call)
  - Uses semantic similarity for cross-language matching

### Code Intelligence & Editing
- `find_logic` - Discover core business logic by filtering framework noise
- `fuzzy_replace` - Diff-match-patch fuzzy text replacement with validation
- `smart_refactor` - AST-aware semantic refactoring
  - Rename symbols across workspace
  - Replace function/method bodies
  - Insert code relative to symbols
  - Extract symbols to new files
- `edit_lines` - Surgical line-level editing (insert/replace/delete)

### Workspace Management
- `manage_workspace` - Index, add, remove, refresh, and clean workspaces

## Architecture

- **Tree-sitter parsers** for accurate symbol extraction across all languages
- **2-tier CASCADE search**: SQLite FTS5 (instant text search) → HNSW (semantic understanding)
- **Per-workspace isolation** with separate databases and indexes
- **ONNX embeddings** for semantic search capabilities
- **MCP protocol** for AI agent integration

## Building

```bash
# Development build
cargo build

# Run tests
cargo test

# Production build
cargo build --release
```

## Usage

Julie runs as an MCP server. First, build the release version:

```bash
cargo build --release
```

Then add it to Claude Code:

```bash
# Windows
claude mcp add julie C:\source\julie\target\release\julie-server.exe

# macOS/Linux
claude mcp add julie /path/to/julie/target/release/julie-server
```

Alternatively, manually configure in your MCP client settings:

```json
{
  "mcpServers": {
    "julie": {
      "command": "/path/to/julie-server",
      "args": []
    }
  }
}
```

The server will automatically index your workspace on first use. Text search is available immediately (<2s); semantic search builds in the background (2-5 minutes per 10,000 symbols, faster on Apple Silicon). You can use text search while semantic indexing completes.

## Testing

Julie uses a comprehensive testing methodology:

- **Unit tests** for all 25 language extractors
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
├── extractors/      # Language-specific symbol extraction (25 languages)
├── database/        # SQLite storage with FTS5 search
├── embeddings/      # ONNX semantic search
├── tools/           # MCP tool implementations
├── workspace/       # Multi-workspace management
└── tests/           # Test infrastructure

fixtures/            # Test data (SOURCE/CONTROL files, real-world samples)
```

## License

[To be determined]

## Contributing

See [CLAUDE.md](CLAUDE.md) for development guidelines and architecture documentation.
