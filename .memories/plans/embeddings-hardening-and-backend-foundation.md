---
id: embeddings-hardening-and-backend-foundation
title: Embeddings Hardening and Backend Foundation
status: completed
created: 2026-02-27T14:51:14.682Z
updated: 2026-03-01T02:01:05.837Z
tags:
  - embeddings
  - semantic-search
  - workspace
  - backend-pluggability
  - candle
  - ort
---

# Embeddings Hardening and Backend Foundation

## Objective
Fix correctness and reliability gaps in semantic/embedding search for both primary and reference workspaces, then establish backend-pluggable architecture for future Candle support.

## Priority Order
1. Semantic filter correctness in hybrid search
2. Primary index/refresh embedding trigger parity
3. Orphan cleanup safety for primary index
4. Incremental stale embedding cleanup on modify/create
5. Vector safety guards (count mismatch + malformed blobs)
6. Provider factory/config seam (default ORT behavior preserved)
7. Feature-gated backend wiring in Cargo
8. Targeted + fast-tier verification

## Deliverables
- Correctly filtered semantic hybrid results
- Consistent embedding scheduling for primary + reference workflows
- Safe orphan cleanup behavior
- No stale vectors on incremental updates
- Defensive vector read/write validation
- Backend selection abstraction ready for Candle
- Build feature gating for backend isolation

## Plan Document
See `docs/plans/2026-02-27-embeddings-hardening-and-backend-foundation.md` for step-by-step TDD tasks and commands.
