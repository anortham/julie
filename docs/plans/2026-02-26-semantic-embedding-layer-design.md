# Semantic Embedding Layer — Design Document

> **Date:** 2026-02-26
> **Status:** Approved
> **Scope:** Add lightweight semantic embeddings to Julie for NL search quality and cross-language symbol bridging

## Problem Statement

Cross-workspace dogfood testing (2026-02-26) revealed that Julie's Tantivy-only search has a hard ceiling for natural language queries on non-Rust codebases:

1. **NL path prior is Julie-specific** — hardcoded `src/tests/`, `src/` patterns don't match C#, Python, Java, or any other project layout. The prior is a no-op for all non-Rust workspaces.
2. **Weak centrality in C# codebases** — important classes like `LuceneIndexService` have `ref_score: 0` because the C# extractor doesn't capture DI constructor injection patterns.
3. **Definition search doesn't over-fetch** — actual definitions rank below references in Tantivy and fall outside small result limits.
4. **Tantivy has a fundamental ceiling for concept queries** — keyword search cannot bridge vocabulary mismatch (e.g., "how does error handling work" → `CircuitBreakerService`).
5. **No cross-language symbol bridging** — `UserDto` (C#) and `IUser` (TypeScript) share zero tokens. Only semantic similarity can connect them.

### What We Tried

- **NL query expansion** (v3.3.3) — hand-curated aliases help but don't scale; essentially building a thesaurus manually.
- **NL path prior** (v3.3.3) — good idea, but language-specific implementation undermines it.
- **Previous semantic search (removed)** — ort had poor GPU acceleration, models weren't code-specific, and the project had gotten bloated.
- **Miller spinoff** — PyTorch + LanceDB works well but Python deployment is painful.

### Key Insight

The previous embedding implementation tried to embed full code bodies, requiring GPU acceleration and large models. Our actual need is much smaller: embed **symbol metadata** (name + signature + doc_comment, typically 20-100 tokens). A 33M-param model on CPU handles this workload in ~60s for 25K symbols, and <200ms for incremental updates.

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                   Julie MCP Server                    │
├─────────────────────────────────────────────────────┤
│  Tools: get_context, fast_search, deep_dive,         │
│         fast_refs, rename_symbol, get_symbols         │
├──────────┬──────────────┬───────────────────────────┤
│ SQLite   │   Tantivy    │   sqlite-vec (NEW)         │
│ symbols  │   keyword    │   vector search             │
│ rels     │   search     │   384-dim embeddings        │
│ types    │   custom     │   cosine similarity         │
│ files    │   tokenizer  │                             │
├──────────┴──────────────┴───────────────────────────┤
│           EmbeddingProvider trait (NEW)                │
│  ┌─────────────┐  ┌──────────────┐  ┌─────────────┐ │
│  │ort/fastembed │  │candle-coreml │  │  (future)   │ │
│  │  (default)   │  │(feature flag)│  │             │ │
│  └─────────────┘  └──────────────┘  └─────────────┘ │
└─────────────────────────────────────────────────────┘
```

### Design Decisions

| Decision | Choice | Alternatives Rejected | Reasoning |
|----------|--------|-----------------------|-----------|
| Keep Tantivy | Yes | Swap for LanceDB | LanceDB uses Tantivy as a black box — we'd lose our custom CodeTokenizer, field schema, weighted query builders, NL expansion, and scoring pipeline |
| Keep SQLite | Yes | Move to LanceDB | Our tool layer (deep_dive, get_context, fast_refs) depends deeply on SQLite's relational structure for relationships, types, and graph queries |
| Vector storage | sqlite-vec | BLOB+brute-force, LanceDB, pure-Rust HNSW | BLOB approach was tried previously and was too slow; sqlite-vec is now pure C with no deps; LanceDB would duplicate storage |
| ML runtime | EmbeddingProvider trait, ort default | Single runtime, Python sidecar | Trait allows swapping runtimes per platform; ort covers DirectML (Windows) + CUDA (Linux) + CPU; candle-coreml can be added for macOS Metal |
| Embedding scope | Symbol metadata only | Full code bodies | Keeps workload small enough for CPU; 20-100 tokens vs 500-2000 tokens; 33M model vs 137M+ model |
| Revive Miller | No | Use Miller as primary | Julie's tool layer is far ahead; Miller stays for Python ML stack users |

## Component Design

### 1. EmbeddingProvider Trait

```rust
/// Abstraction over ML inference runtimes for embedding generation.
/// Default: OrtEmbeddingProvider (fastembed-rs, BGE-small, 384 dims).
/// Future: CandleEmbeddingProvider (candle-coreml for macOS Metal).
pub trait EmbeddingProvider: Send + Sync {
    /// Embed a single query string (for search).
    fn embed_query(&self, text: &str) -> Result<Vec<f32>>;

    /// Embed a batch of symbol metadata strings (for indexing).
    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;

    /// Vector dimensions (e.g., 384 for BGE-small).
    fn dimensions(&self) -> usize;

    /// Hardware/device info for diagnostics.
    fn device_info(&self) -> DeviceInfo;
}
```

**Location:** `src/embeddings/mod.rs` (new module)

**Default implementation:** `OrtEmbeddingProvider` wrapping fastembed-rs with:
- Model: `BAAI/bge-small-en-v1.5` (33M params, 384 dims, 512 token context)
- Hardware: DirectML (Windows), CUDA (Linux), CPU fallback
- Batching: handled by fastembed-rs internally

### 2. Symbol Metadata Embedding

What gets embedded per symbol (short text string):

```
"{kind} {name} {signature} {doc_comment}"
```

Examples:
- `"class CircuitBreakerService : ICircuitBreakerService manages circuit breaker state for error handling"`
- `"method search_symbols query filter limit searches the Tantivy index for matching symbols"`
- `"struct SearchIndex index reader writer schema_fields full-text search engine wrapper"`

**Which symbols get embedded:**
- Functions, methods, classes, structs, interfaces, traits, enums, types, modules
- NOT: imports, fields, constants, variables (too noisy, low semantic value)

**Which symbols do NOT get embedded:**
- Symbols from non-embeddable languages (CSS, HTML — layout, not logic)
- Symbols without meaningful names (anonymous functions, lambdas)

### 3. Vector Storage (sqlite-vec)

Per-workspace, alongside existing SQLite database:

```sql
CREATE VIRTUAL TABLE IF NOT EXISTS symbol_vectors USING vec0(
    symbol_id TEXT PRIMARY KEY,
    embedding float[384]
);
```

**Query pattern:**
```sql
SELECT symbol_id, distance
FROM symbol_vectors
WHERE embedding MATCH ?query_vector
ORDER BY distance
LIMIT 20;
```

**Cross-platform validation required** — sqlite-vec gave us headaches before. Must verify on Windows, macOS, and Linux before committing to this path. If it fails again, fall back to a pure-Rust solution (instant-distance or similar).

### 4. Indexing Pipeline Integration

#### Initial Indexing (async, non-blocking)

```
Phase 1 (existing, immediate):
  tree-sitter extract → SQLite store → Tantivy populate
  → Keyword search available IMMEDIATELY

Phase 2 (new, background — same pattern as reference workspace Tantivy backfill):
  Load symbols from SQLite →
  Generate metadata strings →
  Embed batch via EmbeddingProvider →
  Store in symbol_vectors →
  → Semantic search available when complete (~15-60s on CPU)
```

#### Incremental Updates (filewatcher)

```
File changed →
  1. Extract symbols (existing, tree-sitter)
  2. Update SQLite + Tantivy (existing, atomic)
  3. Generate embeddings for changed symbols (NEW, ~5-50 symbols, <200ms CPU)
  4. Update symbol_vectors: delete old file's vectors, insert new (NEW)
```

Steps 3-4 are synchronous for incremental updates — the overhead is negligible (<200ms for a typical file). Only bulk initial indexing runs async.

#### Integration points:
- `process_files_optimized()` in `src/tools/workspace/indexing/processor.rs` — add Phase 2 background task
- `handle_file_created_or_modified_static()` in `src/watcher/handlers.rs` — add steps 3-4
- `incremental_update_atomic()` in `src/database/bulk_operations.rs` — extend to include vector updates

### 5. Tool Integration

#### `get_context` — Hybrid Search (biggest improvement)

```
Query →
  1. Tantivy keyword search (existing) → top 30 candidates
  2. sqlite-vec semantic search (NEW) → top 20 candidates
  3. Reciprocal Rank Fusion (RRF) to merge results
  4. Apply existing scoring pipeline (centrality, path prior) to merged set
  5. Select pivots from merged+scored candidates
```

RRF formula: `score(d) = Σ 1 / (k + rank_i(d))` where k=60 (standard constant).

This is where NL queries get the biggest improvement — semantic search fills the vocabulary gap that Tantivy can't bridge.

#### `fast_search` — Semantic Fallback for NL Queries

When `is_nl_like_query()` returns true and keyword results are weak (low scores / few results):
- Run semantic search as fallback
- Merge with keyword results
- Present combined results

For identifier queries (exact symbol names), Tantivy-only remains the path — embeddings add noise for exact matching.

#### `deep_dive`, `fast_refs`, `get_symbols`, `rename_symbol` — No Changes

These tools use structured graph queries (lookup by name, traverse relationships). Semantic search doesn't improve them.

#### Cross-Language Bridging (Phase 3)

New capability: find semantically similar symbols across language boundaries.

```
Given: UserDto (C# class, API endpoint DTO)
Find:  IUser (TypeScript interface, SPA model)
       user_model (Python class, data layer)
```

Could be exposed as:
- A `find_similar` parameter on `fast_search`
- A `semantic_refs` reference kind in `fast_refs`
- Both

### 6. Bug Fixes (Independent of Embeddings)

Ship first, regardless of embedding timeline:

1. **NL path prior → language-agnostic** (`src/search/scoring.rs:127-147`)
   - Replace: `path.starts_with("src/tests/")` etc.
   - With generic detection: path contains `test`, `tests`, `.Tests`, `_test`, `spec`, `__tests__`, `test_`
   - And generic source detection: not test, not docs, not config, not build artifacts

2. **Definition search over-fetch** (`src/tools/search/text_search.rs`)
   - When `search_target="definitions"`, fetch 3-5x the user's limit
   - Find exact name matches, promote to top
   - Trim to display limit

3. **C# centrality investigation**
   - Check why DI constructor injection doesn't create relationships
   - Check interface implementation relationships
   - Likely gap in C# extractor's relationship extraction

## Phased Delivery

### Phase 0: Bug Fixes (1-2 days, no new dependencies)
- Language-agnostic NL path prior
- Definition search over-fetch
- C# extractor relationship investigation
- **Exit criteria:** `get_context` on coa workspace returns code (not test files) for NL queries

### Phase 1: Embedding Foundation (3-5 days)
- `EmbeddingProvider` trait + `OrtEmbeddingProvider` (fastembed-rs)
- sqlite-vec integration with **cross-platform validation** (Windows, macOS, Linux)
- Symbol metadata embedding pipeline (what to embed, how to format)
- Background async indexing (Phase 2 pattern)
- Unit tests for embedding generation and vector storage
- **Exit criteria:** symbols have embeddings, KNN query returns semantically similar symbols

### Phase 2: Tool Integration (2-3 days)
- Hybrid search in `get_context` (Tantivy + sqlite-vec + RRF merger)
- Semantic fallback in `fast_search` for NL queries
- Incremental pipeline integration with filewatcher
- Dogfood testing across all 3 workspaces
- **Exit criteria:** "how does text search work" on coa returns `TextSearchTool`, `SearchAsync`, not test methods

### Phase 3: Cross-Language Bridging (2-3 days)
- `find_similar` capability for cross-language symbol discovery
- Semantic refs mode for cross-language tracing
- Quality evaluation across reference workspaces (polyglot scenarios)
- **Exit criteria:** `UserDto` (C#) → `IUser` (TS) semantic bridging works

### Phase 4: Hardware Expansion (future)
- candle-coreml feature flag for macOS Metal GPU acceleration
- Performance benchmarking across platforms
- Model evaluation (BGE-small vs code-specific models)

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| sqlite-vec cross-platform failures | Medium | High | Validate on all 3 platforms in Phase 1 before building on it. Fallback: pure-Rust HNSW (instant-distance) |
| ort/fastembed-rs rough edges | Low | Medium | fastembed-rs is battle-tested (234 commits, 30+ models). CPU-only path is simplest. |
| Embedding quality for code metadata | Medium | Medium | BGE-small is generic English. May need code-specific model later. Phase 4 includes model evaluation. |
| Cold indexing too slow on CPU | Low | Low | 15-60s for 25K symbols is acceptable. Async pipeline means keyword search is never blocked. |
| Binary size increase | Low | Low | fastembed-rs + ort adds ~20-30MB. Acceptable for the capability. |
| Breaking single-binary deployment | Low | High | ort bundles ONNX Runtime. fastembed-rs handles model download. No Python required. |

## Non-Goals

- **Full code body embeddings** — too expensive for CPU, diminishing returns for metadata-quality models
- **Replacing Tantivy** — keyword search is superior for exact symbol lookup
- **Replacing SQLite** — structured relational storage is the foundation for deep_dive, fast_refs, etc.
- **Real-time embedding during typing** — embeddings are for indexed symbols, not live queries beyond the query itself
- **Training custom models** — use pre-trained models; model evaluation is Phase 4
