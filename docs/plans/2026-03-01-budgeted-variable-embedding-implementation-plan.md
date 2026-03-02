# Budgeted Variable Embedding Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add budgeted variable embedding to improve dogfood retrieval precision and cross-language consistency while enforcing strict `+20%` overhead caps.

**Architecture:** Keep existing embeddable symbol flow, then add a deterministic variable-selection pass ranked by language-agnostic importance signals (`reference_score`, API-surface cues, and noise penalties). Wire policy into full and incremental embedding paths, and add explicit stale-vector cleanup for variables that are no longer selected.

**Tech Stack:** Rust, sqlite-vec, existing embedding pipeline (`src/embeddings/*`), Rust test suite (`src/tests/*`).

---

### Task 1: Add RED tests for variable policy scoring and budget cap

**Files:**
- Modify: `src/tests/core/embedding_metadata.rs`
- Test: `src/tests/core/embedding_metadata.rs`

**Step 1: Write the failing test**

Add tests that describe policy behavior before implementation:

```rust
#[test]
fn test_budgeted_variable_policy_prefers_high_signal_variables() {
    let symbols = vec![
        // high signal: export/public/store-like variable
        make_symbol_with_lang("v1", "usePagesStore", SymbolKind::Variable, "typescript"),
        // lower signal local-like variable
        make_symbol_with_lang("v2", "tmp", SymbolKind::Variable, "typescript"),
    ];

    let scores = std::collections::HashMap::from([
        ("v1".to_string(), 12.0),
        ("v2".to_string(), 0.5),
    ]);

    let policy = crate::embeddings::metadata::VariableEmbeddingPolicy {
        enabled: true,
        max_ratio: 0.20,
    };

    let selected = crate::embeddings::metadata::select_budgeted_variables(&symbols, &scores, 10, &policy);
    assert!(selected.iter().any(|(id, _)| id == "v1"));
    assert!(!selected.iter().any(|(id, _)| id == "v2"));
}
```

Add two more tests:
- cap enforcement (`selected.len() <= floor(base_count * 0.20)`)
- deterministic tie-break (stable ordering by score then id)

**Step 2: Run test to verify it fails**

Run: `cargo test --lib embedding_metadata -- --nocapture 2>&1 | tail -40`
Expected: FAIL with missing policy types/functions.

**Step 3: Write minimal implementation stubs**

In `src/embeddings/metadata.rs`, add compile-only stubs for:

```rust
pub struct VariableEmbeddingPolicy {
    pub enabled: bool,
    pub max_ratio: f64,
}

pub fn select_budgeted_variables(
    _symbols: &[Symbol],
    _reference_scores: &std::collections::HashMap<String, f64>,
    _base_count: usize,
    _policy: &VariableEmbeddingPolicy,
) -> Vec<(String, String)> {
    Vec::new()
}
```

**Step 4: Run test to verify failure changes from compile error to assertion error**

Run: `cargo test --lib embedding_metadata -- --nocapture 2>&1 | tail -40`
Expected: FAIL on assertion mismatch (RED achieved).

**Step 5: Commit**

```bash
git add src/tests/core/embedding_metadata.rs src/embeddings/metadata.rs
git commit -m "test: add red coverage for budgeted variable embedding policy"
```

### Task 2: Implement variable scoring and deterministic budgeted selection

**Files:**
- Modify: `src/embeddings/metadata.rs`
- Test: `src/tests/core/embedding_metadata.rs`

**Step 1: Write the failing test for scoring components**

Add focused tests for:
- API-surface boost (public/exported variables > locals)
- reference-score contribution
- noise penalties (short throwaway names and destructuring-like symbols)

**Step 2: Run test to verify it fails**

Run: `cargo test --lib embedding_metadata -- --nocapture 2>&1 | tail -40`
Expected: FAIL due to scoring returning default/incorrect ranking.

**Step 3: Write minimal implementation**

Implement in `src/embeddings/metadata.rs`:

```rust
fn variable_signal_score(symbol: &Symbol, reference_scores: &HashMap<String, f64>) -> f64 {
    let graph = *reference_scores.get(&symbol.id).unwrap_or(&0.0);
    let api_surface = match symbol.visibility {
        Some(Visibility::Public) => 2.0,
        _ => 0.0,
    };
    let name = symbol.name.as_str();
    let noise_penalty = if name.len() <= 2 || name == "tmp" { 1.5 } else { 0.0 };
    graph + api_surface - noise_penalty
}
```

Then sort candidates by `(score desc, id asc)` and truncate to `floor(base_count * policy.max_ratio)`.

**Step 4: Run test to verify it passes**

Run: `cargo test --lib embedding_metadata -- --nocapture 2>&1 | tail -40`
Expected: PASS for new policy tests.

**Step 5: Commit**

```bash
git add src/embeddings/metadata.rs src/tests/core/embedding_metadata.rs
git commit -m "feat: implement deterministic budgeted variable selection for embeddings"
```

### Task 3: Add DB API for stale variable vector cleanup (TDD)

**Files:**
- Modify: `src/database/vectors.rs`
- Modify: `src/tests/core/database.rs`
- Test: `src/tests/core/database.rs`

**Step 1: Write the failing test**

Add a database test that inserts vectors for three symbols, deletes two by id, and verifies one remains:

```rust
#[test]
fn test_delete_embeddings_for_symbol_ids_deletes_only_requested_rows() {
    // setup DB + symbols + vectors
    // call delete_embeddings_for_symbol_ids(["id_a", "id_b"])
    // assert count removed == 2 and id_c still has embedding
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --lib delete_embeddings_for_symbol_ids -- --nocapture 2>&1 | tail -40`
Expected: FAIL with missing method.

**Step 3: Write minimal implementation**

In `src/database/vectors.rs`, add:

```rust
pub fn delete_embeddings_for_symbol_ids(&mut self, symbol_ids: &[String]) -> Result<usize> {
    if symbol_ids.is_empty() {
        return Ok(0);
    }
    let placeholders: Vec<&str> = symbol_ids.iter().map(|_| "?").collect();
    let sql = format!(
        "DELETE FROM symbol_vectors WHERE symbol_id IN ({})",
        placeholders.join(",")
    );
    let params: Vec<&dyn rusqlite::types::ToSql> = symbol_ids
        .iter()
        .map(|id| id as &dyn rusqlite::types::ToSql)
        .collect();
    Ok(self.conn.execute(&sql, params.as_slice())?)
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test --lib delete_embeddings_for_symbol_ids -- --nocapture 2>&1 | tail -40`
Expected: PASS.

**Step 5: Commit**

```bash
git add src/database/vectors.rs src/tests/core/database.rs
git commit -m "feat: add embedding cleanup API for selected symbol ids"
```

### Task 4: Wire budgeted variable policy into full embedding pipeline

**Files:**
- Modify: `src/embeddings/pipeline.rs`
- Modify: `src/embeddings/metadata.rs`
- Modify: `src/tests/integration/embedding_incremental.rs`
- Test: `src/tests/integration/embedding_incremental.rs`

**Step 1: Write failing integration tests**

Add tests that assert:
- variable embeddings are capped relative to non-variable baseline
- high-signal variable is embedded under cap
- stale variable vectors are removed after policy de-select

**Step 2: Run test to verify it fails**

Run: `cargo test --lib embedding_incremental -- --nocapture 2>&1 | tail -60`
Expected: FAIL on missing pipeline behavior.

**Step 3: Write minimal implementation**

In `run_embedding_pipeline`:
- compute base embeddable set (existing behavior)
- fetch reference scores for variable ids
- select budgeted variables using `select_budgeted_variables`
- merge base + selected variable embeddings
- remove stale variable vectors with `delete_embeddings_for_symbol_ids`
- log counters (`candidate_count`, `selected_count`, `budget_cap`, `stale_deleted`)

Use existing graceful error handling; do not fail keyword search path.

**Step 4: Run test to verify it passes**

Run: `cargo test --lib embedding_incremental -- --nocapture 2>&1 | tail -60`
Expected: PASS for new integration cases.

**Step 5: Commit**

```bash
git add src/embeddings/pipeline.rs src/embeddings/metadata.rs src/tests/integration/embedding_incremental.rs
git commit -m "feat: apply budgeted variable policy in full embedding pipeline"
```

### Task 5: Keep incremental file embedding behavior consistent + add dogfood evaluation scaffolding

**Files:**
- Modify: `src/embeddings/pipeline.rs`
- Create: `fixtures/benchmarks/labhandbookv2_dogfood_queries.jsonl`
- Create: `src/tests/tools/search_quality/labhandbook_dogfood.rs`
- Modify: `src/tests/tools/search_quality/mod.rs`
- Test: `src/tests/tools/search_quality/labhandbook_dogfood.rs`

**Step 1: Write failing tests for metrics helpers**

Add pure tests for:
- `Hit@k` computation
- `MRR@10` computation
- `OffTopic@5` computation
- `CrossLangRecall@5` computation

**Step 2: Run test to verify it fails**

Run: `cargo test --lib labhandbook_dogfood -- --nocapture 2>&1 | tail -60`
Expected: FAIL with missing metric helpers.

**Step 3: Write minimal implementation**

Implement:
- metric helper functions in `labhandbook_dogfood.rs`
- fixture loader for query JSONL
- optional ignored test that requires a configured `LabHandbookV2` reference workspace
- incremental pipeline path uses the same variable policy selection logic as full run

Example metric helper signature:

```rust
fn hit_at_k(results: &[String], expected: &std::collections::HashSet<String>, k: usize) -> f64
```

**Step 4: Run test to verify it passes**

Run: `cargo test --lib labhandbook_dogfood -- --nocapture 2>&1 | tail -60`
Expected: PASS for pure metric tests (workspace-dependent test can be `#[ignore]`).

**Step 5: Commit**

```bash
git add src/embeddings/pipeline.rs src/tests/tools/search_quality/mod.rs src/tests/tools/search_quality/labhandbook_dogfood.rs fixtures/benchmarks/labhandbookv2_dogfood_queries.jsonl
git commit -m "test: add dogfood quality metric scaffolding for variable embedding evaluation"
```

### Task 6: Final verification pass and docs sync

**Files:**
- Modify: `TODO.md`
- Modify: `docs/plans/2026-03-01-embedding-quality-design.md`

**Step 1: Write failing check (if needed) for changed assumptions**

If any design assumptions changed during implementation, add a short failing assertion test first (smallest affected module) and then update code.

**Step 2: Run targeted verification suite**

Run:

- `cargo test --lib embedding_metadata -- --nocapture 2>&1 | tail -40`
- `cargo test --lib embedding_incremental -- --nocapture 2>&1 | tail -40`
- `cargo test --lib labhandbook_dogfood -- --nocapture 2>&1 | tail -40`
- `cargo test --lib -- --skip search_quality 2>&1 | tail -20`

Expected: PASS on updated tests and fast-tier suite.

**Step 3: Update docs/todo checkboxes**

- Mark completed implementation/evaluation line items in `TODO.md`.
- Add measured overhead and quality outcomes to the design doc.

**Step 4: Commit**

```bash
git add TODO.md docs/plans/2026-03-01-embedding-quality-design.md
git commit -m "docs: record budgeted variable embedding results and rollout status"
```
