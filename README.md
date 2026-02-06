# Julie

A cross-platform code intelligence server built in Rust, providing LSP-quality features across 31 programming languages via the Model Context Protocol (MCP).

## Features

- **Fast symbol search** with code-aware tokenization
- **Cross-language code navigation** (go-to-definition, find-references)
- **Intelligent code editing** with fuzzy matching and AST-aware refactoring
- **Development memory system** - checkpoint and recall significant development moments
- **Multi-workspace support** for indexing and searching related codebases (one workspace at a time)
- **Call path tracing** across language boundaries

### Performance Characteristics

- Search latency: <5ms (Tantivy full-text search)
- Memory usage: <100MB typical workload
- Startup time: <2s (database + Tantivy indexing)
- Single binary deployment with no external dependencies

**Incremental Updates**: Only changed files are re-indexed, typically completing in 3-15 seconds.

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

## Tools

### Search & Navigation

- `fast_search` - Full-text code search with code-aware tokenization and multiple output modes (symbols/lines)
  - Search full file content or symbol definitions only
  - <5ms search latency with CamelCase/snake_case splitting
  - Language and file pattern filtering
- `fast_goto` - Jump directly to symbol definitions across the workspace
- `fast_refs` - Find all references to a symbol with structured output
- `get_symbols` - Smart file reading with 70-90% token savings
  - View file structure without reading full content
  - Extract specific symbols with complete code bodies
  - Structure/minimal/full reading modes
- `trace_call_path` - Cross-language execution flow tracing
  - Upstream (who calls this) and downstream (what does this call)
  - Uses naming convention variants for cross-language matching

### Code Intelligence & Editing

- `fuzzy_replace` - Diff-match-patch fuzzy text replacement with validation
- `rename_symbol` - Rename symbols across entire workspace
  - Updates all references atomically
  - Preview mode with dry_run parameter
- `edit_symbol` - AST-aware symbol editing
  - Replace function/method bodies
  - Insert code relative to symbols
  - Extract symbols to new files
- `edit_lines` - Surgical line-level editing (insert/replace/delete)

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
- **Tantivy full-text search** with code-aware tokenization (CamelCase/snake_case splitting)
- **SQLite storage** for symbols, identifiers, relationships, and file metadata
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
├── database/        # SQLite structured storage
├── search/          # Tantivy search engine and tokenizer
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
