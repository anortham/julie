---
id: fix-semantic-search-pollution-language-filter-chil
title: "Fix Semantic Search Pollution: Language Filter + Child Enrichment"
status: completed
created: 2026-02-28T23:52:21.067Z
updated: 2026-03-01T00:48:46.073Z
tags:
  - semantic-search
  - embeddings
  - quality
---

# Fix Semantic Search Pollution: Language Filter + Child Enrichment

## Steps
1. Add language filter to embedding metadata (TDD)
2. Enrich class/interface embeddings with child method names (TDD)
3. Purge existing non-code embeddings from vector store
4. Downgrade diagnostic logging to debug level

## Key Files
- `src/embeddings/metadata.rs` — Language filter + child enrichment
- `src/database/vectors.rs` — Purge function
- `src/embeddings/pipeline.rs` — Call purge before embedding
- `src/search/hybrid.rs` — Downgrade logging
- `src/tests/core/embedding_metadata.rs` — Tests
- `src/tests/core/vector_storage.rs` — Purge test
