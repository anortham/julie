---
id: fix-embedding-performance-incremental-pipeline-mps
title: "Fix Embedding Performance: Incremental Pipeline + MPS"
status: completed
created: 2026-02-27T23:46:08.451Z
updated: 2026-02-28T23:07:24.355Z
tags:
  - embeddings
  - performance
  - mps
  - incremental
---

# Fix Embedding Performance: Incremental Pipeline + MPS Acceleration

## Two fixes:
1. **Incremental embedding** — skip symbols that already have vectors (19,464 → 0-50 per session)
2. **Disable CoreML, enable MPS/Metal** — CoreML batch=1 short-circuits MPS; 1-line fix in `default_coreml_model_id_for_platform`

## Key files:
- `src/database/vectors.rs` — add `get_embedded_symbol_ids()`
- `src/embeddings/pipeline.rs` — filter out already-embedded in `run_embedding_pipeline()`
- `src/embeddings/candle_provider.rs` — disable CoreML default (line 546)
- Tests in `src/tests/core/vector_storage.rs` and `src/tests/integration/embedding_pipeline.rs`

