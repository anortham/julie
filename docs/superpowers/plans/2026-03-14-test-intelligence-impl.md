# Test Intelligence Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Per-symbol test coverage linkage and change risk scores, surfaced in `deep_dive` and `get_context` output.

**Architecture:** Two new analysis modules (`test_coverage.rs`, `change_risk.rs`) run post-indexing after existing `compute_test_quality_metrics()`. They read from existing relationships/identifiers/symbols tables and write enrichment into the `metadata` JSON column. Tool formatting reads the metadata at display time.

**Tech Stack:** Rust, SQLite (json_extract), serde_json, existing analysis pipeline pattern

**Spec:** `docs/superpowers/specs/2026-03-14-test-intelligence-design.md`

---

## File Structure

| File | Responsibility | Change |
|------|---------------|--------|
| `src/analysis/mod.rs` | Analysis module root | Add `pub mod test_coverage; pub mod change_risk;` + re-exports |
| `src/analysis/test_coverage.rs` | **NEW** — Test-to-code linkage computation | ~200 lines |
| `src/analysis/change_risk.rs` | **NEW** — Change risk score computation | ~150 lines |
| `src/tools/workspace/indexing/processor.rs` | Indexing pipeline | Hook two new functions at ~line 520 |
| `src/tools/deep_dive/formatting.rs` | Deep dive output | Add `format_change_risk_info()` |
| `src/tools/get_context/formatting.rs` | Get context output | Add `risk_label` field to `PivotEntry`, append to pivot lines |
| `src/tools/get_context/pipeline.rs` | Get context pipeline | Extract `risk_label` from metadata when building `PivotEntry` |
| `src/tests/analysis/mod.rs` | Test module declarations | Add `pub mod test_coverage_tests; pub mod change_risk_tests;` |
| `src/tests/analysis/test_coverage_tests.rs` | **NEW** — Linkage computation tests | ~300 lines |
| `src/tests/analysis/change_risk_tests.rs` | **NEW** — Risk scoring tests | ~250 lines |

---

## Chunk 1: Analysis Modules (Layer C + Layer D)

### Task 1: Test coverage types and unit tests

**Files:**
- Create: `src/analysis/test_coverage.rs`
- Modify: `src/analysis/mod.rs:7-9`
- Create: `src/tests/analysis/test_coverage_tests.rs`
- Modify: `src/tests/analysis/mod.rs:1`

- [ ] **Step 1: Declare the module**

In `src/analysis/mod.rs`, add after line 7 (`pub mod test_quality;`):

```rust
pub mod test_coverage;
```

Add re-export after line 9 (`pub use test_quality::compute_test_quality_metrics;`):

```rust
pub use test_coverage::compute_test_coverage;
```

In `src/tests/analysis/mod.rs`, add after line 1 (`pub mod test_quality_tests;`):

```rust
pub mod test_coverage_tests;
```

- [ ] **Step 2: Create test_coverage.rs with types**

Create `src/analysis/test_coverage.rs`:

```rust
//! Test-to-code linkage: determines which test symbols exercise each
//! production symbol and aggregates their quality tiers.
//!
//! Uses two data sources:
//! 1. Relationships — direct test→production edges (high confidence)
//! 2. Identifiers — test file references to production symbols (medium confidence)

use anyhow::Result;
use std::collections::{HashMap, HashSet};
use tracing::{debug, info, warn};

use crate::database::SymbolDatabase;

/// Per-symbol test coverage data, stored in metadata["test_coverage"].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TestCoverageInfo {
    pub test_count: usize,
    pub best_tier: String,
    pub worst_tier: String,
    pub covering_tests: Vec<String>,
}

/// Summary stats from running test coverage analysis.
#[derive(Debug, Clone, Default)]
pub struct TestCoverageStats {
    pub symbols_covered: usize,
    pub total_linkages: usize,
}
```

- [ ] **Step 3: Write failing tests for tier ordering helper**

Create `src/tests/analysis/test_coverage_tests.rs`:

```rust
//! Tests for test-to-code linkage computation.

#[cfg(test)]
mod tests {
    use crate::analysis::test_coverage::{tier_rank, TestCoverageInfo};

    #[test]
    fn test_tier_rank_ordering() {
        assert!(tier_rank("thorough") > tier_rank("adequate"));
        assert!(tier_rank("adequate") > tier_rank("thin"));
        assert!(tier_rank("thin") > tier_rank("stub"));
        assert_eq!(tier_rank("unknown"), 0);
    }

    #[test]
    fn test_tier_best_worst() {
        // "thorough" should be best, "stub" should be worst
        let tiers = vec!["thin", "thorough", "stub"];
        let best = tiers.iter().max_by_key(|t| tier_rank(t)).unwrap();
        let worst = tiers.iter().min_by_key(|t| tier_rank(t)).unwrap();
        assert_eq!(*best, "thorough");
        assert_eq!(*worst, "stub");
    }
}
```

- [ ] **Step 4: Run tests to verify they fail**

Run: `cargo test --lib tests::analysis::test_coverage_tests`
Expected: FAIL — `tier_rank` not found.

- [ ] **Step 5: Implement tier_rank helper**

Add to `src/analysis/test_coverage.rs`:

```rust
/// Rank quality tiers for comparison (higher = better).
pub fn tier_rank(tier: &str) -> u8 {
    match tier {
        "thorough" => 4,
        "adequate" => 3,
        "thin" => 2,
        "stub" => 1,
        _ => 0,
    }
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --lib tests::analysis::test_coverage_tests`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add src/analysis/test_coverage.rs src/analysis/mod.rs \
        src/tests/analysis/test_coverage_tests.rs src/tests/analysis/mod.rs
git commit -m "feat(analysis): add test_coverage module with types and tier_rank helper"
```

---

### Task 2: Test coverage computation (compute_test_coverage)

**Files:**
- Modify: `src/analysis/test_coverage.rs`
- Modify: `src/tests/analysis/test_coverage_tests.rs`

- [ ] **Step 1: Write failing test for relationship-based linkage**

Add to `src/tests/analysis/test_coverage_tests.rs`:

```rust
    use crate::database::SymbolDatabase;
    use tempfile::TempDir;

    /// Insert a file record (required by foreign key constraint on symbols.file_path).
    fn insert_file(db: &SymbolDatabase, path: &str) {
        db.conn.execute(
            "INSERT OR IGNORE INTO files (path, language, hash, size, last_modified) VALUES (?1, 'rust', 'h', 100, 0)",
            rusqlite::params![path],
        ).unwrap();
    }

    /// Create a minimal database with test and production symbols + relationships.
    fn setup_test_db() -> (TempDir, SymbolDatabase) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        // Insert file records (FK constraint)
        insert_file(&db, "src/payments.rs");
        insert_file(&db, "src/tests/payments.rs");

        db.conn.execute_batch(r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score, visibility)
            VALUES ('prod_1', 'process_payment', 'function', 'rust', 'src/payments.rs', 10, 0, 30, 0, 0, 0, NULL, 5.0, 'public');

            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score, visibility)
            VALUES ('test_1', 'test_process_payment', 'function', 'rust', 'src/tests/payments.rs', 5, 0, 20, 0, 0, 0,
                    '{"is_test": true, "test_quality": {"quality_tier": "thorough", "assertion_count": 3}}', 0.0, 'private');

            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score, visibility)
            VALUES ('test_2', 'test_payment_edge_case', 'function', 'rust', 'src/tests/payments.rs', 25, 0, 40, 0, 0, 0,
                    '{"is_test": true, "test_quality": {"quality_tier": "thin", "assertion_count": 1}}', 0.0, 'private');

            INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind, file_path, line_number)
            VALUES ('rel_1', 'test_1', 'prod_1', 'calls', 'src/tests/payments.rs', 10);

            INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind, file_path, line_number)
            VALUES ('rel_2', 'test_2', 'prod_1', 'calls', 'src/tests/payments.rs', 30);
        "#).unwrap();

        (temp_dir, db)
    }

    #[test]
    fn test_compute_coverage_relationship_linkage() {
        let (_temp, db) = setup_test_db();
        let stats = crate::analysis::test_coverage::compute_test_coverage(&db).unwrap();

        assert_eq!(stats.symbols_covered, 1, "One production symbol should be covered");
        assert!(stats.total_linkages >= 2, "Two test→prod relationships");

        // Verify metadata was written
        let prod = db.get_symbol_by_id("prod_1").unwrap().unwrap();
        let meta = prod.metadata.unwrap();
        let coverage = meta.get("test_coverage").unwrap();
        let test_count = coverage.get("test_count").unwrap().as_u64().unwrap();
        assert_eq!(test_count, 2);
        let best = coverage.get("best_tier").unwrap().as_str().unwrap();
        assert_eq!(best, "thorough");
        let worst = coverage.get("worst_tier").unwrap().as_str().unwrap();
        assert_eq!(worst, "thin");
    }

    #[test]
    fn test_identifier_only_linkage() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        insert_file(&db, "src/utils.rs");
        insert_file(&db, "tests/utils_test.rs");

        // Production symbol — no relationship edges to it
        db.conn.execute_batch(r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score, visibility)
            VALUES ('prod_u', 'validate_input', 'function', 'rust', 'src/utils.rs', 1, 0, 10, 0, 0, 0, NULL, 2.0, 'public');

            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score)
            VALUES ('test_u', 'test_validate', 'function', 'rust', 'tests/utils_test.rs', 1, 0, 5, 0, 0, 0,
                    '{"is_test": true, "test_quality": {"quality_tier": "adequate"}}', 0.0);

            INSERT INTO identifiers (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, containing_symbol_id, target_symbol_id)
            VALUES ('id_u', 'validate_input', 'call', 'rust', 'tests/utils_test.rs', 3, 0, 3, 20, 'test_u', 'prod_u');
        "#).unwrap();

        let stats = crate::analysis::test_coverage::compute_test_coverage(&db).unwrap();
        assert_eq!(stats.symbols_covered, 1, "Identifier-only linkage should create coverage");

        let prod = db.get_symbol_by_id("prod_u").unwrap().unwrap();
        let meta = prod.metadata.unwrap();
        let coverage = meta.get("test_coverage").unwrap();
        assert_eq!(coverage.get("test_count").unwrap().as_u64().unwrap(), 1);
        assert_eq!(coverage.get("best_tier").unwrap().as_str().unwrap(), "adequate");
    }

    #[test]
    fn test_uncovered_symbol_has_no_test_coverage_key() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        insert_file(&db, "src/lib.rs");

        db.conn.execute_batch(r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score)
            VALUES ('lonely', 'lonely_function', 'function', 'rust', 'src/lib.rs', 1, 0, 5, 0, 0, 0, NULL, 0.0);
        "#).unwrap();

        let _stats = crate::analysis::test_coverage::compute_test_coverage(&db).unwrap();

        let sym = db.get_symbol_by_id("lonely").unwrap().unwrap();
        if let Some(meta) = &sym.metadata {
            assert!(meta.get("test_coverage").is_none(), "Uncovered symbol should not have test_coverage key");
        }
    }

    #[test]
    fn test_test_to_test_relationships_excluded() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        insert_file(&db, "tests/a.rs");
        insert_file(&db, "tests/b.rs");

        db.conn.execute_batch(r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata)
            VALUES ('t1', 'test_a', 'function', 'rust', 'tests/a.rs', 1, 0, 5, 0, 0, 0, '{"is_test": true, "test_quality": {"quality_tier": "thin"}}');
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata)
            VALUES ('t2', 'test_b', 'function', 'rust', 'tests/b.rs', 1, 0, 5, 0, 0, 0, '{"is_test": true, "test_quality": {"quality_tier": "thin"}}');
            INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind, file_path, line_number)
            VALUES ('r1', 't1', 't2', 'calls', 'tests/a.rs', 3);
        "#).unwrap();

        let stats = crate::analysis::test_coverage::compute_test_coverage(&db).unwrap();
        assert_eq!(stats.symbols_covered, 0, "Test-to-test calls should not create coverage");
    }

    #[test]
    fn test_covering_tests_capped_at_five() {
        let (_temp, db) = setup_test_db();

        insert_file(&db, "tests/extra.rs");

        // Add 5 more test symbols → 7 total tests for prod_1
        for i in 3..=7 {
            db.conn.execute(&format!(
                "INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata)
                 VALUES ('test_{}', 'test_extra_{}', 'function', 'rust', 'tests/extra.rs', {}, 0, {}, 0, 0, 0,
                         '{{\"is_test\": true, \"test_quality\": {{\"quality_tier\": \"adequate\"}}}}')",
                i, i, i * 10, i * 10 + 5
            ), []).unwrap();
            db.conn.execute(&format!(
                "INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind, file_path, line_number)
                 VALUES ('rel_{}', 'test_{}', 'prod_1', 'calls', 'tests/extra.rs', {})",
                i, i, i * 10
            ), []).unwrap();
        }

        let _stats = crate::analysis::test_coverage::compute_test_coverage(&db).unwrap();
        let prod = db.get_symbol_by_id("prod_1").unwrap().unwrap();
        let meta = prod.metadata.unwrap();
        let coverage = meta.get("test_coverage").unwrap();
        let names = coverage.get("covering_tests").unwrap().as_array().unwrap();
        assert!(names.len() <= 5, "covering_tests should be capped at 5, got {}", names.len());
        let count = coverage.get("test_count").unwrap().as_u64().unwrap();
        assert_eq!(count, 7, "test_count should reflect all 7 tests even though names are capped");
    }

    #[test]
    fn test_deduplication_across_strategies() {
        let (_temp, db) = setup_test_db();

        // Add an identifier that links test_1 → prod_1 (same linkage as the relationship)
        db.conn.execute(
            "INSERT INTO identifiers (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, containing_symbol_id, target_symbol_id)
             VALUES ('ident_1', 'process_payment', 'call', 'rust', 'src/tests/payments.rs', 12, 0, 12, 20, 'test_1', 'prod_1')",
            [],
        ).unwrap();

        let _stats = crate::analysis::test_coverage::compute_test_coverage(&db).unwrap();
        let prod = db.get_symbol_by_id("prod_1").unwrap().unwrap();
        let meta = prod.metadata.unwrap();
        let coverage = meta.get("test_coverage").unwrap();
        let count = coverage.get("test_count").unwrap().as_u64().unwrap();
        assert_eq!(count, 2, "Duplicate test_1→prod_1 from identifier should be deduped");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib tests::analysis::test_coverage_tests`
Expected: FAIL — `compute_test_coverage` not implemented.

- [ ] **Step 3: Implement compute_test_coverage**

Add to `src/analysis/test_coverage.rs`:

```rust
/// Compute test-to-code linkage for all production symbols.
///
/// Runs after `compute_test_quality_metrics()` in the indexing pipeline.
/// Reads relationships and identifiers to find test→production edges,
/// then aggregates coverage data into each production symbol's metadata.
pub fn compute_test_coverage(db: &SymbolDatabase) -> Result<TestCoverageStats> {
    let mut stats = TestCoverageStats::default();

    // Step 1: Relationship-based linkage (high confidence)
    let mut linkages: HashMap<String, HashSet<(String, String, String)>> = HashMap::new();
    // Maps prod_id → set of (test_id, test_name, quality_tier)

    let mut stmt = db.conn.prepare(
        "SELECT r.to_symbol_id, s_test.id, s_test.name,
                COALESCE(json_extract(s_test.metadata, '$.test_quality.quality_tier'), 'unknown')
         FROM relationships r
         JOIN symbols s_test ON r.from_symbol_id = s_test.id
         JOIN symbols s_prod ON r.to_symbol_id = s_prod.id
         WHERE json_extract(s_test.metadata, '$.is_test') = 1
           AND (json_extract(s_prod.metadata, '$.is_test') IS NULL
                OR json_extract(s_prod.metadata, '$.is_test') != 1)
           AND r.kind IN ('calls', 'uses', 'references', 'instantiates', 'imports')"
    )?;

    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;

    for row in rows {
        let (prod_id, test_id, test_name, tier) = row?;
        linkages.entry(prod_id).or_default().insert((test_id, test_name, tier));
    }

    debug!("Step 1 (relationships): {} production symbols linked", linkages.len());

    // Step 2: Identifier-based linkage — precise (target_symbol_id set)
    let mut stmt2 = db.conn.prepare(
        "SELECT i.target_symbol_id, s_test.id, s_test.name,
                COALESCE(json_extract(s_test.metadata, '$.test_quality.quality_tier'), 'unknown')
         FROM identifiers i
         JOIN symbols s_test ON i.containing_symbol_id = s_test.id
         JOIN symbols s_prod ON i.target_symbol_id = s_prod.id
         WHERE json_extract(s_test.metadata, '$.is_test') = 1
           AND i.target_symbol_id IS NOT NULL
           AND (json_extract(s_prod.metadata, '$.is_test') IS NULL
                OR json_extract(s_prod.metadata, '$.is_test') != 1)"
    )?;

    let rows2 = stmt2.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;

    for row in rows2 {
        let (prod_id, test_id, test_name, tier) = row?;
        linkages.entry(prod_id).or_default().insert((test_id, test_name, tier));
    }

    // Step 2b: Identifier-based linkage — name-match fallback
    // For identifiers without target_symbol_id, match by name against production symbols.
    // Disambiguate by file proximity (same directory tree preferred).
    // Group by (test_id, identifier_name) so each identifier reference picks
    // the closest matching production symbol independently.
    let mut stmt3 = db.conn.prepare(
        "SELECT s_prod.id, s_prod.file_path, s_test.id, s_test.name, i.file_path AS test_file,
                COALESCE(json_extract(s_test.metadata, '$.test_quality.quality_tier'), 'unknown'),
                i.name AS ident_name
         FROM identifiers i
         JOIN symbols s_test ON i.containing_symbol_id = s_test.id
         JOIN symbols s_prod ON s_prod.name = i.name
         WHERE json_extract(s_test.metadata, '$.is_test') = 1
           AND i.target_symbol_id IS NULL
           AND (json_extract(s_prod.metadata, '$.is_test') IS NULL
                OR json_extract(s_prod.metadata, '$.is_test') != 1)
           AND s_prod.kind NOT IN ('import', 'export', 'module', 'namespace')"
    )?;

    // Group by (test_id, identifier_name) → pick best prod match by directory proximity
    // Key is (test_id, ident_name) — NOT (test_id, test_name) — so each identifier
    // reference disambiguates independently.
    let mut name_matches: HashMap<(String, String), Vec<(String, String, String, String, String)>> = HashMap::new();

    let rows3 = stmt3.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,  // prod_id
            row.get::<_, String>(1)?,  // prod_file_path
            row.get::<_, String>(2)?,  // test_id
            row.get::<_, String>(3)?,  // test_name
            row.get::<_, String>(4)?,  // test_file_path
            row.get::<_, String>(5)?,  // tier
            row.get::<_, String>(6)?,  // ident_name
        ))
    })?;

    for row in rows3 {
        let (prod_id, prod_path, test_id, test_name, test_path, tier, ident_name) = row?;
        name_matches
            .entry((test_id.clone(), ident_name))
            .or_default()
            .push((prod_id, prod_path, test_path, tier, test_name));
    }

    // For each (test, identifier_name), pick the production symbol with closest directory
    for ((test_id, _ident_name), candidates) in &name_matches {
        if candidates.is_empty() {
            continue;
        }
        let best = candidates.iter().max_by_key(|(_, prod_path, test_path, _, _)| {
            common_directory_depth(prod_path, test_path)
        });
        if let Some((prod_id, _, _, tier, test_name)) = best {
            linkages.entry(prod_id.clone()).or_default().insert((
                test_id.clone(),
                test_name.clone(),
                tier.clone(),
            ));
        }
    }

    debug!(
        "After identifier linkage: {} production symbols linked",
        linkages.len()
    );

    // Step 3+4: Aggregate and write metadata
    db.conn.execute_batch("BEGIN")?;
    let result = (|| -> Result<()> {
        for (prod_id, tests) in &linkages {
            let test_count = tests.len();
            let tiers: Vec<&str> = tests.iter().map(|(_, _, t)| t.as_str()).collect();
            let best_tier = tiers.iter().max_by_key(|t| tier_rank(t)).unwrap_or(&"unknown");
            let worst_tier = tiers.iter().min_by_key(|t| tier_rank(t)).unwrap_or(&"unknown");
            let mut names: Vec<&str> = tests.iter().map(|(_, n, _)| n.as_str()).collect();
            names.sort();
            names.truncate(5);

            let coverage_info = serde_json::json!({
                "test_count": test_count,
                "best_tier": best_tier,
                "worst_tier": worst_tier,
                "covering_tests": names,
            });

            // Merge into existing metadata
            let existing: Option<String> = db.conn.query_row(
                "SELECT metadata FROM symbols WHERE id = ?1",
                [prod_id],
                |row| row.get(0),
            ).ok().flatten();

            let mut meta = match existing {
                Some(json_str) => serde_json::from_str::<serde_json::Value>(&json_str)
                    .unwrap_or_else(|_| serde_json::json!({})),
                None => serde_json::json!({}),
            };

            meta.as_object_mut()
                .unwrap()
                .insert("test_coverage".to_string(), coverage_info);

            db.conn.execute(
                "UPDATE symbols SET metadata = ?1 WHERE id = ?2",
                rusqlite::params![serde_json::to_string(&meta)?, prod_id],
            )?;

            stats.total_linkages += test_count;
        }
        Ok(())
    })();

    match result {
        Ok(()) => {
            db.conn.execute_batch("COMMIT")?;
        }
        Err(e) => {
            let _ = db.conn.execute_batch("ROLLBACK");
            return Err(e);
        }
    }

    stats.symbols_covered = linkages.len();
    info!(
        "Test coverage computed: {} symbols covered, {} total linkages",
        stats.symbols_covered, stats.total_linkages
    );

    Ok(stats)
}

/// Count shared directory segments between two paths.
fn common_directory_depth(path_a: &str, path_b: &str) -> usize {
    let dirs_a: Vec<&str> = path_a.rsplitn(2, '/').last().unwrap_or("").split('/').collect();
    let dirs_b: Vec<&str> = path_b.rsplitn(2, '/').last().unwrap_or("").split('/').collect();
    dirs_a.iter().zip(dirs_b.iter()).take_while(|(a, b)| a == b).count()
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib tests::analysis::test_coverage_tests`
Expected: All tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/analysis/test_coverage.rs src/tests/analysis/test_coverage_tests.rs
git commit -m "feat(analysis): implement compute_test_coverage with relationship + identifier linkage"
```

---

### Task 3: Change risk types and unit tests

**Files:**
- Create: `src/analysis/change_risk.rs`
- Modify: `src/analysis/mod.rs`
- Create: `src/tests/analysis/change_risk_tests.rs`
- Modify: `src/tests/analysis/mod.rs`

- [ ] **Step 1: Declare the module**

In `src/analysis/mod.rs`, add:

```rust
pub mod change_risk;
pub use change_risk::compute_change_risk_scores;
```

In `src/tests/analysis/mod.rs`, add:

```rust
pub mod change_risk_tests;
```

- [ ] **Step 2: Create change_risk.rs with types and helpers**

Create `src/analysis/change_risk.rs`:

```rust
//! Change risk scoring: per-symbol 0.0–1.0 score representing
//! "how risky is it to change this?" based on centrality, visibility,
//! test coverage quality, and symbol kind.

use anyhow::Result;
use tracing::{debug, info};

use crate::database::SymbolDatabase;
use crate::extractors::SymbolKind;

/// Weights for the change risk formula.
const W_CENTRALITY: f64 = 0.35;
const W_VISIBILITY: f64 = 0.25;
const W_TEST_WEAKNESS: f64 = 0.30;
const W_KIND: f64 = 0.10;

/// Summary stats from running change risk analysis.
#[derive(Debug, Clone, Default)]
pub struct ChangeRiskStats {
    pub total_scored: usize,
    pub high_risk: usize,
    pub medium_risk: usize,
    pub low_risk: usize,
}

/// Map visibility string to 0.0–1.0 score.
pub fn visibility_score(vis: Option<&str>) -> f64 {
    match vis {
        Some("public") => 1.0,
        Some("protected") => 0.5,
        Some("private") => 0.2,
        _ => 0.5, // NULL or unknown → moderate exposure
    }
}

/// Map symbol kind to 0.0–1.0 weight.
/// Returns None for Import/Export (excluded from scoring).
pub fn kind_weight(kind: &SymbolKind) -> Option<f64> {
    match kind {
        // Callable: highest risk surface
        SymbolKind::Function | SymbolKind::Method | SymbolKind::Constructor
        | SymbolKind::Destructor | SymbolKind::Operator => Some(1.0),
        // Container: moderate risk
        SymbolKind::Class | SymbolKind::Struct | SymbolKind::Interface
        | SymbolKind::Trait | SymbolKind::Enum | SymbolKind::Union
        | SymbolKind::Module | SymbolKind::Namespace | SymbolKind::Type
        | SymbolKind::Delegate => Some(0.7),
        // Data: lower risk
        SymbolKind::Variable | SymbolKind::Constant | SymbolKind::Property
        | SymbolKind::Field | SymbolKind::EnumMember | SymbolKind::Event => Some(0.3),
        // Import/Export: skip
        SymbolKind::Import | SymbolKind::Export => None,
    }
}

/// Map test coverage best_tier to a "test weakness" score.
/// Higher = worse coverage = more risk.
pub fn test_weakness_score(best_tier: Option<&str>) -> f64 {
    match best_tier {
        None => 1.0,           // Untested
        Some("stub") => 0.8,
        Some("thin") => 0.6,
        Some("adequate") => 0.3,
        Some("thorough") => 0.1,
        _ => 1.0,              // Unknown tier → treat as untested
    }
}

/// Normalize reference_score to 0.0–1.0 using log sigmoid.
pub fn normalize_centrality(reference_score: f64, p95: f64) -> f64 {
    if p95 <= 0.0 {
        return 0.0;
    }
    let normalized = (1.0 + reference_score).ln() / (1.0 + p95).ln();
    normalized.min(1.0)
}

/// Compute final change risk score from normalized signals.
pub fn compute_risk_score(centrality: f64, visibility: f64, test_weakness: f64, kind: f64) -> f64 {
    W_CENTRALITY * centrality + W_VISIBILITY * visibility + W_TEST_WEAKNESS * test_weakness + W_KIND * kind
}

/// Map score to tier label.
pub fn risk_label(score: f64) -> &'static str {
    if score >= 0.7 { "HIGH" }
    else if score >= 0.4 { "MEDIUM" }
    else { "LOW" }
}
```

- [ ] **Step 3: Write failing tests**

Create `src/tests/analysis/change_risk_tests.rs`:

```rust
//! Tests for change risk scoring.

#[cfg(test)]
mod tests {
    use crate::analysis::change_risk::*;
    use crate::extractors::SymbolKind;

    #[test]
    fn test_visibility_scores() {
        assert_eq!(visibility_score(Some("public")), 1.0);
        assert_eq!(visibility_score(Some("protected")), 0.5);
        assert_eq!(visibility_score(Some("private")), 0.2);
        assert_eq!(visibility_score(None), 0.5); // NULL → moderate
    }

    #[test]
    fn test_kind_weights() {
        assert_eq!(kind_weight(&SymbolKind::Function), Some(1.0));
        assert_eq!(kind_weight(&SymbolKind::Method), Some(1.0));
        assert_eq!(kind_weight(&SymbolKind::Constructor), Some(1.0));
        assert_eq!(kind_weight(&SymbolKind::Class), Some(0.7));
        assert_eq!(kind_weight(&SymbolKind::Struct), Some(0.7));
        assert_eq!(kind_weight(&SymbolKind::Trait), Some(0.7));
        assert_eq!(kind_weight(&SymbolKind::Variable), Some(0.3));
        assert_eq!(kind_weight(&SymbolKind::Constant), Some(0.3));
        assert_eq!(kind_weight(&SymbolKind::Import), None); // Excluded
        assert_eq!(kind_weight(&SymbolKind::Export), None);
    }

    #[test]
    fn test_test_weakness_scores() {
        assert_eq!(test_weakness_score(None), 1.0);        // Untested
        assert_eq!(test_weakness_score(Some("stub")), 0.8);
        assert_eq!(test_weakness_score(Some("thin")), 0.6);
        assert_eq!(test_weakness_score(Some("adequate")), 0.3);
        assert_eq!(test_weakness_score(Some("thorough")), 0.1);
    }

    #[test]
    fn test_normalize_centrality() {
        assert_eq!(normalize_centrality(0.0, 20.0), 0.0);
        // P95=20 → score of 20 should be ~1.0
        let at_p95 = normalize_centrality(20.0, 20.0);
        assert!((at_p95 - 1.0).abs() < 0.01, "Score at P95 should be ~1.0, got {}", at_p95);
        // Score above P95 should be capped at 1.0
        assert_eq!(normalize_centrality(100.0, 20.0), 1.0);
        // P95=0 → everything is 0
        assert_eq!(normalize_centrality(5.0, 0.0), 0.0);
    }

    #[test]
    fn test_risk_labels() {
        assert_eq!(risk_label(0.85), "HIGH");
        assert_eq!(risk_label(0.70), "HIGH");
        assert_eq!(risk_label(0.55), "MEDIUM");
        assert_eq!(risk_label(0.40), "MEDIUM");
        assert_eq!(risk_label(0.39), "LOW");
        assert_eq!(risk_label(0.0), "LOW");
    }

    #[test]
    fn test_high_risk_scenario() {
        // Public function, high centrality, untested → HIGH risk
        let score = compute_risk_score(0.95, 1.0, 1.0, 1.0);
        assert!(score >= 0.7, "Public untested function with high centrality should be HIGH, got {:.2}", score);
        assert_eq!(risk_label(score), "HIGH");
    }

    #[test]
    fn test_low_risk_scenario() {
        // Private constant, no centrality, thoroughly tested → LOW risk
        let score = compute_risk_score(0.0, 0.2, 0.1, 0.3);
        assert!(score < 0.4, "Private tested constant should be LOW, got {:.2}", score);
        assert_eq!(risk_label(score), "LOW");
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib tests::analysis::change_risk_tests`
Expected: All tests PASS (these test helpers, not the full compute function yet).

- [ ] **Step 5: Commit**

```bash
git add src/analysis/change_risk.rs src/analysis/mod.rs \
        src/tests/analysis/change_risk_tests.rs src/tests/analysis/mod.rs
git commit -m "feat(analysis): add change_risk module with scoring helpers and tests"
```

---

### Task 4: Change risk computation (compute_change_risk_scores)

**Files:**
- Modify: `src/analysis/change_risk.rs`
- Modify: `src/tests/analysis/change_risk_tests.rs`

- [ ] **Step 1: Write failing test for full computation**

Add to `src/tests/analysis/change_risk_tests.rs`:

```rust
    use crate::database::SymbolDatabase;
    use tempfile::TempDir;

    fn insert_file(db: &SymbolDatabase, path: &str) {
        db.conn.execute(
            "INSERT OR IGNORE INTO files (path, language, hash, size, last_modified) VALUES (?1, 'rust', 'h', 100, 0)",
            rusqlite::params![path],
        ).unwrap();
    }

    #[test]
    fn test_compute_change_risk_scores() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        insert_file(&db, "src/core.rs");
        insert_file(&db, "src/config.rs");
        insert_file(&db, "tests/test.rs");
        insert_file(&db, "src/lib.rs");

        // High-risk: public function, high centrality, untested
        db.conn.execute_batch(r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, reference_score, visibility, metadata)
            VALUES ('s1', 'important_func', 'function', 'rust', 'src/core.rs', 1, 0, 10, 0, 0, 0, 20.0, 'public', NULL);
        "#).unwrap();

        // Low-risk: private constant, no centrality, thoroughly tested
        db.conn.execute_batch(r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, reference_score, visibility, metadata)
            VALUES ('s2', 'MY_CONST', 'constant', 'rust', 'src/config.rs', 1, 0, 1, 0, 0, 0, 0.0, 'private',
                    '{"test_coverage": {"test_count": 2, "best_tier": "thorough", "worst_tier": "adequate", "covering_tests": ["test_a", "test_b"]}}');
        "#).unwrap();

        // Test symbol — should be excluded from risk scoring
        db.conn.execute_batch(r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, reference_score, metadata)
            VALUES ('t1', 'test_thing', 'function', 'rust', 'tests/test.rs', 1, 0, 5, 0, 0, 0, 0.0, '{"is_test": true}');
        "#).unwrap();

        // Import — should be excluded (kind_weight returns None)
        db.conn.execute_batch(r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, reference_score, metadata)
            VALUES ('imp', 'use_thing', 'import', 'rust', 'src/lib.rs', 1, 0, 1, 0, 0, 0, 0.0, NULL);
        "#).unwrap();

        let stats = crate::analysis::change_risk::compute_change_risk_scores(&db).unwrap();
        assert_eq!(stats.total_scored, 2, "Should score s1 and s2 only (not test or import)");
        assert!(stats.high_risk >= 1, "important_func should be HIGH risk");

        // Verify s1 metadata
        let s1 = db.get_symbol_by_id("s1").unwrap().unwrap();
        let meta = s1.metadata.unwrap();
        let risk = meta.get("change_risk").unwrap();
        let label = risk.get("label").unwrap().as_str().unwrap();
        assert_eq!(label, "HIGH");

        // Verify s2 is LOW risk
        let s2 = db.get_symbol_by_id("s2").unwrap().unwrap();
        let meta2 = s2.metadata.unwrap();
        let risk2 = meta2.get("change_risk").unwrap();
        let label2 = risk2.get("label").unwrap().as_str().unwrap();
        assert_eq!(label2, "LOW");

        // Verify test symbol has no change_risk
        let t1 = db.get_symbol_by_id("t1").unwrap().unwrap();
        if let Some(meta_t) = &t1.metadata {
            assert!(meta_t.get("change_risk").is_none(), "Test symbols should not have change_risk");
        }
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib tests::analysis::change_risk_tests::tests::test_compute_change_risk_scores`
Expected: FAIL — `compute_change_risk_scores` not implemented.

- [ ] **Step 3: Implement compute_change_risk_scores**

Add to `src/analysis/change_risk.rs`:

```rust
/// Compute change risk scores for all non-test, non-import/export symbols.
///
/// Must run AFTER `compute_test_coverage()` so that `metadata["test_coverage"]`
/// is available for the test weakness signal.
pub fn compute_change_risk_scores(db: &SymbolDatabase) -> Result<ChangeRiskStats> {
    let mut stats = ChangeRiskStats::default();

    // Compute P95 of reference_score for centrality normalization
    let p95: f64 = db.conn.query_row(
        "SELECT COALESCE(
            (SELECT reference_score FROM symbols
             WHERE reference_score > 0
             ORDER BY reference_score DESC
             LIMIT 1 OFFSET (SELECT MAX(0, CAST(COUNT(*) * 0.05 AS INTEGER))
                             FROM symbols WHERE reference_score > 0)),
            0.0)",
        [],
        |row| row.get(0),
    ).unwrap_or(0.0);

    debug!("Change risk P95 reference_score: {:.2}", p95);

    // Query all non-test symbols with their scoring inputs
    let mut stmt = db.conn.prepare(
        "SELECT id, kind, visibility, reference_score, metadata
         FROM symbols
         WHERE (json_extract(metadata, '$.is_test') IS NULL
                OR json_extract(metadata, '$.is_test') != 1)"
    )?;

    let rows: Vec<(String, String, Option<String>, f64, Option<String>)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, f64>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();

    db.conn.execute_batch("BEGIN")?;
    let result = (|| -> Result<()> {
        for (id, kind_str, vis, ref_score, metadata_json) in &rows {
            let kind = SymbolKind::from_string(kind_str);

            // Skip imports/exports
            let kw = match kind_weight(&kind) {
                Some(w) => w,
                None => continue,
            };

            let centrality = normalize_centrality(*ref_score, p95);
            let vis_score = visibility_score(vis.as_deref());

            // Extract test weakness from metadata["test_coverage"]["best_tier"]
            let best_tier = metadata_json.as_ref().and_then(|json| {
                serde_json::from_str::<serde_json::Value>(json).ok()
            }).and_then(|v| {
                v.get("test_coverage")?.get("best_tier")?.as_str().map(String::from)
            });
            let tw = test_weakness_score(best_tier.as_deref());

            let score = compute_risk_score(centrality, vis_score, tw, kw);
            let label = risk_label(score);

            stats.total_scored += 1;
            match label {
                "HIGH" => stats.high_risk += 1,
                "MEDIUM" => stats.medium_risk += 1,
                _ => stats.low_risk += 1,
            }

            let risk_data = serde_json::json!({
                "score": (score * 100.0).round() / 100.0,
                "label": label,
                "factors": {
                    "centrality": (centrality * 100.0).round() / 100.0,
                    "visibility": vis.as_deref().unwrap_or("unknown"),
                    "test_weakness": tw,
                    "kind": kind_str,
                }
            });

            // Merge into existing metadata
            let mut meta = match metadata_json {
                Some(json_str) => serde_json::from_str::<serde_json::Value>(json_str)
                    .unwrap_or_else(|_| serde_json::json!({})),
                None => serde_json::json!({}),
            };

            meta.as_object_mut()
                .unwrap()
                .insert("change_risk".to_string(), risk_data);

            db.conn.execute(
                "UPDATE symbols SET metadata = ?1 WHERE id = ?2",
                rusqlite::params![serde_json::to_string(&meta)?, id],
            )?;
        }
        Ok(())
    })();

    match result {
        Ok(()) => {
            db.conn.execute_batch("COMMIT")?;
        }
        Err(e) => {
            let _ = db.conn.execute_batch("ROLLBACK");
            return Err(e);
        }
    }

    info!(
        "Change risk computed: {} scored ({} HIGH, {} MEDIUM, {} LOW)",
        stats.total_scored, stats.high_risk, stats.medium_risk, stats.low_risk
    );

    Ok(stats)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib tests::analysis::change_risk_tests`
Expected: All tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/analysis/change_risk.rs src/tests/analysis/change_risk_tests.rs
git commit -m "feat(analysis): implement compute_change_risk_scores with P95 normalization"
```

---

### Task 5: Hook into indexing pipeline

**Files:**
- Modify: `src/tools/workspace/indexing/processor.rs:518-520`

- [ ] **Step 1: Add pipeline calls**

In `src/tools/workspace/indexing/processor.rs`, after the `compute_test_quality_metrics` call (~line 518), add:

```rust
                // Compute test-to-code coverage linkage
                if let Err(e) = crate::analysis::compute_test_coverage(&db_lock) {
                    warn!("Failed to compute test coverage: {}", e);
                }

                // Compute change risk scores
                if let Err(e) = crate::analysis::compute_change_risk_scores(&db_lock) {
                    warn!("Failed to compute change risk scores: {}", e);
                }
```

- [ ] **Step 2: Build to verify compilation**

Run: `cargo build`
Expected: Clean compile.

- [ ] **Step 3: Run dev tier tests**

Run: `cargo xtask test dev`
Expected: No new failures beyond known pre-existing ones.

- [ ] **Step 4: Commit**

```bash
git add src/tools/workspace/indexing/processor.rs
git commit -m "feat(analysis): hook test_coverage and change_risk into indexing pipeline"
```

---

## Chunk 2: Tool Integration

### Task 6: Deep dive — change risk formatting

**Files:**
- Modify: `src/tools/deep_dive/formatting.rs`

- [ ] **Step 1: Add format_change_risk_info function**

In `src/tools/deep_dive/formatting.rs`, add after the `format_test_quality_info` function (~line 175):

```rust
/// Format change risk section for production symbols.
/// Skipped for test symbols (they have quality tiers instead).
fn format_change_risk_info(out: &mut String, symbol: &crate::extractors::base::Symbol, incoming_count: usize) {
    let metadata = match &symbol.metadata {
        Some(m) => m,
        None => return,
    };

    // Skip test symbols
    if metadata.get("is_test").and_then(|v| v.as_bool()).unwrap_or(false) {
        return;
    }

    let risk = match metadata.get("change_risk") {
        Some(r) => r,
        None => return,
    };

    let score = risk.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let label = risk.get("label").and_then(|v| v.as_str()).unwrap_or("LOW");
    let factors = risk.get("factors");

    let vis = factors
        .and_then(|f| f.get("visibility"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let kind = factors
        .and_then(|f| f.get("kind"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    // Build summary line: "Change Risk: HIGH (0.82) — 14 callers, public, thin tests"
    let coverage = metadata.get("test_coverage");
    let test_summary = match coverage {
        Some(tc) => {
            let count = tc.get("test_count").and_then(|v| v.as_u64()).unwrap_or(0);
            let best = tc.get("best_tier").and_then(|v| v.as_str()).unwrap_or("none");
            if count > 0 {
                format!("{} tests", best)
            } else {
                "untested".to_string()
            }
        }
        None => "untested".to_string(),
    };

    out.push_str(&format!(
        "\nChange Risk: {} ({:.2}) — {} callers, {}, {}\n",
        label, score, incoming_count, vis, test_summary
    ));

    // Detail lines
    if let Some(f) = factors {
        let centrality = f.get("centrality").and_then(|v| v.as_f64()).unwrap_or(0.0);
        out.push_str(&format!("  centrality: {:.2} ({} direct callers)\n", centrality, incoming_count));
        out.push_str(&format!("  visibility: {}\n", vis));

        if let Some(tc) = coverage {
            let count = tc.get("test_count").and_then(|v| v.as_u64()).unwrap_or(0);
            let best = tc.get("best_tier").and_then(|v| v.as_str()).unwrap_or("none");
            let worst = tc.get("worst_tier").and_then(|v| v.as_str()).unwrap_or("none");
            out.push_str(&format!("  test coverage: {} tests (best: {}, worst: {})\n", count, best, worst));
        } else {
            out.push_str("  test coverage: untested\n");
        }

        out.push_str(&format!("  kind: {}\n", kind));
    }
}
```

- [ ] **Step 2: Wire format_change_risk_info into symbol formatting**

Find where `format_test_quality_info(out, s)` is called (~line 68) and add after it:

```rust
    format_change_risk_info(out, s, ctx.incoming_total);
```

Do the same after each `format_test_locations(out, ctx, depth)` call — add the risk info. The function will self-skip for test symbols, so it's safe to call unconditionally.

Check each of the call sites at approximately lines 248, 285, 373, 420, 487, 510. After each `format_test_locations` call, add:

```rust
    format_change_risk_info(out, &ctx.symbol, ctx.incoming_total);
```

- [ ] **Step 3: Build to verify compilation**

Run: `cargo build`
Expected: Clean compile.

- [ ] **Step 4: Commit**

```bash
git add src/tools/deep_dive/formatting.rs
git commit -m "feat(deep_dive): display change risk section with score, label, and factors"
```

---

### Task 7: Get context — risk labels on pivots

**Files:**
- Modify: `src/tools/get_context/formatting.rs:44-61` (PivotEntry struct)
- Modify: `src/tools/get_context/pipeline.rs:365-374` (PivotEntry construction)
- Modify: `src/tools/get_context/formatting.rs:142-145` (pivot line output)

- [ ] **Step 1: Add risk_label field to PivotEntry**

In `src/tools/get_context/formatting.rs`, add to the `PivotEntry` struct (~line 59, after `outgoing_names`):

```rust
    /// Change risk label (HIGH/MEDIUM/LOW) from metadata, if available.
    pub risk_label: Option<String>,
```

- [ ] **Step 2: Extract risk_label in pipeline**

In `src/tools/get_context/pipeline.rs`, where `PivotEntry` is constructed (~line 365), extract the risk label from `batch.full_symbols`:

```rust
                let risk_label = batch.full_symbols.get(&pivot.result.id)
                    .and_then(|sym| sym.metadata.as_ref())
                    .and_then(|m| m.get("change_risk"))
                    .and_then(|r| r.get("label"))
                    .and_then(|l| l.as_str())
                    .map(String::from);

                entries.push(PivotEntry {
                    name: pivot.result.name.clone(),
                    file_path: pivot.result.file_path.clone(),
                    start_line: pivot.result.start_line,
                    kind: pivot.result.kind.clone(),
                    reference_score: ref_score,
                    content,
                    incoming_names,
                    outgoing_names,
                    risk_label,
                });
```

- [ ] **Step 3: Append risk label to pivot output line**

In `src/tools/get_context/formatting.rs`, where pivot lines are formatted (~line 142-145), update to include risk label:

Change from:
```rust
out.push_str(&format!(
    "{}:{} ({})\n",
    pivot.file_path, pivot.start_line, pivot.kind
));
```

To:
```rust
let risk_tag = pivot.risk_label.as_ref()
    .map(|l| format!("  [{} risk]", l))
    .unwrap_or_default();
out.push_str(&format!(
    "{}:{} ({}){}\n",
    pivot.file_path, pivot.start_line, pivot.kind, risk_tag
));
```

- [ ] **Step 4: Build to verify compilation**

Run: `cargo build`
Expected: Clean compile.

- [ ] **Step 5: Run dev tier tests**

Run: `cargo xtask test dev`
Expected: No new failures.

- [ ] **Step 6: Commit**

```bash
git add src/tools/get_context/formatting.rs src/tools/get_context/pipeline.rs
git commit -m "feat(get_context): display risk labels on pivot symbols"
```

---

### Task 8: Final regression check and TODO update

**Files:**
- Modify: `TODO.md`

- [ ] **Step 1: Run full dev tier**

Run: `cargo xtask test dev`
Expected: No new failures beyond known pre-existing ones.

- [ ] **Step 2: Update TODO.md**

Mark Layer C and Layer D items as complete. Update the Phase 2 section to reflect completed work.

- [ ] **Step 3: Commit**

```bash
git add TODO.md
git commit -m "docs: mark test intelligence (Layer C + D) as complete in TODO"
```

---

## Verification

After all tasks complete:

1. **Build release**: `cargo build --release`
2. **Restart Claude Code** to pick up new binary
3. **Dogfood — deep_dive**: `deep_dive(symbol="compute_reference_scores")` → should show Change Risk section with score, label, and factors
4. **Dogfood — get_context**: `get_context(query="search scoring")` → pivots should show `[HIGH risk]` / `[MEDIUM risk]` / `[LOW risk]` labels
5. **Sanity check**: Verify risk labels make intuitive sense — heavily-depended-on public functions should be higher risk than private helpers
6. **Regression**: `cargo xtask test dev` — no new failures
