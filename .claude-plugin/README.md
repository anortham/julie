

# Julie Code Intelligence Plugin

Cross-platform code intelligence server with LSP-quality features across 25 programming languages. Fast semantic search (<100ms), cross-language navigation, and safe refactoring tools built in Rust.

## Features

### 🎯 Intelligent Code Skills
- **explore-codebase** - Autonomous code exploration using semantic search and call tracing
- **safe-refactor** - Safe refactoring with reference checking and validation
- **smart-search** - Intelligent search mode selection (semantic vs text vs symbols)

### ⚡ Quick Commands
- `/index` - Index current workspace for code intelligence
- `/search <query>` - Smart code search with auto-mode detection
- `/symbols <file>` - Show file structure (70-90% token savings!)

### 🔧 12 Powerful MCP Tools
**Search & Navigation:**
1. fast_search - Unified search (semantic/text/symbols)
2. fast_goto - Jump to definitions
3. fast_refs - Find all references
4. get_symbols - Smart file reading with structure
5. trace_call_path - Cross-language execution tracing
6. find_logic - Business logic discovery
7. fast_explore - Architecture understanding

**Code Editing:**
8. edit_lines - Surgical line editing
9. fuzzy_replace - Diff-match-patch fuzzy replacement
10. rename_symbol - Workspace-wide AST-aware renaming
11. edit_symbol - Semantic code editing

**Workspace:**
12. manage_workspace - Index, add, remove workspaces

### 🚀 CASCADE Architecture
- **Instant Search**: SQLite FTS5 (<5ms) works immediately
- **Progressive Enhancement**: HNSW semantic search builds in background (~20-30s)
- **No Blocking**: Search available from second one
- **GPU Accelerated**: DirectML (Windows), CUDA (Linux), CPU-optimized (macOS)

### 🌍 25 Language Support
TypeScript, JavaScript, Python, Java, C#, PHP, Ruby, Swift, Kotlin, C, C++, Go, Rust, Lua, GDScript, Vue, Razor, SQL, HTML, CSS, Regex, Bash, PowerShell, Zig, Dart

## Installation

### Prerequisites
- **Rust toolchain** (for building Julie)
- **Bun or Node.js** (if using with other MCP servers)
- Optional: **CUDA 12.x + cuDNN 9** (Linux GPU acceleration)

### Build Julie
```bash
cd julie
cargo build --release
```

The binary will be at: `target/release/julie-server` (or `julie-server.exe` on Windows)

### Via Plugin Marketplace (Future)
```
/plugin install julie-code-intel@your-marketplace
```

### Manual Installation
1. Build Julie: `cargo build --release`
2. Add to `.claude/settings.json`:
```json
{
  "plugins": ["path/to/julie/.claude-plugin"]
}
```
3. Restart Claude Code

## Quick Start

### 1. Index Your Workspace
```bash
/index
```

This creates `.julie/` directory with indexes:
```
.julie/
├── indexes/
│   └── {workspace_id}/
│       ├── db/symbols.db        # SQLite + FTS5 (instant)
│       └── vectors/             # Semantic embeddings (background)
```

**Performance:**
- Small project (100 files): <1s
- Medium project (1000 files): ~2s
- Large project (10,000 files): ~10-15s
- Search works immediately, semantic enhances in background

### 2. Search Code
```bash
/search authentication logic      # Semantic search
/search "console.error"           # Exact string search
/search UserService               # Symbol search
```

### 3. Explore Files (Token-Efficient!)
```bash
/symbols src/services/user.ts
```

**Token savings:**
- Full file read: ~3,500 tokens
- Symbol structure: ~400 tokens
- **Savings: 89%** (3,100 tokens!)

### 4. Navigate Code
Use Julie's tools for deep exploration:
```
fast_goto({ symbol: "UserService" })
fast_refs({ symbol: "authenticate" })
trace_call_path({ symbol: "processPayment", direction: "downstream" })
```

## How It Works

### CASCADE Architecture (2-Tier)

```
Files → Tree-sitter → SQLite (single source of truth)
                         ├─ FTS5 full-text search (<5ms) ✅ Works instantly
                         └─ Symbols + relationships
                              ↓
                         HNSW Semantic (background, 20-30s)
                         └─ 384-dim embeddings ✅ Enhances progressively
```

**Key Benefits:**
- **<2s startup** (SQLite only)
- **Search works immediately** (FTS5)
- **Progressive enhancement** (HNSW adds semantic understanding)
- **No blocking** (background indexing)

### Per-Workspace Isolation

Each workspace gets its own isolated storage:
```
.julie/indexes/
├── primary_workspace_abc123/
│   ├── db/symbols.db
│   └── vectors/
└── reference_workspace_def456/
    ├── db/symbols.db
    └── vectors/
```

Benefits:
- Complete isolation between workspaces
- Trivial deletion (`rm -rf indexes/{id}/`)
- Smaller, faster indexes
- No cross-workspace contamination

## Skills Deep Dive

### explore-codebase Skill

**Activates when:** User wants to understand unfamiliar code

**Pattern:**
1. Semantic search for relevant code
2. get_symbols for structure (token-efficient!)
3. trace_call_path for execution flow
4. fast_refs for usage analysis

**Example:**
```
User: "How does authentication work?"

→ fast_search({ query: "authentication logic", mode: "semantic" })
  Finds: auth.ts, auth-service.ts, jwt.ts

→ get_symbols({ file: "auth.ts", mode: "structure" })
  Shows: Classes, methods, imports (400 tokens vs 3500!)

→ trace_call_path({ symbol: "authenticate", direction: "downstream" })
  Maps: authenticate → validateToken → jwt.verify → UserService.findById

Result: "Authentication uses JWT middleware (auth.ts) that validates
tokens and extracts user data via UserService."
```

### safe-refactor Skill

**Activates when:** User wants to rename or refactor code

**Pattern:**
1. fast_refs to check all references
2. Confirm impact with user (if >10 files)
3. rename_symbol or fuzzy_replace for edits
4. Verify completion

**Example:**
```
User: "Rename getUserData to fetchUserProfile"

→ fast_refs({ symbol: "getUserData" })
  Found: 8 references across 5 files

→ Confirm: "Safe to rename (8 references). Proceeding..."

→ rename_symbol({ old_name: "getUserData", new_name: "fetchUserProfile" })
  Updated: 8 locations across 5 files

Result: "✅ Renamed getUserData → fetchUserProfile (8 references updated)"
```

### smart-search Skill

**Activates when:** User wants to find code

**Intelligence:**
- Concept queries → semantic mode
- Exact strings → text mode
- Symbol names → symbols mode

**Example:**
```
"authentication logic" → semantic (concept)
"console.error" → text (exact string)
"UserService" → symbols (definition)
```

## Advanced Features

### Cross-Language Call Tracing

**Unique capability:** Trace execution across language boundaries

```typescript
// TypeScript
processPayment(data);
```

```
→ trace_call_path({ symbol: "processPayment", direction: "downstream" })

TypeScript: processPayment()
  → Rust: payment_processor::process()  ← CROSSES BOUNDARY
    → SQL: sp_charge_card()              ← CROSSES AGAIN
```

### Fuzzy Code Replacement

**Professional diff-match-patch algorithm:**

```
fuzzy_replace({
  file_path: "src/auth.ts",
  old_string: "console.error('Auth failed',error);",  // Tolerates whitespace
  new_string: "logger.error('Auth failed', { error });",
  similarity_threshold: 0.8
})
```

**Benefits:**
- Handles whitespace variations
- UTF-8 safe (character-based)
- Validates before commit
- Professional editing quality

### Symbol Extraction

**25 language extractors with comprehensive test coverage (636 tests passing):**

Each extractor understands:
- Classes, functions, methods
- Imports, exports, dependencies
- Call relationships
- Symbol scopes and visibility

## GPU Acceleration

### Platform Support

**Windows - DirectML (Works out of box)**
- NVIDIA, AMD, Intel GPUs
- Pre-built binaries included
- No setup required

**Linux - CUDA (Requires setup)**
- NVIDIA GPUs only
- Requires CUDA 12.x + cuDNN 9
- [Setup instructions](../README.md#gpu-acceleration)

**macOS - CPU-optimized**
- Faster than CoreML for transformers
- Optimized ONNX inference
- No GPU needed

### Performance

**With GPU:**
- Embedding generation: ~20-30s for large codebases
- 10-100x faster than CPU
- Real-time semantic search

**Without GPU (CPU fallback):**
- Still fast with ONNX optimizations
- ~2-3 minutes for large codebases
- Automatic fallback, no configuration needed

## Command Reference

### /index [path]
Index workspace for code intelligence.

```bash
/index                    # Index current directory
/index /path/to/project   # Index specific path
/index --reindex          # Force re-index
```

### /search <query> [flags]
Smart code search with automatic mode selection.

```bash
/search authentication            # Auto-detect mode
/search "console.error"           # Exact match
/search --semantic payments       # Force semantic
/search --text "import"           # Force text
/search --symbols UserService     # Force symbols
/search --limit 20 auth           # Return 20 results
```

### /symbols <file> [mode]
Show file structure without reading entire file.

```bash
/symbols src/user.ts              # Structure (default)
/symbols src/user.ts --full       # Full details
/symbols src/user.ts --definitions # Just symbol list
```

## Tool Usage Examples

### Fast Search
```
fast_search({
  query: "authentication logic",
  mode: "semantic",
  limit: 10
})
```

### Get Symbols (Token Savings!)
```
get_symbols({
  file_path: "src/services/user.ts",
  mode: "structure"
})
```

### Trace Execution
```
trace_call_path({
  symbol: "authenticate",
  direction: "downstream",
  max_depth: 5
})
```

### Safe Rename
```
rename_symbol({
  old_name: "getUserData",
  new_name: "fetchUserProfile"
})
```

### Fuzzy Replace
```
fuzzy_replace({
  file_path: "src/auth.ts",
  old_string: "console.error(...)",
  new_string: "logger.error(...)",
  similarity_threshold: 0.8
})
```

## Integration with Other Plugins

### With Goldfish Memory
```
[Before refactoring]
→ Goldfish: checkpoint({ description: "Pre-refactor state" })

[Use Julie for safe refactoring]
→ Julie: rename_symbol(...)

[After refactoring]
→ Goldfish: checkpoint({ description: "Refactored X to Y, 8 files" })
```

### With Sherpa Workflows
```
[TDD Workflow - Phase 1: Define Contract]
→ Julie: fast_search for existing patterns
→ Design interface based on findings

[Phase 2: Write Tests]
→ Use Julie to navigate test examples

[Phase 3: Implement]
→ Julie: get_symbols to see structure
→ Implement features

[Phase 4: Refactor]
→ Julie: safe refactoring with rename_symbol
```

## Performance Targets

Julie meets these benchmarks:

- **Search Latency**: <5ms SQLite FTS5, <100ms Semantic
- **Startup Time**: <2s (SQLite only)
- **Indexing Speed**: 1000 files in <2s
- **Background Indexing**: HNSW semantic 20-30s (GPU)
- **Memory Usage**: <100MB typical (~500MB with large indexes)
- **Token Savings**: 70-90% with get_symbols

## Troubleshooting

### Workspace not indexed
```bash
/index                    # Index current workspace
```

Check: `.julie/` directory should exist in project root

### Slow semantic search
- Ensure GPU acceleration working (check logs)
- Wait for background indexing to complete (~20-30s)
- Try text search mode for faster results

### Symbol extraction missing
- Verify file language is supported (25 languages)
- Check file isn't binary or corrupted
- Try re-indexing: `/index --reindex`

### Search returns no results
- Verify workspace is indexed
- Try different search mode (/search --semantic vs --text)
- Check spelling and terminology

## Storage & Performance

### Disk Usage
- **SQLite database**: ~1-5MB per 1000 files
- **HNSW vectors**: ~50-100MB per 1000 files (with embeddings)
- **Total**: ~50-100MB per 1000 files

### Cleanup
```bash
# Remove all indexes
rm -rf .julie/indexes/

# Re-index
/index
```

## Requirements

- **Build**: Rust 1.75+ (for compilation)
- **Runtime**: Native binary (no dependencies)
- **Optional**: CUDA 12.x + cuDNN 9 (Linux GPU)
- **Claude Code**: Latest version

## Architecture

Julie uses a sophisticated 2-tier CASCADE architecture:

1. **SQLite Layer**: FTS5 full-text search (<5ms)
   - Works immediately on startup
   - BM25 ranking
   - Boolean AND/OR logic
   - Single source of truth

2. **Semantic Layer**: HNSW vector search (<100ms)
   - Builds in background (20-30s)
   - GPU-accelerated embeddings
   - 384-dim BGE model
   - Progressive enhancement

**Result:** Instant search availability with progressive semantic enhancement!

## License

MIT - See LICENSE file in the julie directory

---

**Built with Rust for native performance and true cross-platform compatibility. 25 languages. 12 tools. <2s startup. Let's explore some code! 🚀**
