# Phase 2: Graph Centrality Ranking

**Date:** 2026-02-24
**Status:** Design Approved
**Depends on:** Phase 1 (Search Quality) — for OR fallback infrastructure
**Enables:** Phase 3 (get_context tool) — centrality scores used for pivot selection

## Context

Julie extracts rich relationship data via 30 tree-sitter parsers — 14 relationship types including Calls, Extends, Implements, Imports, Uses, and more. This data is stored in SQLite with indexes on both `from_symbol_id` and `to_symbol_id`.

Currently, this relationship data is **only used for display** in the `deep_dive` tool. It plays zero role in search ranking. A function called by 50 other symbols ranks identically to one called by zero, all else being equal.

Graph centrality ranking uses the existing relationship data to boost "important" symbols in search results — leveraging Julie's unique tree-sitter moat for something embeddings can't do.

## Problem Statement

Search ranking today uses:
1. **Field boosting:** name 5x → signature 3x → doc_comment 2x → body 1x
2. **Language pattern boost:** 1.5x for `pub fn`, `public class`, etc.

Neither considers how connected a symbol is. Searching for `"process"` ranks `process_line` (private helper, zero callers) the same as `process_payment` (public, called by 30 symbols).

## Design

### 1. Schema Change: `reference_score` Column

Add a `reference_score REAL DEFAULT 0.0` column to the `symbols` table.

This stores the pre-computed weighted incoming reference count for each symbol. Computed at index time, queried at search time.

### 2. Index-Time Computation

After all symbols and relationships for a workspace are indexed, compute reference scores:

```sql
-- Conceptual query (actual implementation in Rust)
UPDATE symbols SET reference_score = (
  SELECT COALESCE(SUM(
    CASE r.kind
      WHEN 'calls'      THEN 3.0
      WHEN 'implements'  THEN 2.0
      WHEN 'imports'     THEN 2.0
      WHEN 'extends'     THEN 2.0
      WHEN 'uses'        THEN 1.0
      WHEN 'references'  THEN 1.0
      ELSE 1.0
    END
  ), 0.0)
  FROM relationships r
  WHERE r.to_symbol_id = symbols.id
)
```

**Weight rationale:**
- `Calls` (weight 3): Active usage — someone depends on this function's behavior
- `Implements`/`Imports`/`Extends` (weight 2): Structural dependency — changing this breaks dependents
- `Uses`/`References` (weight 1): Awareness — code mentions this but may not directly depend on it

**Performance:** Single SQL query with JOIN, runs once after indexing completes. For Julie's own codebase (~9000 symbols, ~89000 relationships), this should be <100ms.

### 3. Tantivy Integration

Two options for how centrality affects search scoring:

**Option A: Post-search boost (recommended)**

After Tantivy returns results, look up `reference_score` from SQLite and apply boost:

```rust
fn apply_centrality_boost(results: &mut Vec<SymbolSearchResult>, db: &SymbolDatabase) {
    let symbol_ids: Vec<&str> = results.iter().map(|r| r.id.as_str()).collect();
    let scores = db.get_reference_scores(&symbol_ids);  // batch query

    for result in results.iter_mut() {
        if let Some(ref_score) = scores.get(&result.id) {
            // Logarithmic scaling prevents utility functions from dominating
            result.score *= 1.0 + (1.0 + ref_score).ln() * CENTRALITY_WEIGHT;
        }
    }

    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));
}
```

**Option B: Tantivy stored field + custom scorer**

Store `reference_score` in Tantivy as a fast field and use it in a custom scorer. More tightly integrated but more complex, and Tantivy's custom scoring API is less ergonomic.

**Recommendation: Option A.** Post-search boost is simpler, easier to debug, and the extra SQLite lookup is cheap (batch query by ID, indexed). This matches the existing `apply_important_patterns_boost` pattern in `src/search/scoring.rs`.

### 4. Tuning the CENTRALITY_WEIGHT Constant

Start with `CENTRALITY_WEIGHT = 0.3`. This means:

| reference_score | Boost factor |
|----------------|-------------|
| 0 (no refs) | 1.0x (no change) |
| 1 | 1.21x |
| 5 | 1.54x |
| 10 | 1.72x |
| 50 | 2.18x |
| 100 | 2.38x |

This is a meaningful but not dominant signal. A symbol with 50 incoming references gets ~2.2x boost — enough to move it above a text-similar but unconnected symbol, but not enough to override a strong name match.

The constant should be configurable (not hardcoded) so we can tune it based on real-world testing.

### 5. Handling Edge Cases

**Re-indexing:** `reference_score` is recomputed on every full index. Incremental file changes trigger re-indexing of affected files, which updates relationships, which triggers score recomputation for affected symbols.

**New workspaces:** Score starts at 0.0 for all symbols until first full index completes. No degradation — just no boost.

**Self-references:** A symbol calling itself (recursion) should not boost its own score. Filter `WHERE from_symbol_id != to_symbol_id` in the computation query.

**Test files:** References from test files are legitimate signals — a well-tested function IS more important. No special filtering needed.

## Implementation Steps

### Step 1: Schema migration
1. Add `reference_score REAL DEFAULT 0.0` to `symbols` table
2. Add migration in `src/database/migrations.rs`
3. Write test: new column exists and defaults to 0.0

### Step 2: Reference score computation
1. Write failing test: after indexing symbols + relationships, `reference_score` is computed correctly
2. Add `compute_reference_scores()` method to `SymbolDatabase`
3. Implement weighted aggregation query
4. Write test for self-reference exclusion
5. Write test for weight differentiation (Calls > Uses)

### Step 3: Hook into indexing pipeline
1. Find where indexing completes (after relationships are stored)
2. Call `compute_reference_scores()` at that point
3. Write integration test: full index → scores populated

### Step 4: Search scoring integration
1. Write failing test: symbol with high reference_score ranks above equal-text-match with zero references
2. Add `get_reference_scores(ids: &[&str])` batch query to database
3. Add `apply_centrality_boost()` to `src/search/scoring.rs`
4. Call it in `search_symbols` after `apply_important_patterns_boost`
5. Pass test

### Step 5: Tuning and verification
1. Build debug binary, test against Julie's own codebase
2. Compare search results before/after centrality for common queries
3. Verify well-connected symbols (like `extract_symbols`, `search_symbols`) rank higher
4. Adjust CENTRALITY_WEIGHT if needed
5. Verify no regressions in existing search tests

## Success Criteria

- [ ] `reference_score` column populated for all symbols after indexing
- [ ] Weighted scoring: Calls count more than Uses/References
- [ ] Self-references excluded from score computation
- [ ] Search results visibly improved for ambiguous queries (connected symbols rank higher)
- [ ] No regression in exact-match or definition search
- [ ] Performance: score computation < 200ms for 10k symbols
- [ ] CENTRALITY_WEIGHT is configurable, not hardcoded

## Risk Assessment

**Low-medium risk.** The schema change is additive (new column with default). Post-search boosting doesn't touch the Tantivy query path — it only re-ranks after results are returned. If centrality boost causes unexpected ranking issues, setting CENTRALITY_WEIGHT to 0.0 disables it entirely.

**One concern:** Utility functions (like `new()`, `default()`, `fmt()` in Rust) will have very high reference counts. The logarithmic scaling mitigates this, but we may need to consider a dampening factor for extremely common names, or cap the boost at a maximum (e.g., 3x).
