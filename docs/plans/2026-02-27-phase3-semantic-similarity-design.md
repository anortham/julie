# Phase 3: Semantic Similarity in deep_dive — Design Document

> **Date:** 2026-02-27
> **Status:** Approved
> **Scope:** Add "Semantically Similar" section to `deep_dive` at `full` depth using stored embeddings from Phase 1

## Problem Statement

Julie has semantic embeddings for ~20K+ symbols (Phase 1) and hybrid search in `get_context` and `fast_search` (Phase 2), but there's no way to ask: "given THIS specific symbol, what other symbols in the codebase are conceptually similar?"

This matters for:
- **Duplicate discovery:** Finding `verify_credentials` when you're looking at `authenticate_user`
- **Related concept navigation:** From `UserService` discovering `UserRepository`, `UserController`
- **Refactoring awareness:** Understanding that renaming a concept affects semantically related (not just structurally linked) code

## Design Decisions

| Decision | Choice | Alternatives Rejected | Reasoning |
|----------|--------|-----------------------|-----------|
| Tool surface | `deep_dive` at `full` depth | `fast_refs` with `reference_kind="semantic"`, new `find_similar` tool, `fast_search` param | `deep_dive` is the investigation tool — semantic neighbors belong in the same context as callers/callees/children. Doesn't pollute `fast_refs` (safety/impact tool). No new tool to learn. |
| Scope | Within-workspace only | Cross-workspace | Primary use case is same-codebase concept discovery. Cross-workspace can be added later. |
| Vector source | Stored embedding lookup | Query-time `embed_query()` | No model inference at query time. Uses vectors already computed by Phase 1 pipeline. `get_embedding()` returning `None` handles the "not yet embedded" edge case gracefully. |
| Depth level | `full` only | `context` + `full` | Progressive disclosure: `overview` = stats, `context` = code + immediate refs, `full` = everything including semantic. Keeps `context` fast and focused. |
| Clustering | Not in scope | DBSCAN on `semantic_group` column | KNN point-to-point similarity is simpler and more immediately useful. `semantic_group` column stays unused for future work. |

## Architecture

### Data Flow

```
deep_dive(symbol="authenticate_user", depth="full")
  │
  ├─ find_symbol(db, "authenticate_user")
  │    → Symbol { id: "abc123", ... }
  │
  ├─ build_symbol_context(db, symbol, "full", ...)
  │    ├─ [existing] callers, callees, children, implementations, test_refs
  │    │
  │    └─ [NEW] semantic similarity:
  │         ├─ db.get_embedding("abc123") → Some([0.12, -0.34, ...])
  │         ├─ db.knn_search(&vector, 6) → [(id, distance), ...]
  │         ├─ filter out self (symbol_id == "abc123")
  │         ├─ take top 5
  │         └─ db.get_symbols_by_ids(&ids) → Vec<Symbol>
  │
  ├─ format_symbol_context(ctx, "full")
  │    ├─ [existing] header, body, kind-specific sections
  │    └─ [NEW] format_similar_section(similar)
  │
  └─ Output string
```

### Component Changes

#### 1. `SymbolDatabase` — New method (`src/database/vectors.rs`)

```rust
/// Retrieve a symbol's stored embedding vector.
/// Returns None if the symbol hasn't been embedded yet.
pub fn get_embedding(&self, symbol_id: &str) -> Result<Option<Vec<f32>>>
```

Simple SELECT from `symbol_vectors` virtual table, deserialize blob to `Vec<f32>`.

#### 2. `SymbolContext` — New field (`src/tools/deep_dive/data.rs`)

```rust
/// Entry for a semantically similar symbol
#[derive(Debug)]
pub struct SimilarEntry {
    pub symbol: Symbol,
    pub score: f32,  // 0.0..1.0 (1.0 - cosine_distance), higher = more similar
}

pub struct SymbolContext {
    // ... existing fields unchanged ...
    /// Semantically similar symbols (populated at "full" depth only)
    pub similar: Vec<SimilarEntry>,
}
```

#### 3. `build_symbol_context` — Similarity lookup (`src/tools/deep_dive/data.rs`)

At `full` depth only:
1. `db.get_embedding(&symbol.id)` → `Option<Vec<f32>>`
2. If `Some`: `db.knn_search(&vec, 6)` → filter self → take 5 → `db.get_symbols_by_ids()`
3. If `None`: `similar = vec![]`

At `overview` and `context` depths: `similar = vec![]` (skip entirely).

#### 4. Formatting — New section (`src/tools/deep_dive/formatting.rs`)

```rust
fn format_similar_section(out: &mut String, similar: &[SimilarEntry])
```

Called at the end of `format_symbol_context` (after the kind-specific formatter), only when `similar` is non-empty.

**Output format:**
```
Semantically Similar:
  verify_credentials     0.92  src/auth/legacy.rs:12 (function, public)
  validate_user_login    0.88  src/auth/oauth.rs:67 (function, public)
  check_authentication   0.85  src/middleware/auth.rs:31 (function, private)
```

- Score = `1.0 - distance` (higher = more similar)
- Kind + visibility in parens
- Sorted by descending score (natural KNN order)
- Max 5 entries

#### 5. MCP Tool Description Update (`src/handler.rs`)

Update the `deep_dive` tool description to mention that `full` depth includes semantically similar symbols when embeddings are available.

## Testing Plan

### Unit Tests

| Test | File | What it verifies |
|------|------|-----------------|
| `test_get_embedding_returns_stored_vector` | `src/tests/core/vector_storage.rs` | Round-trip: store embedding → retrieve it |
| `test_get_embedding_returns_none_for_missing` | `src/tests/core/vector_storage.rs` | Returns `None` for non-existent symbol |
| `test_similar_symbols_at_full_depth` | `src/tests/tools/deep_dive_tests.rs` | Similar section populated when embeddings exist |
| `test_similar_symbols_skipped_when_no_embeddings` | `src/tests/tools/deep_dive_tests.rs` | Graceful empty vec when no embeddings |
| `test_similar_symbols_excludes_self` | `src/tests/tools/deep_dive_tests.rs` | Query symbol filtered from its own results |
| `test_similar_symbols_not_at_context_depth` | `src/tests/tools/deep_dive_tests.rs` | `similar` is empty at overview/context depth |

### Integration (Dogfood)

| Test | File | What it verifies |
|------|------|-----------------|
| `test_deep_dive_full_shows_similar_on_real_codebase` | `src/tests/tools/search_quality/` | KNN returns meaningful results on Julie's own embedded symbols |

## Exit Criteria

1. `deep_dive(symbol="hybrid_search", depth="full")` on Julie's own codebase shows semantically similar search-related functions
2. `deep_dive(symbol="X", depth="context")` does NOT show similar section (progressive disclosure)
3. `deep_dive` on a symbol without embeddings shows no similar section (graceful degradation)
4. All existing deep_dive tests continue to pass (no regressions)
5. Fast-tier tests pass

## Non-Goals

- **Cross-workspace similarity** — future enhancement, not in this phase
- **Clustering / `semantic_group`** — column stays unused for now
- **Similarity threshold filtering** — show top 5 regardless of score; the agent can interpret scores
- **New tool or parameter on `fast_refs`/`fast_search`** — single surface in `deep_dive` only
