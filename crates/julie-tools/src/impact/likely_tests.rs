use std::collections::HashSet;

use julie_core::Symbol;
use julie_index::analysis::test_linkage::test_linkage_entry;
use julie_index::graph::{Graph, SymbolId};
use julie_index::search::scoring::is_test_path;

use super::ranking::RankedImpact;
use super::seed::SeedContext;
use crate::snapshot_rows::to_symbol;

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

    fn push_path(&mut self, seen: &mut HashSet<String>, path: String) {
        if seen.insert(path.clone()) {
            self.likely_test_paths.push(path);
        }
    }

    fn push_name(&mut self, seen: &mut HashSet<String>, name: String) {
        if seen.insert(name.clone()) {
            self.related_test_symbols.push(name);
        }
    }

    fn finalized(mut self) -> Self {
        self.likely_test_paths_total = self.likely_test_paths.len();
        self.related_test_symbols_total = self.related_test_symbols.len();
        self
    }
}

/// Tests for the seeds and impacts, first match wins:
/// 1. `test_linkage` metadata on a relevant symbol,
/// 2. test symbols with a graph edge into a relevant symbol,
/// 3. test files whose name shares a stem with a relevant symbol's file.
pub fn collect_likely_tests(
    graph: &Graph,
    seed_context: &SeedContext,
    impacts: &[RankedImpact],
) -> LikelyTests {
    let mut tests = LikelyTests::default();
    let mut seen_paths = HashSet::new();
    let mut seen_names = HashSet::new();

    let seed_ids: HashSet<SymbolId> = seed_context.seed_symbols.iter().copied().collect();
    let mut relevant_ids: Vec<SymbolId> = seed_context.seed_symbols.clone();
    relevant_ids.extend(
        impacts
            .iter()
            .filter_map(|impact| graph.symbol_by_row_id(&impact.symbol.id)),
    );
    let relevant_symbols: Vec<Symbol> = relevant_ids
        .iter()
        .map(|id| to_symbol(graph, *id))
        .collect();

    for symbol in &relevant_symbols {
        let Some(linkage) = symbol.metadata.as_ref().and_then(|metadata| {
            let value = serde_json::to_value(metadata).ok()?;
            test_linkage_entry(&value).cloned()
        }) else {
            continue;
        };
        let strings = |key: &str| -> Vec<String> {
            linkage
                .get(key)
                .and_then(|value| value.as_array())
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|value| value.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        };
        for path in strings("linked_test_paths") {
            tests.push_path(&mut seen_paths, path);
        }
        for name in strings("linked_tests") {
            tests.push_name(&mut seen_names, name);
        }
    }
    if !tests.is_empty() {
        return tests.finalized();
    }

    let mut referrers: Vec<SymbolId> = relevant_ids
        .iter()
        .flat_map(|id| graph.references_to(*id))
        .filter(|id| !seed_ids.contains(id))
        .collect();
    referrers.sort_by(|a, b| {
        let (left, right) = (graph.symbol(*a), graph.symbol(*b));
        left.path
            .cmp(&right.path)
            .then_with(|| left.id.cmp(&right.id))
    });
    referrers.dedup();
    for id in referrers {
        let symbol = to_symbol(graph, id);
        if is_test_symbol(&symbol) {
            tests.push_path(&mut seen_paths, symbol.file_path.clone());
            tests.push_name(&mut seen_names, symbol.name.clone());
        }
    }
    if !tests.is_empty() {
        return tests.finalized();
    }

    let file_stems: HashSet<String> = relevant_symbols
        .iter()
        .filter_map(|symbol| symbol.file_path.rsplit('/').next())
        .filter_map(|file_name| file_name.split('.').next())
        .map(|stem| stem.to_ascii_lowercase())
        .collect();
    for path in graph.paths() {
        if !is_test_path(path) {
            continue;
        }
        let matches_stem = path
            .rsplit('/')
            .next()
            .map(|file_name| file_name.to_ascii_lowercase())
            .is_some_and(|file_name| file_stems.iter().any(|stem| file_name.contains(stem)));
        if matches_stem {
            tests.push_path(&mut seen_paths, path.clone());
        }
    }
    tests.finalized()
}

fn is_test_symbol(symbol: &Symbol) -> bool {
    julie_index::analysis::test_roles::is_test_related(symbol) || is_test_path(&symbol.file_path)
}
