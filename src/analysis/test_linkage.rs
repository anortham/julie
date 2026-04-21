//! Test-to-code linkage: determines which test symbols exercise each
//! production symbol and aggregates their quality tiers plus evidence sources.
//!
//! Uses two data sources:
//! 1. Relationships — direct test→production edges (high confidence)
//! 2. Identifiers — test file references to production symbols (medium confidence)

use anyhow::Result;
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use tracing::{debug, info};

use crate::database::SymbolDatabase;

#[derive(Debug, Clone, Default)]
struct LinkedTest {
    name: String,
    file_path: String,
    tier: String,
    evidence_sources: HashSet<String>,
}

/// Per-symbol static test linkage data, stored in metadata["test_linkage"].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TestLinkageInfo {
    pub test_count: usize,
    pub best_tier: String,
    pub worst_tier: String,
    pub linked_tests: Vec<String>,
    pub linked_test_paths: Vec<String>,
    pub evidence_sources: Vec<String>,
}

/// Summary stats from running test linkage analysis.
#[derive(Debug, Clone, Default)]
pub struct TestLinkageStats {
    pub symbols_covered: usize,
    pub total_linkages: usize,
}

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

fn add_linkage(
    linkages: &mut HashMap<String, HashMap<String, LinkedTest>>,
    prod_id: String,
    test_id: String,
    test_name: String,
    test_file_path: String,
    tier: String,
    source: &str,
) {
    let entry = linkages
        .entry(prod_id)
        .or_default()
        .entry(test_id)
        .or_insert_with(|| LinkedTest {
            name: test_name.clone(),
            file_path: test_file_path.clone(),
            tier: tier.clone(),
            evidence_sources: HashSet::new(),
        });

    entry.name = test_name;
    entry.file_path = test_file_path;
    entry.tier = tier;
    entry.evidence_sources.insert(source.to_string());
}

pub fn test_linkage_entry<'a>(metadata: &'a serde_json::Value) -> Option<&'a serde_json::Value> {
    metadata
        .get("test_linkage")
        .or_else(|| metadata.get("test_coverage"))
}

/// Compute test-to-code linkage for all production symbols.
///
/// Runs after `compute_test_quality_metrics()` in the indexing pipeline.
/// Reads relationships and identifiers to find test→production edges,
/// then aggregates linkage data into each production symbol's metadata.
pub fn compute_test_linkage(db: &SymbolDatabase) -> Result<TestLinkageStats> {
    let mut stats = TestLinkageStats::default();

    // Step 1: Relationship-based linkage (high confidence)
    let mut linkages: HashMap<String, HashMap<String, LinkedTest>> = HashMap::new();
    // Maps prod_id → test_id → linked test details

    let mut stmt = db.conn.prepare(
        "SELECT r.to_symbol_id, s_test.id, s_test.name, s_test.file_path,
                COALESCE(json_extract(s_test.metadata, '$.test_quality.quality_tier'), 'unknown')
         FROM relationships r
         JOIN symbols s_test ON r.from_symbol_id = s_test.id
         JOIN symbols s_prod ON r.to_symbol_id = s_prod.id
         WHERE json_extract(s_test.metadata, '$.is_test') = 1
           AND (json_extract(s_prod.metadata, '$.is_test') IS NULL
                OR json_extract(s_prod.metadata, '$.is_test') != 1)
           AND r.kind IN ('calls', 'uses', 'references', 'instantiates', 'imports')",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;

    for row in rows {
        let (prod_id, test_id, test_name, test_file_path, tier) = row?;
        add_linkage(
            &mut linkages,
            prod_id,
            test_id,
            test_name,
            test_file_path,
            tier,
            "relationship",
        );
    }

    debug!(
        "Step 1 (relationships): {} production symbols linked",
        linkages.len()
    );

    // Step 2: Identifier-based linkage — precise (target_symbol_id set)
    let mut stmt2 = db.conn.prepare(
        "SELECT i.target_symbol_id, s_test.id, s_test.name, s_test.file_path,
                COALESCE(json_extract(s_test.metadata, '$.test_quality.quality_tier'), 'unknown')
         FROM identifiers i
         JOIN symbols s_test ON i.containing_symbol_id = s_test.id
         JOIN symbols s_prod ON i.target_symbol_id = s_prod.id
         WHERE json_extract(s_test.metadata, '$.is_test') = 1
           AND i.target_symbol_id IS NOT NULL
           AND (json_extract(s_prod.metadata, '$.is_test') IS NULL
                OR json_extract(s_prod.metadata, '$.is_test') != 1)",
    )?;

    let rows2 = stmt2.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;

    for row in rows2 {
        let (prod_id, test_id, test_name, test_file_path, tier) = row?;
        add_linkage(
            &mut linkages,
            prod_id,
            test_id,
            test_name,
            test_file_path,
            tier,
            "resolved_identifier",
        );
    }

    // Step 2b: Identifier-based linkage — name-match fallback
    // For identifiers without target_symbol_id, match by name against production symbols.
    // Disambiguate by file proximity (same directory tree preferred).
    // Group by (test_id, identifier_name) so each identifier reference picks
    // the closest matching production symbol independently.
    // NOTE: Language filter is applied in Rust (not SQL) because adding
    // `AND s_test.language = s_prod.language` to the query causes SQLite's
    // planner to drop idx_symbols_name in favor of idx_symbols_language,
    // turning a fast name-index lookup into a full language-scan + name filter.
    // On Julie's codebase this changed a <1s query into a 3+ minute hang.
    // ORDER BY s_prod.id, s_prod.file_path gives the scoring loop a stable
    // row order so max_by_key sees ties in a deterministic sequence across
    // runs on the same database. Combined with the explicit tie-breaker
    // below, `linked_tests` / `linked_test_paths` / `evidence_sources`
    // become reproducible — no SQLite row-order leakage.
    let mut stmt3 = db.conn.prepare(
        "SELECT s_prod.id, s_prod.file_path, s_test.id, s_test.name, i.file_path AS test_file,
                COALESCE(json_extract(s_test.metadata, '$.test_quality.quality_tier'), 'unknown'),
                i.name AS ident_name,
                s_test.language, s_prod.language
         FROM identifiers i
         JOIN symbols s_test ON i.containing_symbol_id = s_test.id
         JOIN symbols s_prod ON s_prod.name = i.name
         WHERE json_extract(s_test.metadata, '$.is_test') = 1
           AND i.target_symbol_id IS NULL
           AND (json_extract(s_prod.metadata, '$.is_test') IS NULL
                OR json_extract(s_prod.metadata, '$.is_test') != 1)
           AND s_prod.kind NOT IN ('import', 'export', 'module', 'namespace')
         ORDER BY s_prod.id, s_prod.file_path",
    )?;

    // Group by (test_id, identifier_name) → pick best prod match by directory proximity
    // Key is (test_id, ident_name) — NOT (test_id, test_name) — so each identifier
    // reference disambiguates independently.
    let mut name_matches: HashMap<(String, String), Vec<(String, String, String, String, String)>> =
        HashMap::new();

    let rows3 = stmt3.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?, // prod_id
            row.get::<_, String>(1)?, // prod_file_path
            row.get::<_, String>(2)?, // test_id
            row.get::<_, String>(3)?, // test_name
            row.get::<_, String>(4)?, // test_file_path
            row.get::<_, String>(5)?, // tier
            row.get::<_, String>(6)?, // ident_name
            row.get::<_, String>(7)?, // test_language
            row.get::<_, String>(8)?, // prod_language
        ))
    })?;

    for row in rows3 {
        let (
            prod_id,
            prod_path,
            test_id,
            test_name,
            test_path,
            tier,
            ident_name,
            test_lang,
            prod_lang,
        ) = row?;
        // Language filter: skip cross-language matches (e.g. Python test → Rust symbol)
        if test_lang != prod_lang {
            continue;
        }
        name_matches
            .entry((test_id.clone(), ident_name))
            .or_default()
            .push((prod_id, prod_path, test_path, tier, test_name));
    }

    // For each (test, identifier_name), pick the production symbol with closest directory
    // and best file-name similarity (test file name contains prod file stem).
    for ((test_id, _ident_name), candidates) in &name_matches {
        if candidates.is_empty() {
            continue;
        }
        // Tie-breaker: when dir_score + name_bonus is equal across candidates,
        // pick the smallest prod_id lexicographically, then smallest prod_path.
        // Wrapping the id/path in Reverse flips their ordering so tuple-compare
        // inside max_by_key selects the smaller value on ties. Combined with
        // the ORDER BY on stmt3, this guarantees deterministic output across
        // runs on the same database.
        let best = candidates
            .iter()
            .max_by_key(|(prod_id, prod_path, test_path, _, _)| {
                let dir_score = common_directory_depth(prod_path, test_path) * 10;
                let test_file_stem = test_path
                    .rsplit('/')
                    .next()
                    .unwrap_or("")
                    .split('.')
                    .next()
                    .unwrap_or("");
                let prod_file_stem = prod_path
                    .rsplit('/')
                    .next()
                    .unwrap_or("")
                    .split('.')
                    .next()
                    .unwrap_or("");
                let name_bonus =
                    if !prod_file_stem.is_empty() && test_file_stem.contains(prod_file_stem) {
                        100
                    } else {
                        0
                    };
                (
                    dir_score + name_bonus,
                    Reverse(prod_id.clone()),
                    Reverse(prod_path.clone()),
                )
            });
        if let Some((prod_id, _, test_path, tier, test_name)) = best {
            add_linkage(
                &mut linkages,
                prod_id.clone(),
                test_id.clone(),
                test_name.clone(),
                test_path.clone(),
                tier.clone(),
                "name_match_fallback",
            );
        }
    }

    debug!(
        "After identifier linkage: {} production symbols linked",
        linkages.len()
    );

    // Step 3+4: Aggregate and write metadata
    db.conn.execute_batch("BEGIN")?;
    let result = (|| -> Result<()> {
        db.conn.execute(
            "UPDATE symbols
             SET metadata = json_remove(metadata, '$.test_linkage', '$.test_coverage')
             WHERE json_extract(metadata, '$.test_linkage') IS NOT NULL
                OR json_extract(metadata, '$.test_coverage') IS NOT NULL",
            [],
        )?;

        for (prod_id, tests) in &linkages {
            let test_count = tests.len();
            let tiers: Vec<&str> = tests.values().map(|test| test.tier.as_str()).collect();
            let best_tier = tiers
                .iter()
                .max_by_key(|t| tier_rank(t))
                .unwrap_or(&"unknown");
            let worst_tier = tiers
                .iter()
                .min_by_key(|t| tier_rank(t))
                .unwrap_or(&"unknown");
            let mut names: Vec<&str> = tests.values().map(|test| test.name.as_str()).collect();
            names.sort();
            names.dedup();
            names.truncate(5);
            let mut paths: Vec<&str> = tests.values().map(|test| test.file_path.as_str()).collect();
            paths.sort();
            paths.dedup();
            paths.truncate(5);
            let mut evidence_sources: Vec<String> = tests
                .values()
                .flat_map(|test| test.evidence_sources.iter().cloned())
                .collect();
            evidence_sources.sort();
            evidence_sources.dedup();

            let linkage_info = serde_json::json!({
                "test_count": test_count,
                "best_tier": best_tier,
                "worst_tier": worst_tier,
                "linked_tests": names,
                "linked_test_paths": paths,
                "evidence_sources": evidence_sources,
            });

            // Merge into existing metadata
            let existing: Option<String> = db
                .conn
                .query_row(
                    "SELECT metadata FROM symbols WHERE id = ?1",
                    [prod_id],
                    |row| row.get(0),
                )
                .ok()
                .flatten();

            let mut meta = match existing {
                Some(json_str) => serde_json::from_str::<serde_json::Value>(&json_str)
                    .unwrap_or_else(|_| serde_json::json!({})),
                None => serde_json::json!({}),
            };

            if let Some(obj) = meta.as_object_mut() {
                obj.insert("test_linkage".to_string(), linkage_info);
            }

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

    // Step 4: Aggregate method-level linkage to parent classes/structs.
    // Parents that have NO direct test_linkage but have children WITH test_linkage
    // inherit aggregated stats from their children.
    let mut parent_stmt = db.conn.prepare(
        "SELECT parent.id,
                json_extract(child.metadata, '$.test_linkage.test_count'),
                json_extract(child.metadata, '$.test_linkage.best_tier'),
                json_extract(child.metadata, '$.test_linkage.worst_tier')
         FROM symbols parent
         JOIN symbols child ON child.parent_id = parent.id
         WHERE parent.kind IN ('class', 'struct', 'interface', 'enum', 'trait')
           AND json_extract(child.metadata, '$.test_linkage') IS NOT NULL
           AND (json_extract(parent.metadata, '$.test_linkage') IS NULL)",
    )?;

    let mut parent_coverage: HashMap<String, (u32, String, String)> = HashMap::new();
    let parent_rows = parent_stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, u32>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;

    for row in parent_rows {
        let (parent_id, child_count, child_best, child_worst) = row?;
        let entry = parent_coverage.entry(parent_id).or_insert((
            0,
            "stub".to_string(),
            "thorough".to_string(),
        ));
        entry.0 += child_count;
        if tier_rank(&child_best) > tier_rank(&entry.1) {
            entry.1 = child_best;
        }
        if tier_rank(&child_worst) < tier_rank(&entry.2) {
            entry.2 = child_worst;
        }
    }

    if !parent_coverage.is_empty() {
        db.conn.execute_batch("BEGIN")?;
        let agg_result = (|| -> Result<()> {
            for (parent_id, (total_tests, best, worst)) in &parent_coverage {
                let linkage = serde_json::json!({
                    "test_count": total_tests,
                    "best_tier": best,
                    "worst_tier": worst,
                    "linked_tests": [],
                    "linked_test_paths": [],
                    "evidence_sources": ["aggregated_from_methods"],
                    "source": "aggregated_from_methods"
                });
                db.conn.execute(
                    "UPDATE symbols SET metadata = json_set(
                        COALESCE(metadata, '{}'),
                        '$.test_linkage', json(?1)
                    ) WHERE id = ?2",
                    rusqlite::params![linkage.to_string(), parent_id],
                )?;
                stats.symbols_covered += 1;
            }
            Ok(())
        })();

        match agg_result {
            Ok(()) => {
                db.conn.execute_batch("COMMIT")?;
            }
            Err(e) => {
                let _ = db.conn.execute_batch("ROLLBACK");
                return Err(e);
            }
        }
        debug!(
            "Step 4 (parent aggregation): {} classes/structs got linkage from methods",
            parent_coverage.len()
        );
    }

    info!(
        "Test linkage computed: {} symbols covered, {} total linkages",
        stats.symbols_covered, stats.total_linkages
    );

    Ok(stats)
}

/// Count shared directory segments between two paths.
fn common_directory_depth(path_a: &str, path_b: &str) -> usize {
    let dirs_a: Vec<&str> = path_a
        .rsplitn(2, '/')
        .last()
        .unwrap_or("")
        .split('/')
        .collect();
    let dirs_b: Vec<&str> = path_b
        .rsplitn(2, '/')
        .last()
        .unwrap_or("")
        .split('/')
        .collect();
    dirs_a
        .iter()
        .zip(dirs_b.iter())
        .take_while(|(a, b)| a == b)
        .count()
}
