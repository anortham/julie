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

**Strategy 1 — Relationships (high confidence):** Query relationships where `from_symbol` has `metadata["is_test"] = true` and `to_symbol` does not. These are direct "test calls production code" edges (kind = "Calls", "Uses", etc.). Resolved by the relationship resolver, so linkage is precise.

**Strategy 2 — Identifiers (medium confidence):** Query identifiers where the containing symbol has `is_test = true`. Prefer `target_symbol_id` when populated (precise linkage). When `target_symbol_id` is NULL (common — resolved on-demand, not at index time), fall back to name matching against non-test symbols, disambiguated by file proximity (same directory > same parent > anywhere). This fallback can produce false positives for common names, but the deduplication step limits the damage.

### Algorithm

New function `compute_test_coverage(db: &SymbolDatabase) -> Result<TestCoverageStats>`:

**Step 1 — Relationship-based linkage:**

```sql
SELECT r.to_symbol_id AS prod_id, s_test.id AS test_id, s_test.name AS test_name,
       json_extract(s_test.metadata, '$.test_quality.quality_tier') AS tier
FROM relationships r
JOIN symbols s_test ON r.from_symbol_id = s_test.id
JOIN symbols s_prod ON r.to_symbol_id = s_prod.id
WHERE json_extract(s_test.metadata, '$.is_test') = 1
  AND (json_extract(s_prod.metadata, '$.is_test') IS NULL
       OR json_extract(s_prod.metadata, '$.is_test') != 1)
  AND r.kind IN ('Calls', 'Uses', 'References', 'Instantiates', 'Imports')
```

**Step 2 — Identifier-based linkage (supplements step 1):**

```sql
-- Precise: use target_symbol_id when available
SELECT i.target_symbol_id AS prod_id, s_test.id AS test_id, s_test.name AS test_name,
       json_extract(s_test.metadata, '$.test_quality.quality_tier') AS tier
FROM identifiers i
JOIN symbols s_test ON i.containing_symbol_id = s_test.id
JOIN symbols s_prod ON i.target_symbol_id = s_prod.id
WHERE json_extract(s_test.metadata, '$.is_test') = 1
  AND i.target_symbol_id IS NOT NULL
  AND (json_extract(s_prod.metadata, '$.is_test') IS NULL
       OR json_extract(s_prod.metadata, '$.is_test') != 1)

-- Fallback: name match when target_symbol_id is NULL
-- Disambiguate by preferring symbols in the same directory tree
SELECT s_prod.id AS prod_id, s_test.id AS test_id, s_test.name AS test_name,
       json_extract(s_test.metadata, '$.test_quality.quality_tier') AS tier
FROM identifiers i
JOIN symbols s_test ON i.containing_symbol_id = s_test.id
JOIN symbols s_prod ON s_prod.name = i.name
WHERE json_extract(s_test.metadata, '$.is_test') = 1
  AND i.target_symbol_id IS NULL
  AND (json_extract(s_prod.metadata, '$.is_test') IS NULL
       OR json_extract(s_prod.metadata, '$.is_test') != 1)
  AND s_prod.kind NOT IN ('Import', 'Export', 'Module', 'Namespace')
```

For the name-match fallback, when multiple production symbols match, prefer the one whose `file_path` shares the longest common directory prefix with the test file. This handles the "3 functions named `validate`" case by picking the one closest in the directory tree.

**Step 3** — Deduplicate by `(test_symbol_id, production_symbol_id)` pairs across both strategies.

**Step 4** — For each production symbol, aggregate:
- Count of distinct test symbols that exercise it
- Best and worst quality tier among those tests (from test symbol's `metadata["test_quality"]["quality_tier"]`)
- Names of the covering test functions (capped at 5 for storage)

**Step 5** — Bulk UPDATE production symbols' metadata

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

Four normalized signals (all 0.0–1.0), weighted:

| Signal | Weight | Source | Meaning |
|--------|--------|--------|---------|
| **Centrality** | 0.35 | `reference_score` column (normalized) | How many things depend on this |
| **Visibility** | 0.25 | `visibility` column | Public = 1.0, Protected = 0.5, Private = 0.2, NULL = 0.5 |
| **Test weakness** | 0.30 | `test_coverage` metadata (Layer C) | Inverse of coverage quality |
| **Symbol kind** | 0.10 | `kind` column | Callable = 1.0, container = 0.7, data = 0.3 |

**Centrality normalization:** `reference_score` is an unbounded weighted count (e.g., 0.0 to 50.0+). Normalize to 0.0–1.0 using a logarithmic sigmoid:

```
centrality_normalized = min(1.0, ln(1.0 + reference_score) / ln(1.0 + P95))
```

Where `P95` is the 95th percentile `reference_score` in the workspace, computed once at the start of `compute_change_risk_scores()`. This ensures the top 5% of symbols saturate at 1.0 and the distribution is spread meaningfully across the range. If `P95 = 0` (no reference scores), all centrality values are 0.0.

**Visibility mapping:** NULL visibility (common in Python, Go, JS, C) maps to 0.5 — assume moderate exposure when unknown.

**Symbol kind mapping:**

| Weight | Kinds |
|--------|-------|
| 1.0 (callable) | Function, Method, Constructor, Destructor, Operator |
| 0.7 (container) | Class, Struct, Interface, Trait, Enum, Union, Module, Namespace, Type, Delegate |
| 0.3 (data) | Variable, Constant, Property, Field, EnumMember, Event |
| 0.0 (skip) | Import, Export — excluded from risk scoring entirely |

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

**Implementation:** Requires two changes:

1. **Pipeline** (`src/tools/get_context/pipeline.rs`): When constructing `PivotEntry` from pivot results (~line 365), extract `metadata["change_risk"]["label"]` and store in a new `pub risk_label: Option<String>` field on `PivotEntry`.
2. **Formatting** (`src/tools/get_context/formatting.rs`): When formatting pivot lines, append `[{risk_label} risk]` when the field is `Some`. No label for neighbors (too noisy at that density).

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

Order matters: change risk reads `reference_score` (from step 3), `test_quality` tiers (from step 4), and `test_coverage` (from step 5). Test coverage reads `test_quality` tiers to determine best/worst tier. So steps 3→4→5→6 must run in sequence.

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
| `src/tools/get_context/pipeline.rs` | Add `risk_label` to `PivotEntry`, extract from metadata | ~10 |
| `src/tools/get_context/formatting.rs` | Append risk label to pivot lines | ~10 |
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
