# Embedding Status Dashboard (#15)

**Date:** 2026-03-09
**Status:** Approved

## Summary

Keep ORT as sidecar fallback (no code changes needed — already works). Add embedding health visibility to the dashboard so users can see backend status, acceleration, degraded reasons, and trigger initialization on demand.

## Decision: ORT Dependency

**Keep ORT as fallback.** The `embeddings-ort` feature flag (default on) compiles ORT support. The `Auto` resolution path: Sidecar (Python/GPU) preferred → ORT (CPU) fallback. Architecture is sound, tests cover fallback paths.

## Backend Changes

### Extend `GET /api/dashboard/stats`

Add `embeddings: Vec<EmbeddingProjectStatus>` to `DashboardStats`:

```rust
pub struct EmbeddingProjectStatus {
    pub project: String,
    pub workspace_id: String,
    pub backend: Option<String>,      // "sidecar", "ort", or None if not initialized
    pub accelerated: Option<bool>,
    pub degraded_reason: Option<String>,
    pub embedding_count: i64,         // from SQLite symbol_vectors COUNT
    pub initialized: bool,
}
```

For each loaded workspace:
- If `embedding_runtime_status` exists → populate backend/accelerated/degraded_reason, `initialized = true`
- If not → `backend = None`, `initialized = false`
- Always query `embedding_count` from SQLite regardless of runtime status

### New `POST /api/embeddings/check`

Triggers `initialize_embedding_provider()` on all loaded workspaces that don't have a provider yet. Returns the updated `Vec<EmbeddingProjectStatus>`.

**Files:** `src/api/dashboard.rs`, `src/api/mod.rs`

## Frontend Changes

### New Embeddings card on Dashboard

Position: after Backends card in row 2. Structure:

- Header: brain icon + "Embeddings" label
- Per-project rows:
  - Colored dot: green (accelerated), yellow (degraded/CPU-only), gray (not initialized)
  - Project name
  - Backend badge or "not initialized"
  - Embedding count
- Degraded reason shown as subtitle text
- "Check Status" button at bottom → calls `POST /api/embeddings/check`, refreshes card

**File:** `ui/src/views/Dashboard.vue`

## Files to Modify

- `src/api/dashboard.rs` — add `EmbeddingProjectStatus` struct, populate in `stats()`, add `check_embeddings()` handler
- `src/api/mod.rs` — register `/embeddings/check` route, add schemas to OpenAPI
- `ui/src/views/Dashboard.vue` — add Embeddings card with check button

## Acceptance Criteria

- [ ] Dashboard stats API returns per-project embedding status with counts
- [ ] New Embeddings card on dashboard shows per-project backend, acceleration, counts
- [ ] "Check Status" button triggers initialization and refreshes the card
- [ ] Degraded reasons shown with visual indicator
- [ ] Projects with no runtime status but existing embeddings show count with "not initialized" note
- [ ] ORT fallback verified via existing test suite
