# Julie RAG Transformation: From Code Intelligence to Codebase Understanding

**Last Updated:** 2025-11-05
**Status:** Planning / POC Phase
**Strategic Priority:** High - Fundamental evolution of Julie's capabilities

---

## Executive Summary

This document outlines the transformation of Julie from an LSP-quality code intelligence tool into a **Retrieval-Augmented Generation (RAG) system for codebases**. This evolution represents a fundamental shift in how AI agents interact with software projects.

**Core Insight:**
```
Traditional Approach: Read entire files → Understand → Answer
RAG Approach: Embed knowledge → Retrieve relevant chunks → Synthesize understanding

Token Reduction: 85-95%
Comprehension Quality: Higher (focused, relevant context only)
```

**Strategic Vision: Julie + Goldfish = Complete Augmented Developer Intelligence**
- **Goldfish**: RAG on temporal developer memories (what you've worked on)
- **Julie**: RAG on spatial codebase knowledge (how the codebase works)
- **Together**: Full context for any development task

---

## The Problem: Token Waste in Developer Onboarding

### Current Inefficiency

When AI agents (or human developers) work with unfamiliar codebases:

**To Understand Architecture:**
- Read CLAUDE.md: ~8,000 tokens
- Read SEARCH_FLOW.md: ~3,000 tokens
- Read ARCHITECTURE_DEBT.md: ~2,000 tokens
- Explore related code: ~10,000 tokens
- **Total: ~23,000 tokens** just to understand core architecture

**To Find Implementation Patterns:**
- Read multiple test files: ~2,000-4,000 tokens each
- Read similar implementations: ~5,000 tokens
- Understand context: ~3,000 tokens
- **Total: ~15,000 tokens** to find how to implement something

**To Understand Design Decisions:**
- Search through documentation: ~5,000 tokens
- Read commit messages: ~2,000 tokens
- Explore related code: ~8,000 tokens
- **Total: ~15,000 tokens** to understand "why" something is designed a certain way

**Grand Total: 50,000+ tokens** for comprehensive onboarding on a medium-sized project.

### The Opportunity

With semantic retrieval:
- Architecture question: Retrieve 1 relevant section (~500 tokens) vs reading entire file (3,000 tokens) = **85% reduction**
- Pattern search: Retrieve 2-3 examples (~1,000 tokens) vs reading 5 files (10,000 tokens) = **90% reduction**
- Design decision: Direct retrieval (~300 tokens) vs exploratory reading (15,000 tokens) = **98% reduction**

**Projected onboarding cost with RAG: <3,000 tokens (94% reduction)**

---

## The Vision: Julie as RAG System

### From LSP to RAG

**Current Paradigm (LSP-Quality Tools):**
```
Capabilities:
- Symbol navigation (go to definition)
- Text search (find references)
- Static analysis (type checking)

Limitations:
- Can only answer "where" and "what"
- Cannot explain "why" or "how"
- Requires reading entire files for context
- No understanding of relationships
```

**New Paradigm (RAG-Powered Codebase Intelligence):**
```
Capabilities:
- Semantic understanding of code AND documentation
- Context-aware retrieval across multiple knowledge types
- Relationship discovery (code ↔ docs ↔ tests ↔ decisions)
- Pattern recognition and suggestion

Answers:
- "Why was this designed this way?"
- "How do I implement feature X?"
- "What are the patterns here?"
- "Show me examples of Y"
```

### RAG Architecture

```
┌─────────────────────────────────────────┐
│  Query: "How does CASCADE work?"         │
└────────────────┬────────────────────────┘
                 ↓
        ┌────────────────────────┐
        │   Query Processor       │
        │ (Intent Recognition)    │
        │  - Architecture query   │
        │  - Implementation query │
        │  - Pattern query        │
        └────────┬───────────────┘
                 ↓
    ┌────────────────────────────────┐
    │     Dual Embedding Engine      │
    │  ┌──────────────────────────┐  │
    │  │ CodeBERT (code symbols)  │  │
    │  │   384D vectors           │  │
    │  ├──────────────────────────┤  │
    │  │ BGE-Small (docs/text)    │  │
    │  │   384D vectors           │  │
    │  └──────────────────────────┘  │
    └────────┬───────────────────────┘
             ↓
    ┌──────────────────────────┐
    │   HNSW Vector Store      │
    │  (Unified Index)         │
    │  - Code embeddings       │
    │  - Doc embeddings        │
    │  - Test embeddings       │
    │  - ADR embeddings        │
    └────────┬─────────────────┘
             ↓
    ┌──────────────────────────┐
    │   Retrieval Engine       │
    │  - Semantic search       │
    │  - Diversity (MMR)       │
    │  - Cross-domain linking  │
    └────────┬─────────────────┘
             ↓
    ┌──────────────────────────┐
    │   Context Assembler      │
    │  - Deduplicate           │
    │  - Token management      │
    │  - Relevance ordering    │
    └────────┬─────────────────┘
             ↓
    ┌─────────────────────────────────┐
    │   Response (Token-Optimized)    │
    │                                  │
    │   CASCADE Architecture:          │
    │   - 2-tier system (SQLite→HNSW) │
    │   - Instant search availability │
    │   - Progressive enhancement     │
    │   - Per-workspace isolation     │
    │                                  │
    │   [Relevant section only: 500   │
    │    tokens vs 3,000 for full]    │
    └─────────────────────────────────┘
```

---

## Historical Context: Why the Explore Tool Failed

### What the Explore Tool Was

Removed in commit d281590 (2025-10-18) as "redundant with fast_search + get_symbols".

**Original Capabilities (4 modes):**
1. **Overview**: Symbol counts, file stats, language distribution
2. **Dependencies**: Relationship analysis
3. **Hotspots**: Complexity analysis (files with most symbols)
4. **Trace**: Focused relationship queries

**Implementation:**
- Intelligent SQL aggregations
- Optimized database queries
- No loading all symbols into memory
- Actually worked correctly (7,951 symbols, 620 files validated)

### Why It Failed

**The Real Problem: Not Leveraging Embeddings**

The explore tool was **aggregating database statistics**, not performing semantic understanding:

```rust
// What explore tool did:
SELECT COUNT(*) FROM symbols GROUP BY language;
SELECT * FROM symbols ORDER BY complexity DESC LIMIT 10;

// What it SHOULD have done (RAG approach):
Query: "What are the core architectural components?"
→ Semantic search across code + docs
→ Retrieve relevant symbols with context
→ Synthesize understanding
```

**Key Insight:** The tool failed because it was trying to solve the wrong problem:
- **Explore**: "How many symbols are there?" (database query)
- **RAG**: "What is this codebase about?" (semantic understanding)

No unique value proposition → overlapped with existing tools → removed.

### What We're Building Differently

**RAG System Will Succeed Because:**

1. **Clear, Unmet Need**: No existing tool does "explain this codebase" with semantic understanding
2. **Unique Capability**: Semantic search across code + docs + tests + decisions
3. **Leverages Embeddings Properly**: Actually uses HNSW semantic search (not just SQL aggregations)
4. **Existing Infrastructure**: Can reuse patterns from `find_logic` tool (Tier 4: Semantic Business Search)
5. **Better Model**: CodeBERT trained specifically on code (vs generic BGE-Small)

---

## Embedding Model Strategy

### Current State: BGE-Small-EN-V1.5

**Model Details:**
- **Name**: `BAAI/bge-small-en-v1.5` (Beijing Academy of AI)
- **Type**: BERT-based general-purpose text embedding
- **Dimensions**: 384
- **Size**: ~130MB ONNX model + ~450KB tokenizer
- **Backend**: ONNX Runtime with GPU acceleration

**Why It Was Chosen:**
- General-purpose: works for natural language + code
- Small footprint: 384 dimensions, manageable memory
- ONNX support: easy GPU acceleration (DirectML/CUDA/CoreML)
- Popular/proven: widely used in embedding tasks

**Critical Limitation: NOT Code-Specific**
- Optimized for natural language semantic similarity
- Doesn't understand: syntax, types, control flow, code structure
- Misses code-specific patterns and relationships

### POC Task: Evaluate CodeBERT

**CodeBERT Family:**
- **CodeBERT**: Microsoft's code + docs model (6 languages)
- **GraphCodeBERT**: Understands data flow and program structure
- **UniXcoder**: Cross-language code understanding
- **StarEncoder**: Specifically for code search (BigCode)

**Evaluation Criteria:**
1. **ONNX Availability**: Can we export/obtain ONNX model?
2. **Retrieval Quality**: Code pattern matching accuracy
3. **Cross-Language**: Symbol matching across languages
4. **Performance**: Inference latency (GPU/CPU)
5. **Memory**: Model size and runtime footprint

### Recommended Architecture: Dual-Model System

```rust
struct DualEmbeddingEngine {
    code_model: CodeBERT,      // For symbols, functions, code patterns
    text_model: BGE,           // For docs, comments, ADRs
    router: QueryRouter,       // Decides which model to use
}

impl DualEmbeddingEngine {
    async fn embed(&self, content: &str, content_type: ContentType) -> Vec<f32> {
        match content_type {
            ContentType::Code => self.code_model.embed(content).await,
            ContentType::Documentation => self.text_model.embed(content).await,
            ContentType::Comment => self.text_model.embed(content).await,
            ContentType::Mixed => {
                // Hybrid: embed with both, concatenate/merge vectors
                let code_vec = self.code_model.embed(content).await;
                let text_vec = self.text_model.embed(content).await;
                merge_vectors(code_vec, text_vec)
            }
        }
    }
}
```

**Benefits:**
- **Best of both worlds**: Code understanding + natural language semantics
- **Flexible**: Route queries to appropriate model
- **Backward compatible**: Can start with BGE, add CodeBERT later
- **Quality improvement**: Each domain gets specialized model

---

## Database Schema Design

### UPDATE (2025-11-07): Architecture Simplification

**Critical Finding:** SQLite FTS5 virtual tables are incompatible with foreign key constraints and triggers, causing "unsafe use of virtual table" errors. The `knowledge_embeddings` approach is unworkable.

**New Decision: Use Existing Symbols Table** ✅

**Rationale:**
1. **Proven Infrastructure**: Symbols table already handles 9000+ symbols successfully
2. **Working FTS5 Search**: Already implemented and tested
3. **No SQLite Complications**: No virtual table + foreign key issues
4. **Already Implemented**: Markdown extractor stores docs as symbols (504 working)
5. **Simpler is Better**: Leverage what works instead of fighting SQLite limitations

**Implementation:**
- Documentation stored as symbols with special `kind` values (e.g., "heading", "section")
- Add `content_type` field to distinguish documentation from code
- Use existing FTS5 index on symbols table
- Remove `knowledge_embeddings` complexity entirely

### Original Design (Abandoned Due to SQLite Limitations)

**Decision: Unified Schema** ~~✅~~ ❌

**Rationale (still valid conceptually):**
1. **Cross-domain semantic search**: "How do I implement auth?" returns:
   - Documentation sections about authentication
   - Code implementations (existing auth functions)
   - Test examples (auth test cases)
   - Architecture decisions (why auth designed this way)

2. **Relationship discovery**: Single HNSW index enables:
   - Documentation → Code linking
   - Design decision → Implementation
   - Test → Function being tested
   - Pattern → Examples

3. **Simpler maintenance**: One index, one embedding pipeline, one search API

### Proposed Schema (ABANDONED - SQLite Limitations)

```sql
-- This approach failed due to FTS5 + foreign key incompatibility
-- Keeping for reference of what was attempted

-- Unified knowledge embeddings table
CREATE TABLE knowledge_embeddings (
    id INTEGER PRIMARY KEY AUTOINCREMENT,

    -- Entity identification
    entity_type TEXT NOT NULL,  -- 'code_symbol', 'doc_section', 'test_case', 'adr', 'comment'
    entity_id TEXT NOT NULL,     -- Reference to source entity (symbol_id, doc_section_id, etc.)

    -- Source information
    source_file TEXT NOT NULL,   -- Relative Unix-style path
    section_title TEXT,          -- For docs: "CASCADE Architecture"
    language TEXT,               -- For code: "rust", "typescript", etc.

    -- Content
    content TEXT NOT NULL,       -- Original text (for display/context)
    content_hash TEXT NOT NULL,  -- Blake3 hash for deduplication

    -- Embedding
    embedding BLOB NOT NULL,     -- 384-dimensional f32 vector (1536 bytes)
    model_name TEXT NOT NULL,    -- "bge-small" or "codebert"

    -- Metadata (JSON for flexibility)
    metadata TEXT,               -- JSON: {"tags": [...], "importance": "high", ...}

    -- Timestamps
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,

    -- Indexes
    UNIQUE(entity_type, entity_id, model_name)
);

CREATE INDEX idx_entity_type ON knowledge_embeddings(entity_type);
CREATE INDEX idx_source_file ON knowledge_embeddings(source_file);
CREATE INDEX idx_model_name ON knowledge_embeddings(model_name);
CREATE INDEX idx_content_hash ON knowledge_embeddings(content_hash);

-- Relationship table (for explicit cross-references)
CREATE TABLE knowledge_relationships (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    from_id INTEGER NOT NULL,    -- knowledge_embeddings.id
    to_id INTEGER NOT NULL,      -- knowledge_embeddings.id
    relationship_type TEXT NOT NULL,  -- 'implements', 'documents', 'tests', 'decides'
    confidence REAL DEFAULT 1.0, -- Relationship strength

    FOREIGN KEY (from_id) REFERENCES knowledge_embeddings(id),
    FOREIGN KEY (to_id) REFERENCES knowledge_embeddings(id)
);

CREATE INDEX idx_relationships_from ON knowledge_relationships(from_id);
CREATE INDEX idx_relationships_to ON knowledge_relationships(to_id);
CREATE INDEX idx_relationship_type ON knowledge_relationships(relationship_type);
```

### Revised Migration Strategy (2025-11-07)

**Phase 1: Enhance Existing Infrastructure**
1. Add `content_type` field to `symbols` table (nullable, backward compatible)
2. Keep using existing FTS5 index
3. Documentation already stored as symbols via markdown extractor
4. No new tables needed

**Phase 2: Improve Documentation Extraction**
1. Enhance markdown extractor to include section content (not just headings)
2. Store richer embeddings for documentation symbols
3. Use symbol `kind` field to identify documentation (e.g., "heading", "section")

**Phase 3: Semantic Search Enhancement**
1. Update embedding generation to handle documentation symbols better
2. Improve search ranking for documentation vs code
3. Add cross-reference capabilities using existing infrastructure

---

## Phase 1: Proof of Concept (1-2 weeks)

### Goals

**Validate Core Assumptions:**
1. Semantic retrieval reduces tokens by 85%+
2. Retrieval quality is high (>80% precision)
3. CodeBERT improves code understanding vs BGE
4. Implementation is tractable with existing infrastructure

**Success Metrics:**
- ✅ Documentation retrieval: <100ms latency, >80% precision
- ✅ Token reduction demonstrated: >85% savings
- ✅ CodeBERT evaluation complete: go/no-go decision
- ✅ Pattern search improvement: measurable quality gain
- ✅ Architecture questions: answered correctly with minimal context

### Task 1: Model Evaluation (2-3 days)

**Research Phase:**
1. Find CodeBERT ONNX models (HuggingFace, ONNX Model Zoo)
2. Evaluate alternatives: GraphCodeBERT, StarEncoder, UniXcoder
3. Test ONNX runtime compatibility (DirectML/CUDA)
4. Measure model sizes and memory requirements

**Comparison Framework:**
```rust
struct ModelBenchmark {
    model_name: String,
    test_queries: Vec<CodeQuery>,

    // Metrics
    retrieval_accuracy: f32,      // % of relevant results in top-k
    inference_latency_ms: f32,    // GPU vs CPU
    memory_usage_mb: usize,       // Runtime footprint
    model_size_mb: usize,         // Disk size
}

// Test queries for comparison
let test_queries = vec![
    CodeQuery {
        query: "error handling patterns",
        expected_symbols: ["handle_error", "propagate_error", "try_operation"],
    },
    CodeQuery {
        query: "async database operations",
        expected_symbols: ["async_query", "connect_pool", "execute_batch"],
    },
    // ... more test cases
];
```

**Deliverable:** Model comparison report with recommendation (BGE-Small vs CodeBERT vs Dual)

### Task 2: Documentation Embeddings (3-4 days)

**Markdown Parser:**
```rust
pub struct DocumentChunk {
    pub source_file: String,
    pub section_title: String,
    pub content: String,
    pub depth: usize,          // Header level (## = 2, ### = 3)
    pub tags: Vec<String>,     // Extracted from headers
    pub code_blocks: Vec<CodeBlock>,
    pub token_count: usize,
}

pub struct MarkdownParser {
    max_chunk_tokens: usize,   // Default: 512 (BERT limit)

    pub fn parse(&self, markdown: &str) -> Vec<DocumentChunk> {
        // 1. Split by headers (## and ###)
        // 2. Extract code blocks (preserve language tags)
        // 3. Chunk long sections (overlap for context)
        // 4. Extract metadata (tags from headers)
    }
}
```

**Indexing Pipeline:**
```rust
pub async fn index_documentation(&self) -> Result<()> {
    let docs = vec![
        "CLAUDE.md",
        "docs/SEARCH_FLOW.md",
        "docs/ARCHITECTURE.md",
        "TODO.md",
    ];

    for doc_path in docs {
        // 1. Parse markdown into chunks
        let chunks = self.parser.parse(&content)?;

        // 2. Generate embeddings
        for chunk in chunks {
            let embedding = self.embedding_engine
                .embed_text(&chunk.content)
                .await?;

            // 3. Store in database
            self.db.insert_knowledge_embedding(KnowledgeEmbedding {
                entity_type: "doc_section",
                entity_id: format!("{}#{}", doc_path, chunk.section_title),
                source_file: doc_path,
                section_title: chunk.section_title,
                content: chunk.content,
                embedding,
                model_name: "bge-small",
                metadata: json!({
                    "tags": chunk.tags,
                    "depth": chunk.depth,
                }),
            })?;
        }
    }

    Ok(())
}
```

**Deliverable:** All markdown docs indexed and searchable

### Task 3: Semantic Doc Search Tool (2-3 days)

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct SemanticDocSearchTool {
    pub query: String,
    pub limit: Option<usize>,           // Default: 5
    pub min_similarity: Option<f32>,    // Default: 0.7
    pub entity_types: Option<Vec<String>>,  // Filter: ["doc_section", "adr"]
}

impl McpTool for SemanticDocSearchTool {
    async fn execute(&self, handler: &JulieServerHandler) -> Result<ToolResponse> {
        // 1. Generate query embedding
        let query_embedding = handler
            .get_embedding_engine()
            .await?
            .embed_text(&self.query)
            .await?;

        // 2. Search HNSW index
        let vector_store = handler.get_vector_store().await?;
        let neighbors = vector_store.search(
            &query_embedding,
            self.limit.unwrap_or(5) * 2,  // Over-fetch for filtering
        )?;

        // 3. Load from database
        let db = handler.get_database().await?;
        let mut results = vec![];

        for (id, similarity) in neighbors {
            if similarity < self.min_similarity.unwrap_or(0.7) {
                continue;
            }

            let embedding = db.get_knowledge_embedding_by_id(id)?;

            // Filter by entity type if specified
            if let Some(ref types) = self.entity_types {
                if !types.contains(&embedding.entity_type) {
                    continue;
                }
            }

            results.push(SearchResult {
                source_file: embedding.source_file,
                section_title: embedding.section_title,
                content: embedding.content,
                similarity,
                metadata: embedding.metadata,
            });
        }

        // 4. Apply diversity (MMR - Maximal Marginal Relevance)
        let diverse_results = apply_mmr(results, self.limit.unwrap_or(5))?;

        // 5. Format response
        Ok(ToolResponse::success(json!({
            "results": diverse_results,
            "query": self.query,
            "total_found": diverse_results.len(),
        })))
    }
}
```

**Deliverable:** Working `semantic_doc_search` tool

### Task 4: Validation & Metrics (2-3 days)

**Test Queries:**

```rust
let test_cases = vec![
    TestCase {
        query: "How does CASCADE architecture work?",
        expected_file: "docs/SEARCH_FLOW.md",
        expected_section: "CASCADE Architecture",
        expected_keywords: vec!["SQLite", "HNSW", "2-tier", "progressive enhancement"],
    },
    TestCase {
        query: "Why was Tantivy removed?",
        expected_file: "CLAUDE.md",
        expected_section: "CASCADE Architecture",
        expected_keywords: vec!["Arc<RwLock>", "deadlock", "simplification"],
    },
    TestCase {
        query: "What is SOURCE/CONTROL methodology?",
        expected_file: "CLAUDE.md",
        expected_section: "Testing Standards",
        expected_keywords: vec!["original files", "expected results", "diff-match-patch"],
    },
];
```

**Metrics:**
1. **Retrieval Precision**: % of results that are relevant
2. **Retrieval Recall**: % of relevant docs found
3. **Token Reduction**: Tokens retrieved vs full file read
4. **Latency**: Query → results time
5. **Answer Quality**: Can query be answered from retrieved context?

**Deliverable:** Metrics report validating POC success/failure

---

## Phase 2: Core RAG Implementation (2-3 weeks)

### Architecture Decision Records (ADRs)

**Template:**
```markdown
# ADR-001: Remove Tantivy Search Engine

**Status:** Accepted
**Date:** 2025-10-12
**Decision Makers:** Development team
**Tags:** architecture, search, performance

## Context

Julie originally used 3-tier CASCADE: SQLite FTS5 → Tantivy → HNSW Semantic.
Tantivy was causing Arc<RwLock> deadlocks and 5-10s commit times.

## Decision

Remove Tantivy entirely, simplify to 2-tier: SQLite FTS5 → HNSW Semantic.

## Rationale

1. **Deadlock Elimination**: Arc<RwLock> contention was causing crashes
2. **Performance**: SQLite FTS5 alone is <5ms, sufficient for text search
3. **Simplicity**: Fewer moving parts, easier maintenance
4. **Proven**: SQLite FTS5 has decades of production use

## Alternatives Considered

1. **Fix Tantivy deadlocks**: Too complex, Arc<RwLock> fundamentally problematic
2. **Use different Rust search engine**: Would face similar concurrency issues
3. **Keep Tantivy, disable writes**: Defeats purpose of incremental indexing

## Consequences

**Positive:**
- 30-60x faster startup (no Tantivy blocking)
- Zero deadlocks
- Simpler architecture
- Per-workspace isolation trivial

**Negative:**
- Slightly less sophisticated text search (no BM25F, just BM25)
- Lost some Tantivy-specific features

## Implementation

- Commit: 2a37142
- Removed: src/search/tantivy/
- Updated: CASCADE to 2-tier in SEARCH_FLOW.md
```

**ADR Extraction Process:**
1. Identify 10-15 key decisions in CLAUDE.md, commit messages
2. Create ADR for each with template above
3. Store in `.julie/decisions/`
4. Embed each ADR
5. Index in `knowledge_embeddings` table

### Enhanced Code Embeddings

**Current (Minimal):**
```rust
format!("{} {} {} {}",
    symbol.name,
    symbol.kind,
    symbol.signature,
    symbol.doc_comment,
)
```

**Enhanced (With Bodies):**
```rust
pub fn build_enhanced_embedding_text(&self, symbol: &Symbol) -> String {
    let mut parts = vec![
        symbol.name.clone(),
        symbol.kind.clone(),
    ];

    if let Some(sig) = &symbol.signature {
        parts.push(sig.clone());
    }

    if let Some(doc) = &symbol.doc_comment {
        parts.push(doc.clone());
    }

    // NEW: Include function body (truncated to 256 tokens)
    if let Some(body) = &symbol.code_body {
        let truncated = truncate_to_tokens(body, 256);
        parts.push(truncated);
    }

    parts.join(" ")
}
```

**Benefits:**
- Pattern matching: "Show error handling" finds actual error handling code
- Implementation search: "async database operations" finds await patterns
- Testing: "SOURCE/CONTROL tests" finds test methodology examples

### Cross-Reference System

```rust
pub struct RelationshipBuilder {
    db: Database,
    embedding_engine: EmbeddingEngine,

    pub async fn build_relationships(&self) -> Result<()> {
        // 1. Documentation → Code
        self.link_docs_to_code().await?;

        // 2. Tests → Functions
        self.link_tests_to_functions().await?;

        // 3. ADRs → Implementations
        self.link_adrs_to_code().await?;

        // 4. Comments → Code
        self.link_comments_to_functions().await?;

        Ok(())
    }

    async fn link_docs_to_code(&self) -> Result<()> {
        // Find doc sections that describe code
        let doc_chunks = self.db.get_embeddings_by_type("doc_section")?;

        for doc in doc_chunks {
            // Search for semantically similar code symbols
            let similar_code = self.semantic_search(
                &doc.embedding,
                entity_type: "code_symbol",
                limit: 5,
                min_similarity: 0.75,
            ).await?;

            for code in similar_code {
                self.db.insert_relationship(Relationship {
                    from_id: doc.id,
                    to_id: code.id,
                    relationship_type: "documents",
                    confidence: code.similarity,
                })?;
            }
        }

        Ok(())
    }
}
```

---

## Phase 3: Advanced RAG Features (3-4 weeks)

### Generative Capabilities

**Tool: explain_code**
```rust
pub struct ExplainCodeTool {
    pub symbol_or_file: String,
    pub depth: ExplanationDepth,  // Summary, Detailed, Comprehensive
}

impl McpTool for ExplainCodeTool {
    async fn execute(&self) -> Result<ToolResponse> {
        // 1. Retrieve code
        let code = self.get_code(&self.symbol_or_file)?;

        // 2. Retrieve related context
        let context = self.retrieve_context(&code).await?;

        // 3. Assemble explanation context
        let explanation_context = format!(
            "Code:\n{}\n\nDocumentation:\n{}\n\nTests:\n{}\n\nDesign Decisions:\n{}",
            code,
            context.docs,
            context.tests,
            context.adrs,
        );

        // 4. Return structured explanation
        Ok(ToolResponse::success(json!({
            "code": code,
            "related_docs": context.docs,
            "related_tests": context.tests,
            "design_decisions": context.adrs,
            "token_count": estimate_tokens(&explanation_context),
        })))
    }
}
```

**Tool: suggest_implementation**
```rust
pub struct SuggestImplementationTool {
    pub task_description: String,
    pub language: Option<String>,
}

impl McpTool for SuggestImplementationTool {
    async fn execute(&self) -> Result<ToolResponse> {
        // 1. Find similar implementations
        let similar = self.semantic_search(&self.task_description).await?;

        // 2. Extract patterns
        let patterns = self.extract_patterns(&similar)?;

        // 3. Return suggestions
        Ok(ToolResponse::success(json!({
            "suggestions": patterns,
            "examples": similar,
            "reasoning": "Based on these existing implementations...",
        })))
    }
}
```

### Goldfish Integration

**Vision: Unified Context System**

```rust
pub async fn unified_context_query(
    query: &str,
    goldfish: &GoldfishClient,
    julie: &JulieClient,
) -> Result<UnifiedContext> {
    // Query both systems in parallel
    let (goldfish_context, julie_context) = tokio::join!(
        goldfish.recall_work(query),
        julie.semantic_search(query),
    );

    // Merge and rank results
    UnifiedContext {
        temporal: goldfish_context?,  // What you've worked on
        spatial: julie_context?,      // How the codebase works
        synthesized: synthesize_context(goldfish_context, julie_context)?,
    }
}
```

**Example Query:**
```
"What was that authentication approach I tried last week
 that's similar to how the payment service does it?"

Goldfish retrieves:
- Your auth work from last week
- Checkpoint: "Implemented JWT validation with refresh tokens"
- Code changes: src/auth/jwt.rs

Julie retrieves:
- Payment service auth pattern
- Code: src/payment/auth.rs
- ADR: "Why JWT over OAuth for payments"

Synthesis:
- Compare both approaches
- Highlight similarities/differences
- Suggest best approach
```

---

## Implementation Timeline

### POC Phase (Weeks 1-2)

**Week 1: Model Evaluation**
- Day 1-2: Research CodeBERT ONNX models
- Day 3: Build comparison framework
- Day 4: Run benchmarks
- Day 5: Document results, make go/no-go decision

**Week 2: Documentation Embeddings**
- Day 1: Build markdown parser
- Day 2: Create knowledge_embeddings schema
- Day 3: Index core documentation
- Day 4: Implement semantic_doc_search tool
- Day 5: Validation and metrics

**Decision Point:** POC successful? Proceed to full implementation or iterate.

### Core Implementation (Weeks 3-5)

**Week 3: ADRs + Enhanced Code Embeddings**
- Extract and document 10+ ADRs
- Enhance code embeddings with bodies
- Build relationship system foundation

**Week 4: Cross-Reference System**
- Link docs ↔ code
- Link tests ↔ functions
- Link ADRs ↔ implementations

**Week 5: Tool Integration**
- Update existing tools to use RAG
- Create new explanation tools
- Performance optimization

### Advanced Features (Weeks 6-8)

**Week 6: Generative Tools**
- explain_code tool
- suggest_implementation tool
- generate_test tool

**Week 7: Multi-Modal Retrieval**
- Unified context assembly
- Diversity algorithms (MMR)
- Token optimization

**Week 8: Goldfish Integration**
- Cross-system queries
- Unified context protocol
- End-to-end testing

---

## Success Criteria

### POC Success (Go/No-Go Decision)

✅ **MUST ACHIEVE:**
1. Documentation retrieval: <100ms, >80% precision
2. Token reduction: >85% vs full file reads
3. CodeBERT evaluation: clear winner or dual-model justified
4. Architecture questions: answered correctly with minimal context

❌ **FAIL IF:**
1. Retrieval quality <70% precision
2. Token reduction <50%
3. Latency >500ms
4. Answers incomplete or incorrect

### Full Implementation Success

✅ **MUST ACHIEVE:**
1. Onboarding context: <3,000 tokens (90% reduction from 30,000)
2. All markdown docs embedded and searchable
3. 10+ ADRs extracted and indexed
4. Cross-reference graph operational
5. Integration with existing tools seamless
6. No regressions in current functionality

📊 **METRICS:**
- Search quality: >85% precision, >90% recall
- Performance: <100ms for doc search, <50ms for code search
- Coverage: 100% of markdown files, 100% of code symbols
- Relationships: >1000 doc↔code links discovered

---

## Technical Risks & Mitigation

### Risk 1: CodeBERT ONNX Unavailable

**Probability:** Medium
**Impact:** High

**Mitigation:**
1. Start POC with BGE-Small (validates approach)
2. Research alternatives: StarEncoder, UniXcoder
3. Consider manual ONNX export of CodeBERT
4. Fallback: Dual-write to both models, compare quality

### Risk 2: Retrieval Quality Insufficient

**Probability:** Low
**Impact:** High

**Mitigation:**
1. Hybrid search: semantic + keyword (fallback)
2. Fine-tune embeddings on Julie codebase
3. Use LLM re-ranking for final results
4. Implement user feedback loop for quality improvement

### Risk 3: Context Assembly Complexity

**Probability:** Medium
**Impact:** Medium

**Mitigation:**
1. Start simple: top-k retrieval only
2. Iterate: add diversity, deduplication, ranking
3. Use existing token optimization utilities
4. Benchmark against token budgets continuously

### Risk 4: Performance Degradation

**Probability:** Low
**Impact:** High

**Mitigation:**
1. Leverage existing HNSW infrastructure (proven <50ms)
2. Background indexing for documentation (non-blocking)
3. Cache query embeddings
4. Batch operations where possible

---

## Open Questions

### Model Selection

**Q1:** CodeBERT vs GraphCodeBERT vs UniXcoder?
- **Research needed:** ONNX availability, quality benchmarks, memory footprint

**Q2:** Dual-model vs single-model?
- **Decision criteria:** Quality improvement vs complexity cost

### Schema Design

**Q3:** Unified index vs separate indexes per entity type?
- **Trade-off:** Search simplicity vs filtering performance

**Q4:** Relationship table vs semantic-only linking?
- **Decision:** Explicit relationships for high-confidence, semantic for discovery

### Tool Design

**Q5:** Dedicated RAG tools vs enhance existing tools?
- **Option A:** New `explain_codebase`, `onboard_task` tools
- **Option B:** Add RAG mode to `fast_search`, `get_symbols`

**Q6:** Context assembly in tool vs in client?
- **Trade-off:** Server-side logic vs client flexibility

---

## Appendix: Existing Infrastructure

### What We Already Have (Working)

1. **HNSW Vector Store** (`src/embeddings/vector_store.rs`)
   - Fast nearest-neighbor search (<50ms)
   - Disk persistence
   - Integrated with embedding engine

2. **GPU-Accelerated Embeddings** (`src/embeddings/mod.rs`)
   - DirectML (Windows), CUDA (Linux), CPU-optimized (macOS)
   - Batch processing (10-100x faster)
   - `embed_text()` ready for RAG queries

3. **Semantic Search Pattern** (`find_logic` tool)
   - Query → embedding → HNSW search → ranking
   - Confidence scoring
   - Already proven in production

4. **Cross-Language Semantic Matching** (`cross_language.rs`)
   - Semantic neighbor search
   - Similarity scoring
   - Cross-boundary discovery

5. **Token Optimization** (`src/utils/token_estimation.rs`)
   - Fast token estimation (<1ms)
   - Truncation utilities
   - Budget management

### What We Need to Build

1. **Code-Specific Embedding Model**
   - CodeBERT/GraphCodeBERT ONNX
   - Dual-model architecture
   - Model router

2. **Knowledge Schema**
   - `knowledge_embeddings` table
   - `knowledge_relationships` table
   - Migration from current schema

3. **Markdown Parser**
   - Section extraction
   - Chunking strategy
   - Metadata extraction

4. **RAG Tools**
   - `semantic_doc_search`
   - `explain_code`
   - `suggest_implementation`

5. **Context Assembly**
   - Retrieval ranking
   - Diversity algorithms (MMR)
   - Token management

---

## References

### Internal Documentation
- [SEARCH_FLOW.md](./SEARCH_FLOW.md) - CASCADE architecture details
- [ARCHITECTURE.md](./ARCHITECTURE.md) - Token optimization strategies
- [CLAUDE.md](../CLAUDE.md) - Development guidelines and decisions

### External Resources
- [CodeBERT Paper](https://arxiv.org/abs/2002.08155) - Microsoft Research
- [GraphCodeBERT](https://arxiv.org/abs/2009.08366) - Data flow understanding
- [RAG Paper](https://arxiv.org/abs/2005.11401) - Retrieval-Augmented Generation
- [BGE Model](https://huggingface.co/BAAI/bge-small-en-v1.5) - Current embedding model

### Tools & Libraries
- ONNX Runtime - GPU acceleration
- HNSW (Hierarchical Navigable Small World) - Vector search
- SQLite FTS5 - Full-text search
- Tree-sitter - Code parsing

---

**Document Status:** Living document - updated as implementation progresses
**Last Review:** 2025-11-05
**Next Review:** After POC completion
