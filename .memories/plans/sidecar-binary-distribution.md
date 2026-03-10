---
id: sidecar-binary-distribution
title: Sidecar Binary Distribution
status: completed
created: 2026-03-04T21:45:29.810Z
updated: 2026-03-08T22:14:07.433Z
tags:
- sidecar
- distribution
- embeddings
---

# Sidecar Binary Distribution

## Goal
Embed Python sidecar source into Julie binary so distributed binaries and `cargo install` work without a source checkout.

## Approach
Use `include_dir` crate to embed `python/embeddings_sidecar/` at compile time. Extract to cache dir on first use. 4-level fallback chain in `sidecar_root_path()`.

## Tasks
1. Add `include_dir` dependency
2. Extraction function + tests (4 tests in `src/tests/core/sidecar_embedding_tests.rs`)
3. Update `sidecar_root_path()` with fallback chain (env → adjacent → source → extract)
4. Fast tier regression check
5. Manual verification with isolated binary

## Key Files
- `src/embeddings/sidecar_supervisor.rs` — main changes
- `src/embeddings/mod.rs` — export updates
- `src/tests/core/sidecar_embedding_tests.rs` — new tests
- `Cargo.toml` — new dependency

## Design Doc
`docs/plans/2026-03-04-sidecar-binary-distribution-design.md`

## Implementation Plan
`docs/plans/2026-03-04-sidecar-binary-distribution-impl.md`
