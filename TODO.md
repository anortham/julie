# Julie TODO

## 🎯 Current Status (2025-11-10)

**Latest Release**: v1.5.0 (2025-11-10)
**Latest Development**: Critical Performance Fix + Memory System ✅
**Languages Supported**: 31/31 ✅
**Architecture**: CASCADE (SQLite FTS5 → HNSW Semantic)

### ✅ Recent Completions

**v1.5.1 - JSONC Support (2025-11-10)**
- 📄 Added JSONC (JSON with Comments) file extension support
- ✅ VSCode config files (tsconfig.json, settings.json) now fully indexed
- 🧪 4 comprehensive test cases for line comments, block comments, and real-world configs
- 📝 Zero new dependencies - reuses existing tree-sitter-json parser

**v1.5.0 - Critical Performance Fix + Memory System (2025-11-10)**
- 🚀 Fixed incremental indexing path mismatch causing 100% cache miss on startup
- ⚡ 53% startup time reduction (8.4s → 3.9s) - now correctly skips 741/786 unchanged files
- ⚡ **Background auto-indexing**: Server now starts instantly, indexing runs in background after MCP handshake
  - Moved auto-indexing from `main()` to `on_initialized()` callback in handler.rs
  - Zero startup delay - instant MCP server availability regardless of workspace size
- 🐛 Fixed checkpoint tool crash from string slice panic on short git hashes
- 💾 Complete Phase 1 memory system (checkpoint/recall tools, SQL views, 26 tests)
- 📝 Updated tool descriptions with behavioral adoption patterns for proactive usage
- 🔧 Memory files in `.memories/` are git-tracked, human-readable JSON

**Phase 1 Memory System - Complete (2025-11-10)**
- ✅ Checkpoint tool: Save immutable memories (checkpoints, decisions, learnings)
- ✅ Recall tool: Query memories chronologically with filtering
- ✅ Storage: `.memories/` directory (separate from `.julie/` internal state)
- ✅ Architecture: Pretty-printed JSON files, git-trackable, atomic writes
- ✅ Indexing: Automatic via file watcher, searchable with fast_search
- ✅ Clean implementation: Whitelisted `.memories`, no complex path logic
- 📍 See: `docs/JULIE_2_PLAN.md` for Phase 2 (mutable plans) roadmap

**v1.2.0 - Embedding Fixes for Goldfish Integration**
- Fixed EmbeddingEngine to support standalone mode (no dummy databases)
- Enhanced error diagnostics for failed embeddings (warn level with previews)
- Removed unsafe env::set_var() mutation, CPU-only fallback now safe
- Added download timeouts (5min model, 1min tokenizer)
- Embedding dimension validation to detect model corruption

**v1.1.4 - Query Expansion Quality Improvements**
- Query expansion fully wired and functional
- Symbol name relevance checking improves precision
- CamelCase/PascalCase queries now find snake_case functions
- Multi-word query handling improved

**v1.1.3 - YAML Support (Language #31)**
- YAML extractor for CI/CD configs (GitHub Actions, Kubernetes, Docker Compose)
- All 14 YAML extractor tests passing

**v1.1.1 - Database Robustness**
- SQLite corruption fixed (WAL mode + proper shutdown)
- Enhanced RAG embeddings with code_context
- Semantic search fallback when text search returns 0 results

**v1.1.0 - RAG POC Complete**
- 88.9% average token reduction validated (83-94% range)
- Enhanced markdown content extraction
- Both FTS5 (text) and HNSW (semantic) search operational

---

## 🎯 Active Priorities

### Priority 0: Embedding Optimization (In Progress - 2025-11-11)

**Context**: Database grew 4x (50MB → 203MB, 2.8K → 11.4K symbols), embedding generation significantly slower

**Audit Findings**:
- Current embedding per symbol: `name + kind + signature + doc_comment + code_context`
- Average: 1,482 chars (88% is code_context at 1,304 chars avg)
- Problem: BGE-Small truncates at 512 tokens (~2KB), so massive text gets truncated anyway
- Outliers: Test fixtures with 479KB code_context fields (99.6% truncated!)
- 11,944 embeddings total (11,440 symbols + some have multiple models)

**Phase 1: Remove Noise (Target: 75-88% faster embedding generation)** ✅ Complete
- ✅ Audit completed - identified code_context as primary bottleneck
- ✅ Removed code_context from `build_embedding_text()` - focus on semantic units
- ✅ Added `fixtures/` to `.julieignore` - exclude 761 test fixture symbols
- ⏳ Test search quality - validate no degradation (requires reindex + testing)
- Expected: Embedding time 75-88% faster, search quality same or better

**Phase 2: Fix Memory Embeddings (Target: Proper RAG patterns)**
- Custom pipeline for `.memories/` files
- Embed only `description` field as focused semantic unit
- Prefix with type: `"checkpoint: <description>"` or `"decision: <description>"`
- Replace current 10 symbols/memory → 1 focused embedding/memory

**Phase 3: Validation & Metrics**
- Measure embedding generation time before/after
- Compare search relevance scores
- Document findings in Key Learnings

**RAG Principle**: Embed semantically meaningful units (one concept per embedding), not random surrounding code

### Priority 1: Language Support Expansion
- ✅ Markdown (#28), JSON (#29), TOML (#30), YAML (#31), JSONC (#32) - Complete
- ⏸️ Dockerfile - Blocked on tree-sitter-dockerfile 0.25+ compatibility (crate uses 0.20)
- Consider: Plain text (.txt), CSV for structured data
- Consider: Additional doc formats (PDF, DOCX) if needed for RAG

### Priority 2: Search Quality Improvements
- Monitor FTS5 + query expansion performance in production use
- Consider advanced query expansion patterns based on usage
- Search result ranking improvements
- Cross-reference discovery (code ↔ docs linking)

### Priority 3: Production Polish
- Agent onboarding flow improvements
- Query suggestion system
- Performance monitoring and optimization

---

## 📝 Scratchpad / Investigation Notes

<!-- Use this section for temporary notes during development -->
<!-- Clear out resolved items regularly -->

### Active Investigations

(None currently - add notes here as needed)

### Questions to Explore

- Should we add a dedicated semantic search tool (vs mode in fast_search)?
- Which other tools could benefit from semantic search layer?
- Additional file types for RAG? (PDF, DOCX parsing considerations)

---

## 📊 Key Metrics

**Token Reduction (RAG)**: 88.9% average (83-94% range)
**Search Performance**: <5ms (FTS5), <50ms (HNSW semantic)
**Language Coverage**: 31 languages with tree-sitter parsers
**Test Coverage**: Comprehensive (SOURCE/CONTROL methodology)

---

## 🧠 Key Learnings

1. **Simpler is better** - Unified symbols table works perfectly, no knowledge_embeddings complexity needed
2. **Content extraction is critical** - Full section bodies (not just headings) enable true RAG token reduction
3. **SQLite FTS5 + Query Expansion is sufficient** - Performs well with proper query preprocessing
4. **Test-driven validation works** - Measured token reduction proves the value proposition
5. **Tree-sitter version compatibility is fragile** - See TREE_SITTER_WARNING.md before updating deps
