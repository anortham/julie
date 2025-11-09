# Julie TODO

## 🎯 Current Status (2025-11-09)

**Latest Release**: v1.2.0 (2025-11-09)
**Languages Supported**: 31/31 ✅
**Architecture**: CASCADE (SQLite FTS5 → HNSW Semantic)

### ✅ Recent Completions

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

### Priority 1: Language Support Expansion
- ✅ Markdown (#28), JSON (#29), TOML (#30), YAML (#31) - Complete
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
