# Julie

A cross-platform code intelligence server built in Rust, providing LSP-quality features across 31 programming languages via the Model Context Protocol (MCP).

## Features

- **Fast symbol search** with text and semantic modes
- **Cross-language code navigation** (go-to-definition, find-references)
- **Intelligent code editing** with fuzzy matching and AST-aware refactoring
- **Development memory system** - checkpoint and recall significant development moments
- **Multi-workspace support** for indexing and searching related codebases (one workspace at a time)
- **Call path tracing** across language boundaries

### Performance Characteristics

- Search latency: <10ms (text), <100ms (semantic)
- Memory usage: <100MB typical workload
- Startup time: <2s (database indexing - text search available immediately)
- Single binary deployment with no external dependencies

**Semantic Indexing Performance** (background, non-blocking):

- **Windows (GPU via DirectML)**: ~30 seconds per 10,000 symbols
- **macOS (CPU-optimized)**: ~1-3 minutes per 10,000 symbols
- **Linux (CPU default)**: ~5-10 minutes per 10,000 symbols
  - **Linux (GPU via CUDA)**: ~30 seconds per 10,000 symbols (requires CUDA 12.x + cuDNN 9 - see GPU Acceleration section below)

**Incremental Updates**: Only changed files are re-indexed, typically completing in 3-15 seconds regardless of platform.

**First-time setup**: Initial workspace indexing happens once and runs in the background. Text search is available immediately; semantic search becomes available after embeddings complete. Most workflows use incremental updates (fast) rather than full re-indexing (one-time cost).

### GPU Acceleration

Julie automatically uses GPU acceleration for semantic embeddings when available:

- **Windows**: DirectML (supports NVIDIA, AMD, Intel GPUs) - **✅ Enabled by default**
- **Linux**: CUDA support built-in - **⚠️ Requires CUDA 12.x + cuDNN 9**
  - Pre-built binaries use CUDA 12.x libraries (CUDA 13+ not compatible due to symbol versioning)
  - Install CUDA 12.6 from [NVIDIA Developer](https://developer.nvidia.com/cuda-12-6-0-download-archive)
  - Install cuDNN 9 from [NVIDIA Developer](https://developer.nvidia.com/cudnn-downloads)
  - Add to library path: `export LD_LIBRARY_PATH=/usr/local/cuda-12/lib64:$LD_LIBRARY_PATH`
  - **CPU fallback automatic** if CUDA libraries not found
- **macOS**: CPU-optimized (faster than CoreML for BERT/transformer models)

**Automatic CPU Fallback**: If GPU initialization or inference fails, Julie automatically detects the failure and reinitializes in CPU-only mode. This handles:

- Missing or incompatible CUDA/cuDNN libraries
- Incompatible GPU drivers
- DirectML/CUDA crashes during inference
- Remote Desktop sessions (GPU unavailable)
- Specific GPU/model incompatibilities

The fallback happens once at runtime with clear logging - no manual intervention needed. Machines with working GPUs continue using acceleration; machines with GPU issues fall back to stable CPU mode.

**Manual CPU Override**: Set the environment variable `JULIE_FORCE_CPU=1` to skip GPU entirely and use CPU-only mode from startup.

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

Julie will automatically index your workspace on first use:

- **Text search**: Available immediately (~2s)
- **Semantic search**: Builds in background (30s-3min depending on workspace size and GPU)

You can start searching with text mode while semantic indexing completes.

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

### Development Memory

- `checkpoint` - Save immutable development memories (bug fixes, decisions, learnings)
  - **Never ask permission** - create checkpoints proactively after significant work
  - Automatically captures git context (branch, commit, dirty state)
  - Stored as human-readable JSON in `.memories/` directory
  - Performance: <50ms per checkpoint
- `recall` - Query development history with filtering
  - Filter by type (checkpoint, decision, learning, observation)
  - Date range filtering (since/until)
  - Returns most recent memories first
  - Use for understanding past decisions and avoiding repeated mistakes
  - Performance: <5ms for chronological queries

**Memory System Benefits:**
- Build persistent context across sessions
- Understand why architectural decisions were made
- Learn from previous debugging sessions
- Create searchable development history (use `fast_search` with `file_pattern=".memories/**"`)


## Architecture

- **Tree-sitter parsers** for accurate symbol extraction across all languages
- **2-tier CASCADE search**: SQLite FTS5 (instant text search) → HNSW (semantic understanding)
- **Per-workspace isolation** with separate databases and indexes
- **ONNX embeddings** for semantic search capabilities
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
- **Memory system integration tests** (26 tests covering checkpoint/recall/SQL views)
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
├── database/        # SQLite storage with FTS5 search
├── embeddings/      # ONNX semantic search
├── tools/           # MCP tool implementations
│   ├── memory/      # Development memory system (checkpoint/recall)
│   ├── search/      # Search tools (fast_search, fast_goto, fast_refs)
│   ├── editing/     # Code editing tools (fuzzy_replace, smart_refactor)
│   └── workspace/   # Workspace management
├── workspace/       # Multi-workspace management
└── tests/           # Test infrastructure

fixtures/            # Test data (SOURCE/CONTROL files, real-world samples)
.memories/           # Development memories (checkpoints, decisions, learnings)
```

## License

MIT License - see [LICENSE](LICENSE) file for details

## Contributing

See [CLAUDE.md](CLAUDE.md) for development guidelines and architecture documentation.
