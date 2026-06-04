use std::collections::{HashMap, HashSet};

use anyhow::Result;

use julie_index::analysis::test_linkage::test_linkage_entry;
use julie_core::database::{IdentifierRef, SymbolDatabase};
use julie_index::search::scoring::is_test_path;

use super::ranking::RankedImpact;
use super::seed::SeedContext;

/// Bundle of test evidence surfaced next to a blast radius report.
///
/// Two collections instead of one mixed list: paths drive navigation,
/// symbol names are supplementary context. Keeps the formatter honest about
/// what "Likely tests" means.
#[derive(Debug, Default, Clone)]
pub struct LikelyTests {
    pub likely_test_paths: Vec<String>,
    pub related_test_symbols: Vec<String>,
    /// Pre-truncate counts so the formatter can surface overflow markers
    /// independently per collection.
    pub likely_test_paths_total: usize,
    pub related_test_symbols_total: usize,
}

impl LikelyTests {
    pub fn is_empty(&self) -> bool {
        self.likely_test_paths.is_empty() && self.related_test_symbols.is_empty()
    }

    pub fn visible(&self, limit: usize) -> Self {
        let mut visible = self.clone();
        visible.likely_test_paths_total = self
            .likely_test_paths_total
            .max(self.likely_test_paths.len());
        visible.related_test_symbols_total = self
            .related_test_symbols_total
            .max(self.related_test_symbols.len());
        visible.likely_test_paths.truncate(limit);
        visible.related_test_symbols.truncate(limit);
        visible
    }
}

pub fn collect_likely_tests(
    db: &SymbolDatabase,
    seed_context: &SeedContext,
    impacts: &[RankedImpact],
) -> Result<LikelyTests> {
    let mut tests = LikelyTests::default();

    let relevant_symbols: Vec<_> = seed_context
        .seed_symbols
        .iter()
        .chain(impacts.iter().map(|impact| &impact.symbol))
        .collect();
    let seed_ids: HashSet<String> = seed_context
        .seed_symbols
        .iter()
        .map(|symbol| symbol.id.clone())
        .collect();
    let relevant_ids: HashSet<String> = relevant_symbols
        .iter()
        .map(|symbol| symbol.id.clone())
        .collect();

    // Tier 1: metadata-declared linkage from test_linkage / test_coverage.
    // Paths go into likely_test_paths, bare names into related_test_symbols.
    let mut seen_paths = HashSet::new();
    let mut seen_names = HashSet::new();
    for symbol in &relevant_symbols {
        if let Some(linkage) = symbol.metadata.as_ref().and_then(|metadata| {
            let value = serde_json::to_value(metadata).ok()?;
            test_linkage_entry(&value).cloned()
        }) {
            if let Some(linked_test_paths) = linkage
                .get("linked_test_paths")
                .and_then(|value| value.as_array())
            {
                for linked_test_path in linked_test_paths.iter().filter_map(|value| value.as_str())
                {
                    push_unique(
                        &mut tests.likely_test_paths,
                        &mut seen_paths,
                        linked_test_path.to_string(),
                    );
                }
            }
            if let Some(linked_tests) = linkage
                .get("linked_tests")
                .and_then(|value| value.as_array())
            {
                for linked_test in linked_tests.iter().filter_map(|value| value.as_str()) {
                    push_unique(
                        &mut tests.related_test_symbols,
                        &mut seen_names,
                        linked_test.to_string(),
                    );
                }
            }
        }
    }

    if !tests.is_empty() {
        finalize_likely_tests(&mut tests);
        return Ok(tests);
    }

    // Tier 2: relationships table — any test symbol that calls/uses the
    // relevant symbols. Yields test file paths.
    let symbol_ids: Vec<String> = relevant_symbols
        .iter()
        .map(|symbol| symbol.id.clone())
        .collect();
    let relationship_tests = db.get_relationships_to_symbols(&symbol_ids)?;
    let mut from_ids: Vec<String> = relationship_tests
        .iter()
        .map(|relationship| relationship.from_symbol_id.clone())
        .collect();
    from_ids.sort();
    from_ids.dedup();
    let mut from_symbols = db.get_symbols_by_ids(&from_ids)?;
    from_symbols.sort_by(|a, b| a.file_path.cmp(&b.file_path).then_with(|| a.id.cmp(&b.id)));
    for symbol in from_symbols {
        if is_test_symbol(&symbol) {
            push_unique(
                &mut tests.likely_test_paths,
                &mut seen_paths,
                symbol.file_path.clone(),
            );
        }
    }

    if !tests.likely_test_paths.is_empty() {
        finalize_likely_tests(&mut tests);
        return Ok(tests);
    }

    // Tier 3: identifiers table. First pass — resolved matches where
    // target_symbol_id points at a seed. Those are much higher signal than
    // name-only matches. If any resolved matches exist, we use ONLY them so
    // the result stays tight.
    let relevant_names: Vec<String> = relevant_symbols
        .iter()
        .map(|symbol| symbol.name.clone())
        .collect();
    let mut identifier_refs = db.get_identifiers_by_names(&relevant_names)?;

    // Drop rows whose container is a seed — a seed "calling itself" via its
    // own name is noise.
    identifier_refs.retain(|iref| {
        iref.containing_symbol_id
            .as_ref()
            .is_none_or(|id| !seed_ids.contains(id))
    });

    let resolved_refs: Vec<IdentifierRef> = identifier_refs
        .iter()
        .filter(|iref| {
            iref.target_symbol_id
                .as_ref()
                .is_some_and(|target| relevant_ids.contains(target))
        })
        .cloned()
        .collect();

    let mut working_refs = if resolved_refs.is_empty() {
        identifier_refs
    } else {
        resolved_refs
    };
    sort_identifier_refs(&mut working_refs);

    let containing_ids: Vec<String> = working_refs
        .iter()
        .filter_map(|identifier| identifier.containing_symbol_id.clone())
        .collect();
    let containing_symbols = db.get_symbols_by_ids(&containing_ids)?;
    let containing_map: HashMap<String, julie_extractors::Symbol> = containing_symbols
        .into_iter()
        .map(|symbol| (symbol.id.clone(), symbol))
        .collect();

    for identifier in working_refs {
        let containing_symbol = identifier
            .containing_symbol_id
            .as_ref()
            .and_then(|id| containing_map.get(id));
        if containing_symbol.is_some_and(is_test_symbol) || is_test_path(&identifier.file_path) {
            let test_path = containing_symbol
                .map(|symbol| symbol.file_path.clone())
                .unwrap_or_else(|| identifier.file_path.clone());
            push_unique(&mut tests.likely_test_paths, &mut seen_paths, test_path);
            if let Some(symbol) = containing_symbol {
                push_unique(
                    &mut tests.related_test_symbols,
                    &mut seen_names,
                    symbol.name.clone(),
                );
            }
        }
    }

    if !tests.is_empty() {
        finalize_likely_tests(&mut tests);
        return Ok(tests);
    }

    // Tier 4: stem-matching fallback. Walk the file index in deterministic
    // order and flag test files whose name shares a stem with any relevant
    // symbol's source file. Paths only (no symbol names).
    let mut stmt = db.conn.prepare("SELECT path FROM files ORDER BY path")?;
    let file_rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let file_stems: HashSet<String> = relevant_symbols
        .iter()
        .filter_map(|symbol| symbol.file_path.rsplit('/').next())
        .filter_map(|file_name| file_name.split('.').next())
        .map(|stem| stem.to_ascii_lowercase())
        .collect();

    for row in file_rows {
        let path = row?;
        if !is_test_path(&path) {
            continue;
        }
        let matches_stem = path
            .rsplit('/')
            .next()
            .map(|file_name| file_name.to_ascii_lowercase())
            .is_some_and(|file_name| file_stems.iter().any(|stem| file_name.contains(stem)));
        if matches_stem {
            push_unique(&mut tests.likely_test_paths, &mut seen_paths, path);
        }
    }

    finalize_likely_tests(&mut tests);
    Ok(tests)
}

fn push_unique(values: &mut Vec<String>, seen: &mut HashSet<String>, candidate: String) {
    if seen.insert(candidate.clone()) {
        values.push(candidate);
    }
}

fn finalize_likely_tests(tests: &mut LikelyTests) {
    // Capture totals so the formatter can emit an overflow marker per
    // collection. Independent caps: paths and symbol names never share a budget.
    tests.likely_test_paths_total = tests.likely_test_paths.len();
    tests.related_test_symbols_total = tests.related_test_symbols.len();
}

fn sort_identifier_refs(refs: &mut [IdentifierRef]) {
    refs.sort_by(|a, b| {
        // Confidence descending, then file_path ascending, then
        // containing_symbol_id ascending (break ties deterministically).
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.file_path.cmp(&b.file_path))
            .then_with(|| {
                let left = a.containing_symbol_id.as_deref().unwrap_or("");
                let right = b.containing_symbol_id.as_deref().unwrap_or("");
                left.cmp(right)
            })
            .then_with(|| a.start_line.cmp(&b.start_line))
    });
}

fn is_test_symbol(symbol: &julie_extractors::Symbol) -> bool {
    julie_index::analysis::test_roles::is_test_related(symbol) || is_test_path(&symbol.file_path)
}
