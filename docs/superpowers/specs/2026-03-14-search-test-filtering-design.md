# Smart Test Filtering for fast_search

## Problem

When AI agents search for production code ("payment processing", "auth middleware"), test symbols pollute results. Julie currently applies a soft 0.95x path-based scoring penalty, but test functions still appear prominently. Phase 1 of Code Health Intelligence now tags every test symbol with `metadata["is_test"] = true` at the symbol level — we can use this for precise, hard filtering.

## Design

### New parameter on `FastSearchTool`

```rust
/// Exclude test symbols from results.
/// Default: auto (excludes for NL queries, includes for definition searches).
/// Set explicitly to override.
#[serde(default)]
pub exclude_tests: Option<bool>,
```

### Smart default resolution

When `exclude_tests` is `None` (not explicitly set by agent):

| Condition | Resolved value | Rationale |
|-----------|---------------|-----------|
| `search_target == "definitions"` | `false` | Agent may be looking for a test function by name |
| NL query detected | `true` | Exploratory queries want production code |
| Otherwise | `false` | Preserve current behavior for ambiguous cases |

NL detection: reuse existing `is_nl_like_query()` from `src/search/scoring.rs`.

When `exclude_tests` is explicitly `Some(true)` or `Some(false)`, use that value regardless of query type.

### Filter implementation

**Important:** `SymbolSearchResult` (from Tantivy/hybrid search) does NOT carry symbol metadata — it only has id, name, signature, score, etc. The `is_test` flag lives in SQLite's `symbols.metadata` JSON column and only becomes available after `enrich_symbols_from_db()` hydrates full `Symbol` objects.

Therefore, the filter applies **after enrichment** in `text_search.rs`, not inside `matches_filter()` in `hybrid.rs`:

1. Add `exclude_tests: bool` to `SearchFilter` (resolved from `Option<bool>` + smart default)
2. In `text_search_impl()` / `definition_search_with_index()` in `text_search.rs`, after calling `enrich_symbols_from_db()`, filter out `Symbol` objects where `metadata["is_test"] == true` when `filter.exclude_tests` is set
3. `matches_filter()` in `hybrid.rs` is NOT modified — it operates on `SymbolSearchResult` which lacks metadata

This is a post-enrichment filter, similar to how search results are already capped, deduplicated, and scored after enrichment.

### Line-mode / content search

The `exclude_tests` parameter applies only to definition/symbol searches. For `search_target == "content"` (line-mode search), the parameter is ignored — line-mode searches file content directly, not symbols, so `is_test` metadata doesn't apply. The existing `is_test_path()` scoring penalty remains as the soft signal for content searches.

### get_context integration

`get_context` already handles test exclusion via `is_test_path()` checks in `select_pivots()`, `build_neighbor_entries()`, and `get_pivot_relationship_names_batched()`. These path-based checks effectively suppress test symbols from pivots.

For Phase 2 scope: keep the existing path-based approach in `get_context`. The `is_test` metadata provides a stronger signal, but wiring it through `get_context` requires a separate DB lookup step that doesn't exist in the current pipeline. The path-based approach is sufficient and already working. We can upgrade to metadata-based filtering in a later pass if the path heuristic proves insufficient.

### What does NOT change

- `deep_dive` — already shows test quality tiers, no filtering needed
- `fast_refs` — test references are legitimate when checking "who calls X"
- `get_context` — existing `is_test_path()` approach is sufficient for now
- Existing `is_test_path()` 0.95x scoring penalty — stays as soft signal (catches non-function symbols in test files, and cases where metadata isn't available)

## Files to modify

| File | Change |
|------|--------|
| `src/tools/search/mod.rs` | Add `exclude_tests: Option<bool>` to `FastSearchTool` |
| `src/search/index.rs` | Add `exclude_tests: bool` to `SearchFilter` |
| `src/tools/search/text_search.rs` | Resolve smart default + filter enriched symbols |

No new files. No schema changes. ~40-60 lines total.

## Testing

- Unit test: filtering function correctly excludes `Symbol` objects with `is_test` metadata
- Unit test: smart default resolves correctly for NL vs definition vs content searches
- Integration test: `fast_search` with NL query excludes test results; same query with `exclude_tests: false` includes them
- Integration test: definition search includes test results by default
- Regression: existing search tests still pass (default behavior unchanged for definition searches)
