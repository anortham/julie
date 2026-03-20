# Julie

**[Website](https://anortham.github.io/julie/)** · **[Installation](#installation)** · **[Tools](#tools-8)** · **[Skills](#skills)** · **[33 Languages](#supported-languages-33)**

A cross-platform code intelligence server built in Rust, providing LSP-quality features across 33 programming languages via the Model Context Protocol (MCP).

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
- **Cross-language code navigation** (go-to-definition, find-references) across 33 languages
- **Code Health Intelligence** — automatic test detection, test quality metrics, change risk scores, and security risk signals computed at index time and surfaced in tool output
- **AST-aware refactoring** with workspace-wide rename and dry-run preview
- **Operational metrics** — per-tool timing, context efficiency tracking, "bytes NOT injected" headline metric
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
- `JULIE_EMBEDDING_SIDECAR_MODEL_ID`: Override the embedding model (default: `BAAI/bge-small-en-v1.5`). For better code similarity, use `nomic-ai/CodeRankEmbed` (768d, requires sidecar). The model downloads once to `~/.cache/huggingface/` and is shared across all projects. Switching models automatically wipes and re-embeds all vectors on the next indexing run; no manual cleanup needed.
- See `docs/operations/embedding-sidecar.md` for all env vars and troubleshooting

## Supported Languages (33)

**Core:** Rust, TypeScript, JavaScript, Python, Java, C#, PHP, Ruby, Swift, Kotlin, Scala

**Systems:** C, C++, Go, Lua, Zig

**Functional:** Elixir

**Specialized:** GDScript, Vue, QML, R, Razor, SQL, HTML, CSS, Regex, Bash, PowerShell, Dart

**Documentation:** Markdown, JSON, TOML, YAML

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
  - Pivots include change risk and security risk labels for immediate risk awareness
  - Adaptive budget: few results → deep context, many results → broad overview
  - Use at the start of a task for area-level orientation
- `deep_dive` - Progressive-depth symbol investigation
  - Overview (~200 tokens), context (~600 tokens), or full (~1500 tokens) detail levels
  - Kind-aware: functions show callers/callees/types, traits show implementations, structs show fields/methods
  - Includes test locations with quality tiers, change risk scores, and security risk signals
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

### Code Health & Metrics

- `query_metrics` - Query code health metrics or operational performance stats
  - `category: "code_health"` (default) — sort by security risk, change risk, centrality, or test coverage with filters for risk level, test status, symbol kind, file pattern, and language
  - `category: "session"` — current session stats: per-tool call counts, average latency, output bytes, and context efficiency (source bytes examined vs output returned)
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

## Code Health Intelligence

Julie automatically analyzes your codebase for test coverage quality and structural risk during indexing — no configuration required. These signals appear directly in `deep_dive` and `get_context` output, giving agents immediate awareness without extra tool calls.

### Test Intelligence

- **Test detection** across all 33 languages — recognizes `#[test]`, `@Test`, `pytest`, `describe`/`it`, and language-specific test patterns
- **Test quality metrics** — assertion density, mock usage, error path coverage, classified as thorough/adequate/thin/stub
- **Test-to-code linkage** — maps which tests exercise each production function via call graph and identifier analysis
- **Smart test filtering** — `fast_search` supports `exclude_tests` parameter to filter test symbols from results

### Risk Scoring

- **Change risk** (0.0–1.0) — "how dangerous is it to modify this?" based on centrality, visibility, test coverage quality, and symbol kind. Displayed as HIGH/MEDIUM/LOW in `deep_dive` with full factor breakdown, and as labels on `get_context` pivots.
- **Security risk** (0.0–1.0) — "does this code have structural security concerns?" based on five signals:
  - **Exposure** — public callable functions score highest
  - **Input handling** — detects string/Request/Query parameter types in signatures
  - **Sink calls** — one-hop detection of calls to exec/eval/execute/query patterns
  - **Blast radius** — how many other symbols depend on this one
  - **Untested** — no test coverage for security-critical code

Example `deep_dive` output:
```
Change Risk: MEDIUM (0.66) — 8 callers, public, thorough tests
Security Risk: HIGH (0.84) — calls execute; public; accepts string params
  sink calls: execute
  untested: yes
```

## Skills

Julie ships with 11 pre-built skills — reusable prompt workflows that combine Julie's tools into higher-level capabilities. Skills are invoked as slash commands (e.g., `/codehealth`) in harnesses that support them, or used as system prompt instructions.

### Report Skills

| Skill | Description |
|-------|-------------|
| `/codehealth` | Risk hotspots, test gaps, dead code candidates, and prioritized recommendations |
| `/security-audit` | Security risk analysis with plain-language explanations of risky patterns |
| `/architecture` | Architecture overview — entry points, module map, dependency flow, suggested reading order |
| `/metrics` | Session performance report — tool usage, timing, and context efficiency (bytes NOT injected) |

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

For now, do not treat `system` or `full` as green-by-default right now: a pre-existing `workspace_init` issue is still being worked through in that path.

## Project Structure

```
src/
├── main.rs          # Stdio MCP entry point
├── handler.rs       # MCP tool handler (rmcp ServerHandler)
├── startup.rs       # Workspace initialization and staleness detection
├── cli.rs           # CLI argument parsing
├── extractors/      # Language-specific symbol extraction (33 languages)
├── analysis/        # Post-indexing analysis (test quality, coverage, risk scoring)
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
