# Test Intelligence — Phase 2 of Code Health Intelligence

## Goal

Give AI agents per-symbol **change risk** scores and **test coverage** data in `deep_dive` and `get_context` output. An agent modifying `process_payment()` should immediately see: "HIGH risk (0.82) — 14 callers, public, thin tests" without any extra tool calls.

## Scope

This spec covers **test intelligence only** — Layers C and D from the Code Health Intelligence roadmap. Security risk signals are a separate spec.

| Layer | Name | What it computes |
|-------|------|-----------------|
| **C** | Test-to-code linkage | Which tests exercise each production symbol, and how well |
| **D** | Change risk scoring | 0.0–1.0 risk score per production symbol |

## Prerequisites (Phase 1 — Complete)

- Layer A: `is_test_symbol()` across 31 languages → `metadata["is_test"] = true`
- Layer B: `compute_test_quality_metrics()` → `metadata["test_quality"] = { quality_tier, assertion_count, ... }`
- `exclude_tests` parameter on `fast_search`
- `reference_score` (graph centrality) computed for all symbols
- `deep_dive` already displays test locations with quality tiers

---

## Layer C: Test-to-Code Linkage

### What it does

For every production symbol, determine which test symbols exercise it and aggregate their quality.

### Data source

Existing tables — no new tables or schema migrations.

1. **Relationships table** — Where `from_symbol` has `metadata["is_test"] = true` and `to_symbol` does not. These are direct "test calls production code" edges (kind = "Calls", "Uses", etc.).
2. **Identifiers table** — Where the containing symbol has `is_test = true` and the referenced name matches a non-test symbol. These capture "test references production symbol" even without a resolved relationship.

### Algorithm

New function `compute_test_coverage(db: &SymbolDatabase) -> Result<TestCoverageStats>`:

1. Query all relationships where `from_symbol` has `metadata["is_test"] = true` and `to_symbol` does NOT
2. Query identifiers where the containing symbol has `is_test = true` and the referenced name matches a non-test symbol
3. Deduplicate by `(test_symbol_id, production_symbol_id)` pairs
4. For each production symbol, aggregate:
   - Count of distinct test symbols that exercise it
   - Best and worst quality tier among those tests (from test symbol's `metadata["test_quality"]["quality_tier"]`)
   - Names of the covering test functions (capped at 5 for storage)
5. Bulk UPDATE production symbols' metadata

### Storage

In the production symbol's existing `metadata` JSON column:

```json
{
  "test_coverage": {
    "test_count": 3,
    "best_tier": "thorough",
    "worst_tier": "thin",
    "covering_tests": ["test_process_payment", "test_refund", "test_partial_payment"]
  }
}
```

Symbols with zero test coverage get no `test_coverage` key (absence = untested).

### Performance

Single pass over relationships + identifiers with `is_test` join. Similar cost to `compute_reference_scores()`. All UPDATEs wrapped in a single transaction.

---

## Layer D: Change Risk Scoring

### What it does

For every non-test symbol, compute a 0.0–1.0 score representing "how risky is it to change this?" High score = important, exposed, and poorly tested.

### Formula

Four normalized signals, weighted:

| Signal | Weight | Source | Meaning |
|--------|--------|--------|---------|
| **Centrality** | 0.35 | `reference_score` column | How many things depend on this |
| **Visibility** | 0.25 | `visibility` column | Public = 1.0, Protected = 0.5, Private = 0.2 |
| **Test weakness** | 0.30 | `test_coverage` metadata (Layer C) | Inverse of coverage quality |
| **Symbol kind** | 0.10 | `kind` column | Functions/methods = 1.0, classes/structs = 0.7, constants/fields = 0.3 |

**Test weakness mapping:**

| Coverage state | Test weakness score |
|---------------|-------------------|
| Untested (no `test_coverage` key) | 1.0 |
| Stub tests only (`best_tier` = "stub") | 0.8 |
| Thin tests only (`best_tier` = "thin") | 0.6 |
| Adequate (`best_tier` = "adequate") | 0.3 |
| Thorough (`best_tier` = "thorough") | 0.1 |

**Final score:**

```
change_risk = 0.35 * centrality + 0.25 * visibility + 0.30 * test_weakness + 0.10 * kind_weight
```

**Tier labels:**

| Score | Label |
|-------|-------|
| ≥ 0.7 | HIGH |
| ≥ 0.4 | MEDIUM |
| < 0.4 | LOW |

### Storage

In the production symbol's existing `metadata` JSON column:

```json
{
  "change_risk": {
    "score": 0.82,
    "label": "HIGH",
    "factors": {
      "centrality": 0.95,
      "visibility": "public",
      "test_weakness": 0.8,
      "kind": "function"
    }
  }
}
```

### Implementation

New function `compute_change_risk_scores(db: &SymbolDatabase) -> Result<ChangeRiskStats>`. Reads `reference_score`, `visibility`, `kind`, and the `test_coverage` metadata just computed by Layer C. Single-pass bulk UPDATE wrapped in a transaction.

---

## Tool Integration

### `deep_dive` — full risk context

After the existing test locations block, append a change risk section. Only shown for production symbols (skip for test symbols).

```
Change Risk: HIGH (0.82) — 14 callers, public, thin tests
  centrality: 0.95 (14 direct callers)
  visibility: public
  test coverage: 3 tests (best: thorough, worst: thin)
  kind: function
```

**Implementation:** New `format_change_risk_info()` in `src/tools/deep_dive/formatting.rs`. Reads `metadata["change_risk"]` and `metadata["test_coverage"]`. Caller count from existing `incoming` refs.

### `get_context` — risk labels on pivots

Append risk label after pivot symbol info. Compact — label only, no breakdown.

```
Pivots:
  process_payment  src/payments.rs:42  (function, public)  [HIGH risk]
  validate_input   src/validation.rs:18  (function, public)  [MEDIUM risk]
  PaymentConfig    src/config.rs:5  (struct, public)  [LOW risk]
```

**Implementation:** In `src/tools/get_context/formatting.rs`, extract risk label from `metadata["change_risk"]["label"]` when formatting pivots. No label for neighbors (too noisy at that density).

### What does NOT change

- `fast_search` — no risk labels in search results
- `fast_refs` — references are factual, not risk-assessed
- `rename_symbol` — no risk labels

---

## Pipeline Order

Extended from current indexing pipeline:

```
Extract & Store
  → Resolve Relationships
  → compute_reference_scores()          [existing]
  → compute_test_quality_metrics()      [existing]
  → compute_test_coverage()             [NEW - Layer C]
  → compute_change_risk_scores()        [NEW - Layer D]
```

Order matters: each step depends on the previous. Change risk needs test coverage, which needs test quality, which needs reference scores.

Hook point: `src/tools/workspace/indexing/processor.rs`, after the existing `compute_test_quality_metrics()` call (~line 520).

---

## File Structure

| File | Change | Est. lines |
|------|--------|-----------|
| `src/analysis/mod.rs` | Add `pub mod test_coverage; pub mod change_risk;` | ~4 |
| `src/analysis/test_coverage.rs` | **NEW** — `compute_test_coverage()` | ~200 |
| `src/analysis/change_risk.rs` | **NEW** — `compute_change_risk_scores()` | ~150 |
| `src/tools/workspace/indexing/processor.rs` | Hook two new functions after test quality | ~6 |
| `src/tools/deep_dive/formatting.rs` | Add `format_change_risk_info()` | ~40 |
| `src/tools/get_context/formatting.rs` | Append risk label to pivot lines | ~15 |
| `src/tests/analysis/test_coverage_tests.rs` | **NEW** — linkage computation tests | ~300 |
| `src/tests/analysis/change_risk_tests.rs` | **NEW** — risk scoring tests | ~250 |

No schema migrations. No new tables. All storage in existing `metadata` JSON column.

### What does NOT change

- Extractors — no modifications
- `test_detection.rs` — already complete
- `test_quality.rs` — already complete
- Database schema — no migrations
- `fast_search` — already has `exclude_tests`
- `fast_refs` — no risk labels

---

## Testing Strategy

### Unit tests

**test_coverage_tests.rs:**
- Test linkage via Calls relationships (test → production)
- Test linkage via identifiers (test file references production symbol)
- Test deduplication (same test exercises same symbol via call + identifier)
- Test aggregation (best/worst tier, count, names capped at 5)
- Test that test-to-test relationships are excluded
- Test that symbols with zero coverage get no `test_coverage` key

**change_risk_tests.rs:**
- Test score computation for each visibility level
- Test score computation for each quality tier
- Test score computation for each symbol kind
- Test tier label boundaries (0.4, 0.7)
- Test that test symbols are excluded from risk scoring
- Test edge cases: no reference_score, no visibility, no test_coverage

### Integration tests

- Build Julie fixture database → run `compute_test_coverage()` → verify known test functions link to known production functions
- Run `compute_change_risk_scores()` → verify known high-centrality untested functions get HIGH risk
- Call `deep_dive` on a well-tested function → verify risk section appears with correct label
- Call `get_context` → verify pivot lines include risk labels

### Dogfood validation

- Build release, restart Claude Code
- `deep_dive(symbol="compute_reference_scores")` → should show test coverage + change risk
- `get_context(query="search scoring")` → pivots should have risk labels
- Verify risk labels make intuitive sense for Julie's own codebase
