# Phase 2: Graph Centrality Ranking — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Boost search ranking of well-connected symbols using pre-computed reference scores from the relationship graph.

**Architecture:** Add a `reference_score` column to the `symbols` table via migration 009. Compute weighted incoming reference counts after indexing completes. Apply logarithmic centrality boost during post-search scoring in `src/search/scoring.rs`.

**Tech Stack:** rusqlite (schema migration, aggregation query), Tantivy (post-search reranking), existing relationship infrastructure (14 edge types)

---

### Task 1: Schema Migration — Add reference_score Column

**Files:**
- Modify: `src/database/migrations.rs:9` (bump LATEST_SCHEMA_VERSION to 9)
- Modify: `src/database/migrations.rs:90-103` (add case 9 to apply_migration)
- Modify: `src/database/migrations.rs:107-113` (add description for version 9)
- Modify: `src/database/schema.rs:96-130` (add reference_score to CREATE TABLE)

**Step 1: Write the failing test**

Create new test file `src/tests/database/migration_009_tests.rs`:

```rust
//! Tests for migration 009: Add reference_score column to symbols table.

use tempfile::TempDir;
use crate::database::SymbolDatabase;

#[test]
fn test_migration_009_adds_reference_score_column() {
    let temp_dir = TempDir::new().unwrap();
    let db = SymbolDatabase::new(temp_dir.path().join("test.db")).unwrap();

    // After migration, the reference_score column should exist with default 0.0
    let result: f64 = db
        .conn()
        .query_row(
            "SELECT COALESCE(reference_score, -1.0) FROM symbols LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0.0);  // No rows is fine, just checking column exists

    // If column doesn't exist, the query above would error
    // Insert a symbol and verify default
    db.conn()
        .execute(
            "INSERT INTO files (path, language, file_hash, last_indexed) VALUES ('test.rs', 'rust', 'abc', 0)",
            [],
        )
        .unwrap();
    db.conn()
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path) VALUES ('s1', 'test_fn', 'function', 'rust', 'test.rs')",
            [],
        )
        .unwrap();

    let score: f64 = db
        .conn()
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 's1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(score, 0.0, "Default reference_score should be 0.0");
}
```

Register in `src/tests/mod.rs` under the database test section (or the appropriate existing location for database tests).

**Step 2: Run test to verify it fails**

Run: `cargo test test_migration_009_adds_reference_score_column 2>&1 | tail -20`
Expected: FAIL — column does not exist

**Step 3: Implement migration 009**

In `src/database/migrations.rs`:

1. Change line 9: `pub const LATEST_SCHEMA_VERSION: i32 = 9;`

2. Add case to `apply_migration` (after line 99):
```rust
9 => self.migration_009_add_reference_score()?,
```

3. Add description (in `record_migration` match):
```rust
9 => "Add reference_score column for graph centrality ranking",
```

4. Add migration function:
```rust
/// Migration 009: Add reference_score column for graph centrality ranking.
///
/// Stores pre-computed weighted incoming reference count per symbol.
/// Used by search scoring to boost well-connected symbols.
fn migration_009_add_reference_score(&self) -> Result<()> {
    info!("Running migration 009: Add reference_score column");
    self.conn.execute(
        "ALTER TABLE symbols ADD COLUMN reference_score REAL NOT NULL DEFAULT 0.0",
        [],
    )?;
    info!("✅ Added reference_score column to symbols table");
    Ok(())
}
```

5. Update `src/database/schema.rs` — add `reference_score REAL NOT NULL DEFAULT 0.0` to the CREATE TABLE statement for new databases.

**Step 4: Run test to verify it passes**

Run: `cargo test test_migration_009_adds_reference_score_column 2>&1 | tail -20`
Expected: PASS

**Step 5: Commit**

```bash
git add src/database/migrations.rs src/database/schema.rs src/tests/database/migration_009_tests.rs src/tests/mod.rs
git commit -m "feat: add reference_score column to symbols table (migration 009)"
```

---

### Task 2: Compute Reference Scores

**Files:**
- Modify: `src/database/relationships.rs` (add compute_reference_scores method)
- Test: `src/tests/database/migration_009_tests.rs` (add computation tests)

**Step 1: Write the failing test**

Append to migration_009_tests.rs:

```rust
#[test]
fn test_compute_reference_scores_weighted() {
    let temp_dir = TempDir::new().unwrap();
    let db = SymbolDatabase::new(temp_dir.path().join("test.db")).unwrap();

    // Setup: file, two symbols, relationships of different kinds
    db.conn().execute(
        "INSERT INTO files (path, language, file_hash, last_indexed) VALUES ('test.rs', 'rust', 'abc', 0)",
        [],
    ).unwrap();
    db.conn().execute(
        "INSERT INTO symbols (id, name, kind, language, file_path) VALUES ('caller', 'caller_fn', 'function', 'rust', 'test.rs')",
        [],
    ).unwrap();
    db.conn().execute(
        "INSERT INTO symbols (id, name, kind, language, file_path) VALUES ('target', 'target_fn', 'function', 'rust', 'test.rs')",
        [],
    ).unwrap();
    db.conn().execute(
        "INSERT INTO symbols (id, name, kind, language, file_path) VALUES ('importer', 'importer_fn', 'function', 'rust', 'test.rs')",
        [],
    ).unwrap();

    // caller --calls--> target (weight 3)
    db.conn().execute(
        "INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind) VALUES ('r1', 'caller', 'target', 'calls')",
        [],
    ).unwrap();
    // importer --imports--> target (weight 2)
    db.conn().execute(
        "INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind) VALUES ('r2', 'importer', 'target', 'imports')",
        [],
    ).unwrap();

    db.compute_reference_scores().unwrap();

    let score: f64 = db.conn().query_row(
        "SELECT reference_score FROM symbols WHERE id = 'target'",
        [],
        |row| row.get(0),
    ).unwrap();

    // calls=3 + imports=2 = 5.0
    assert_eq!(score, 5.0, "target should have score 5.0 (calls=3 + imports=2)");

    // caller and importer have no incoming refs, score = 0
    let caller_score: f64 = db.conn().query_row(
        "SELECT reference_score FROM symbols WHERE id = 'caller'",
        [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(caller_score, 0.0, "caller has no incoming refs");
}

#[test]
fn test_compute_reference_scores_excludes_self_refs() {
    let temp_dir = TempDir::new().unwrap();
    let db = SymbolDatabase::new(temp_dir.path().join("test.db")).unwrap();

    db.conn().execute(
        "INSERT INTO files (path, language, file_hash, last_indexed) VALUES ('test.rs', 'rust', 'abc', 0)",
        [],
    ).unwrap();
    db.conn().execute(
        "INSERT INTO symbols (id, name, kind, language, file_path) VALUES ('recursive', 'recursive_fn', 'function', 'rust', 'test.rs')",
        [],
    ).unwrap();

    // Self-reference (recursion) — should NOT count
    db.conn().execute(
        "INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind) VALUES ('r1', 'recursive', 'recursive', 'calls')",
        [],
    ).unwrap();

    db.compute_reference_scores().unwrap();

    let score: f64 = db.conn().query_row(
        "SELECT reference_score FROM symbols WHERE id = 'recursive'",
        [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(score, 0.0, "Self-references should not contribute to score");
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test test_compute_reference_scores 2>&1 | tail -20`
Expected: FAIL — `compute_reference_scores` method does not exist

**Step 3: Implement compute_reference_scores**

In `src/database/relationships.rs`, add:

```rust
/// Relationship kind weights for centrality scoring.
/// Higher weight = stronger signal of importance.
const RELATIONSHIP_WEIGHTS: &[(&str, f64)] = &[
    ("calls", 3.0),
    ("implements", 2.0),
    ("imports", 2.0),
    ("extends", 2.0),
    ("instantiates", 2.0),
    ("uses", 1.0),
    ("references", 1.0),
    ("returns", 1.0),
    ("parameter", 1.0),
    ("defines", 1.0),
    ("overrides", 1.0),
    ("contains", 0.0),  // structural, not a usage signal
    ("joins", 1.0),
    ("composition", 1.0),
];

impl SymbolDatabase {
    /// Compute reference_score for all symbols based on weighted incoming relationships.
    /// Self-references (recursion) are excluded.
    pub fn compute_reference_scores(&self) -> Result<()> {
        // Build the CASE expression for weighted counting
        let case_expr = RELATIONSHIP_WEIGHTS
            .iter()
            .map(|(kind, weight)| format!("WHEN '{}' THEN {}", kind, weight))
            .collect::<Vec<_>>()
            .join(" ");

        let sql = format!(
            "UPDATE symbols SET reference_score = COALESCE(
                (SELECT SUM(CASE r.kind {} ELSE 1.0 END)
                 FROM relationships r
                 WHERE r.to_symbol_id = symbols.id
                   AND r.from_symbol_id != symbols.id),
                0.0
            )",
            case_expr
        );

        self.conn().execute(&sql, [])?;
        Ok(())
    }
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test test_compute_reference_scores 2>&1 | tail -20`
Expected: PASS (both weighted and self-ref exclusion tests)

**Step 5: Run full test suite**

Run: `cargo test 2>&1 | tail -20`
Expected: All pass

**Step 6: Commit**

```bash
git add src/database/relationships.rs src/tests/database/migration_009_tests.rs
git commit -m "feat: compute weighted reference_score from incoming relationships"
```

---

### Task 3: Hook into Indexing Pipeline

**Files:**
- Modify: `src/tools/workspace/indexing/processor.rs:461` (after stats.log_summary())

**Step 1: Write the failing test**

This is an integration test — after a full index, reference_score should be populated. Add to the migration test file or create a focused integration test:

```rust
#[test]
fn test_indexing_populates_reference_scores() {
    // After full indexing with relationships, reference_score should be non-zero
    // for symbols that have incoming relationships.
    // Use the existing test infrastructure for creating a temp workspace + indexing.
    // This test verifies the hook point, not the computation (Task 2 covers that).

    let temp_dir = TempDir::new().unwrap();
    let db = SymbolDatabase::new(temp_dir.path().join("test.db")).unwrap();

    // Insert file, symbols, and relationships
    db.conn().execute(
        "INSERT INTO files (path, language, file_hash, last_indexed) VALUES ('test.rs', 'rust', 'abc', 0)",
        [],
    ).unwrap();
    db.conn().execute(
        "INSERT INTO symbols (id, name, kind, language, file_path) VALUES ('a', 'fn_a', 'function', 'rust', 'test.rs')",
        [],
    ).unwrap();
    db.conn().execute(
        "INSERT INTO symbols (id, name, kind, language, file_path) VALUES ('b', 'fn_b', 'function', 'rust', 'test.rs')",
        [],
    ).unwrap();
    db.conn().execute(
        "INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind) VALUES ('r1', 'a', 'b', 'calls')",
        [],
    ).unwrap();

    // Simulate what the indexing pipeline does after storing relationships
    db.compute_reference_scores().unwrap();

    let score: f64 = db.conn().query_row(
        "SELECT reference_score FROM symbols WHERE id = 'b'",
        [],
        |row| row.get(0),
    ).unwrap();
    assert!(score > 0.0, "reference_score should be populated after indexing");
}
```

**Step 2: Add the hook to processor.rs**

In `src/tools/workspace/indexing/processor.rs`, after line 461 (`stats.log_summary()`), before line 462 (`drop(db_lock)`):

```rust
// Compute graph centrality reference scores
if let Err(e) = db_lock.compute_reference_scores() {
    warn!("Failed to compute reference scores: {}", e);
}
```

**Step 3: Run test and verify**

Run: `cargo test test_indexing_populates_reference_scores 2>&1 | tail -20`
Expected: PASS

**Step 4: Run full test suite**

Run: `cargo test 2>&1 | tail -20`
Expected: All pass

**Step 5: Commit**

```bash
git add src/tools/workspace/indexing/processor.rs src/tests/database/migration_009_tests.rs
git commit -m "feat: compute reference scores after indexing completes"
```

---

### Task 4: Batch Query for Reference Scores

**Files:**
- Modify: `src/database/symbols/queries.rs` (add get_reference_scores method)
- Test: `src/tests/database/migration_009_tests.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_get_reference_scores_batch() {
    let temp_dir = TempDir::new().unwrap();
    let db = SymbolDatabase::new(temp_dir.path().join("test.db")).unwrap();

    db.conn().execute(
        "INSERT INTO files (path, language, file_hash, last_indexed) VALUES ('test.rs', 'rust', 'abc', 0)",
        [],
    ).unwrap();
    db.conn().execute(
        "INSERT INTO symbols (id, name, kind, language, file_path, reference_score) VALUES ('s1', 'fn_a', 'function', 'rust', 'test.rs', 5.0)",
        [],
    ).unwrap();
    db.conn().execute(
        "INSERT INTO symbols (id, name, kind, language, file_path, reference_score) VALUES ('s2', 'fn_b', 'function', 'rust', 'test.rs', 12.0)",
        [],
    ).unwrap();
    db.conn().execute(
        "INSERT INTO symbols (id, name, kind, language, file_path, reference_score) VALUES ('s3', 'fn_c', 'function', 'rust', 'test.rs', 0.0)",
        [],
    ).unwrap();

    let scores = db.get_reference_scores(&["s1", "s2", "s3"]).unwrap();
    assert_eq!(scores.get("s1"), Some(&5.0));
    assert_eq!(scores.get("s2"), Some(&12.0));
    assert_eq!(scores.get("s3"), Some(&0.0));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_get_reference_scores_batch 2>&1 | tail -20`
Expected: FAIL — method does not exist

**Step 3: Implement get_reference_scores**

In `src/database/symbols/queries.rs`:

```rust
use std::collections::HashMap;

impl SymbolDatabase {
    /// Get reference_score for a batch of symbol IDs.
    /// Returns a HashMap of id → score.
    pub fn get_reference_scores(&self, ids: &[&str]) -> Result<HashMap<String, f64>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT id, reference_score FROM symbols WHERE id IN ({})",
            placeholders
        );
        let params: Vec<&dyn rusqlite::types::ToSql> =
            ids.iter().map(|id| id as &dyn rusqlite::types::ToSql).collect();

        let mut stmt = self.conn().prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
        })?;

        let mut scores = HashMap::new();
        for row in rows {
            let (id, score) = row?;
            scores.insert(id, score);
        }
        Ok(scores)
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test test_get_reference_scores_batch 2>&1 | tail -20`
Expected: PASS

**Step 5: Commit**

```bash
git add src/database/symbols/queries.rs src/tests/database/migration_009_tests.rs
git commit -m "feat: add batch get_reference_scores query"
```

---

### Task 5: Integrate Centrality Boost into Search Scoring

**Files:**
- Modify: `src/search/scoring.rs` (add apply_centrality_boost function)
- Modify: `src/search/index.rs:293-296` (call centrality boost after pattern boost)
- Test: `src/tests/tools/search/tantivy_integration_tests.rs` (new test)

**Step 1: Write the failing test**

Append to `src/tests/tools/search/tantivy_integration_tests.rs`:

```rust
#[test]
fn test_centrality_boost_reranks_results() {
    // Two symbols with similar text relevance but different reference_scores.
    // The one with higher reference_score should rank higher after centrality boost.
    let temp_dir = TempDir::new().unwrap();

    // Create database with symbols and reference_scores
    let db = SymbolDatabase::new(temp_dir.path().join("test.db")).unwrap();
    // ... setup files table ...
    // Insert symbol "process_data" with reference_score = 50.0
    // Insert symbol "process_items" with reference_score = 1.0
    // Both match query "process" equally by text

    // Create Tantivy index with both symbols
    let index = SearchIndex::create(temp_dir.path().join("tantivy")).unwrap();
    // ... add both symbols to index ...

    let mut results = index
        .search_symbols("process", &SearchFilter::default(), 10)
        .unwrap();

    // Before centrality boost, order is arbitrary (both match "process" similarly)
    let scores = db.get_reference_scores(
        &results.iter().map(|r| r.id.as_str()).collect::<Vec<_>>()
    ).unwrap();

    apply_centrality_boost(&mut results, &scores);

    // After boost, process_data (score=50) should rank above process_items (score=1)
    assert_eq!(results[0].name, "process_data",
        "Higher reference_score should rank higher");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_centrality_boost_reranks 2>&1 | tail -20`
Expected: FAIL — `apply_centrality_boost` does not exist

**Step 3: Implement apply_centrality_boost**

In `src/search/scoring.rs`:

```rust
use std::collections::HashMap;

/// Weight for centrality boost in search scoring.
/// Higher = centrality matters more relative to text relevance.
/// At 0.3: score=50 refs gets ~2.2x boost, score=5 gets ~1.5x.
pub const CENTRALITY_WEIGHT: f32 = 0.3;

/// Apply graph centrality boost to search results, then re-sort.
///
/// Uses logarithmic scaling to prevent utility functions from dominating.
/// Formula: boosted = score * (1.0 + ln(1 + reference_score) * CENTRALITY_WEIGHT)
pub fn apply_centrality_boost(
    results: &mut Vec<SymbolSearchResult>,
    reference_scores: &HashMap<String, f64>,
) {
    for result in results.iter_mut() {
        if let Some(&ref_score) = reference_scores.get(&result.id) {
            if ref_score > 0.0 {
                let boost = 1.0 + (1.0 + ref_score as f32).ln() * CENTRALITY_WEIGHT;
                result.score *= boost;
            }
        }
    }
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
}
```

**Step 4: Hook into search_symbols**

In `src/search/index.rs`, the `search_symbols` method needs access to the database to fetch reference scores. This is a design decision point — the SearchIndex currently doesn't hold a database reference.

Options:
1. Pass `&SymbolDatabase` into `search_symbols` (changes the API)
2. Do the centrality boost in the caller (`text_search_impl` in `src/tools/search/text_search.rs`) which already has access to both
3. Return results from search_symbols and let the tool layer apply centrality

**Recommended: Option 2** — apply centrality boost in `text_search_impl`, keeping SearchIndex pure (Tantivy-only). This matches the principle of tools composing primitives.

In `src/tools/search/text_search.rs`, after getting search results and before returning:

```rust
// Apply centrality boost if database is available
let symbol_ids: Vec<&str> = results.iter().map(|r| r.id.as_str()).collect();
if let Ok(ref_scores) = db.get_reference_scores(&symbol_ids) {
    apply_centrality_boost(&mut search_results, &ref_scores);
}
```

**Step 5: Run test to verify it passes**

Run: `cargo test test_centrality_boost_reranks 2>&1 | tail -20`
Expected: PASS

**Step 6: Run full test suite**

Run: `cargo test 2>&1 | tail -20`
Expected: All pass

**Step 7: Commit**

```bash
git add src/search/scoring.rs src/search/index.rs src/tools/search/text_search.rs src/tests/tools/search/tantivy_integration_tests.rs
git commit -m "feat: apply graph centrality boost to search ranking"
```

---

### Task 6: Tuning and Verification

**Step 1: Build debug binary**

Run: `cargo build 2>&1 | tail -5`

**Step 2: Restart and test**

After restarting with the new build, verify with real queries:

- Search for `"process"` — well-connected functions like `process_files_optimized` should rank above isolated helpers
- Search for `"search"` — `search_symbols` (called by many tools) should rank high
- Search for `"extract"` — `extract_symbols` (called by all language extractors) should rank high

**Step 3: Check reference_score distribution**

Query the database to understand the score distribution:
```sql
SELECT name, kind, reference_score FROM symbols ORDER BY reference_score DESC LIMIT 20;
```

Verify that:
- High scores → genuinely important symbols (public APIs, heavily-used functions)
- Low scores → leaf nodes, private helpers
- Zero scores → unused or test-only symbols

**Step 4: Adjust CENTRALITY_WEIGHT if needed**

If centrality dominates too much (utility functions always on top), lower the weight.
If centrality has no visible effect, increase it.

**Step 5: Commit any tuning changes**

```bash
git commit -m "tune: adjust CENTRALITY_WEIGHT based on real-world testing"
```
