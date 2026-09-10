use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::snapshot_rows::to_symbol;
use julie_core::Symbol;
use julie_facts::rows::SymbolRow;
use julie_index::graph::Graph;
use julie_index::search::index::{SearchFilter, SymbolSearchResult};

#[derive(Debug, Clone, Default)]
pub struct TaskSignals {
    pub edited_files: Vec<String>,
    pub entry_symbols: Vec<String>,
    pub entry_symbol_leaves: HashSet<String>,
    pub stack_trace: Option<String>,
    pub stack_trace_files: Vec<String>,
    pub stack_trace_lines: HashMap<String, Vec<u32>>,
    pub stack_trace_symbols: HashSet<String>,
    pub failing_test: Option<String>,
    pub failing_test_linked_symbol_ids: HashSet<String>,
    pub max_hops: u32,
    pub prefer_tests: bool,
}

impl TaskSignals {
    pub fn from_tool(tool: &super::GetContextTool) -> Self {
        let entry_symbols = tool.entry_symbols.clone().unwrap_or_default();
        let entry_symbol_leaves = entry_symbols
            .iter()
            .filter_map(|symbol| symbol_leaf(symbol))
            .collect();

        let (stack_trace_files, stack_trace_lines, stack_trace_symbols) =
            parse_stack_trace(tool.stack_trace.as_deref());

        Self {
            edited_files: tool.edited_files.clone().unwrap_or_default(),
            entry_symbols,
            entry_symbol_leaves,
            stack_trace: tool.stack_trace.clone(),
            stack_trace_files,
            stack_trace_lines,
            stack_trace_symbols,
            failing_test: tool.failing_test.clone(),
            failing_test_linked_symbol_ids: HashSet::new(),
            max_hops: tool.max_hops.unwrap_or(1),
            prefer_tests: tool.prefer_tests.unwrap_or(false),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.edited_files.is_empty()
            && self.entry_symbols.is_empty()
            && self.stack_trace_files.is_empty()
            && self.stack_trace_symbols.is_empty()
            && self.failing_test.is_none()
            && self.failing_test_linked_symbol_ids.is_empty()
            && self.max_hops <= 1
            && !self.prefer_tests
    }

    pub fn score_multiplier(&self, result: &SymbolSearchResult) -> f32 {
        let mut multiplier = 1.0;

        if self
            .edited_files
            .iter()
            .any(|path| path_matches_signal(&result.file_path, path))
        {
            multiplier *= 2.5;
        }

        if self.entry_symbol_matches(&result.name) {
            multiplier *= 3.0;
        }

        if self
            .stack_trace_files
            .iter()
            .any(|path| path_matches_signal(&result.file_path, path))
        {
            multiplier *= 2.0;
        }

        if self
            .stack_trace_lines
            .iter()
            .find(|(path, _)| path_matches_signal(&result.file_path, path))
            .is_some_and(|(_, lines)| {
                lines
                    .iter()
                    .any(|line| result.start_line.abs_diff(*line) <= 2)
            })
        {
            multiplier *= 1.5;
        }

        if self.stack_trace_symbols.contains(&result.name) {
            multiplier *= 1.5;
        }

        if self.failing_test_linked_symbol_ids.contains(&result.id) {
            multiplier *= 2.5;
        }

        multiplier
    }

    fn entry_symbol_matches(&self, name: &str) -> bool {
        self.entry_symbols.iter().any(|symbol| {
            symbol == name
                || symbol_leaf(symbol)
                    .as_deref()
                    .is_some_and(|leaf| leaf == name)
        }) || self.entry_symbol_leaves.contains(name)
    }
}

fn parse_stack_trace(
    stack_trace: Option<&str>,
) -> (Vec<String>, HashMap<String, Vec<u32>>, HashSet<String>) {
    let mut files = Vec::new();
    let mut lines_by_file = HashMap::new();
    let mut symbols = HashSet::new();

    let Some(stack_trace) = stack_trace else {
        return (files, lines_by_file, symbols);
    };

    for line in stack_trace.lines() {
        for token in line.split_whitespace() {
            let clean = token.trim_matches(|c: char| matches!(c, '(' | ')' | '[' | ']' | ','));
            if let Some((file_path, line_number)) = parse_file_line_token(clean) {
                files.push(file_path.clone());
                lines_by_file
                    .entry(file_path)
                    .or_insert_with(Vec::new)
                    .push(line_number);
            }

            if let Some(symbol) = symbol_leaf(clean) {
                symbols.insert(symbol);
            }
        }
    }

    files.sort();
    files.dedup();
    for line_numbers in lines_by_file.values_mut() {
        line_numbers.sort_unstable();
        line_numbers.dedup();
    }

    (files, lines_by_file, symbols)
}

fn parse_file_line_token(token: &str) -> Option<(String, u32)> {
    let parts: Vec<&str> = token.split(':').collect();
    if parts.len() < 2 {
        return None;
    }

    for index in (1..parts.len()).rev() {
        let Ok(line_number) = parts[index].parse::<u32>() else {
            continue;
        };
        let path = parts[..index].join(":");
        if path.contains('/') || path.contains('\\') || path.contains('.') {
            return Some((path, line_number));
        }
    }

    None
}

fn symbol_leaf(symbol: &str) -> Option<String> {
    let leaf = symbol
        .rsplit_once("::")
        .map(|(_, tail)| tail)
        .or_else(|| symbol.rsplit_once('.').map(|(_, tail)| tail))
        .unwrap_or(symbol)
        .trim_matches(|c: char| matches!(c, ':' | '.' | '(' | ')' | '[' | ']'));

    if leaf.is_empty() || leaf.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }

    Some(leaf.to_string())
}

pub fn path_matches_signal(actual_path: &str, signal_path: &str) -> bool {
    actual_path == signal_path
        || actual_path.ends_with(signal_path)
        || signal_path.ends_with(actual_path)
}

pub fn merge_task_signal_seed_results(
    results: &mut Vec<SymbolSearchResult>,
    graph: &Graph,
    filter: &SearchFilter,
    signals: &TaskSignals,
) {
    if signals.is_empty() {
        return;
    }

    let top_score = results
        .iter()
        .map(|result| result.score)
        .fold(0.0_f32, f32::max);
    let had_search_results = !results.is_empty();
    let mut result_positions: HashMap<String, usize> = results
        .iter()
        .enumerate()
        .map(|(index, result)| (result.id.clone(), index))
        .collect();

    for seed in collect_task_signal_seed_symbols(graph, signals) {
        let score = score_for_seed(seed.priority, top_score, had_search_results);
        let result = symbol_to_search_result(seed.symbol, score);
        if filter.matches_symbol_result(&result) {
            if let Some(index) = result_positions.get(&result.id).copied() {
                results[index].score = results[index].score.max(result.score);
                continue;
            }

            result_positions.insert(result.id.clone(), results.len());
            results.push(result);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum SeedPriority {
    BroadFile,
    StackLine,
    Explicit,
}

struct SeededSymbol {
    symbol: Symbol,
    priority: SeedPriority,
}

fn collect_task_signal_seed_symbols(graph: &Graph, signals: &TaskSignals) -> Vec<SeededSymbol> {
    const MAX_FILE_SIGNAL_SEEDS_PER_FILE: usize = 12;

    let mut seeds: HashMap<String, SeededSymbol> = HashMap::new();

    for path in &signals.edited_files {
        for symbol in symbols_for_signal_path(graph, path)
            .into_iter()
            .take(MAX_FILE_SIGNAL_SEEDS_PER_FILE)
        {
            merge_seed(&mut seeds, symbol, SeedPriority::BroadFile);
        }
    }

    for path in &signals.stack_trace_files {
        let matching_lines = signals
            .stack_trace_lines
            .iter()
            .find(|(signal_path, _)| path_matches_signal(path, signal_path))
            .map(|(_, lines)| lines.as_slice())
            .unwrap_or(&[]);
        for symbol in symbols_for_signal_path(graph, path)
            .into_iter()
            .take(MAX_FILE_SIGNAL_SEEDS_PER_FILE)
        {
            let priority = if matching_lines
                .iter()
                .any(|line| symbol.start_line.abs_diff(*line) <= 2)
            {
                SeedPriority::StackLine
            } else {
                SeedPriority::BroadFile
            };
            merge_seed(&mut seeds, symbol, priority);
        }
    }

    let mut names = signal_symbol_names(signals);
    names.sort();
    names.dedup();
    for name in &names {
        for &id in graph.find_by_name(name) {
            merge_seed(&mut seeds, to_symbol(graph, id), SeedPriority::Explicit);
        }
    }

    for linked_id in &signals.failing_test_linked_symbol_ids {
        if let Some(id) = graph.symbol_by_row_id(linked_id) {
            merge_seed(&mut seeds, to_symbol(graph, id), SeedPriority::Explicit);
        }
    }

    let mut seed_list: Vec<SeededSymbol> = seeds.into_values().collect();
    seed_list.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then_with(|| a.symbol.file_path.cmp(&b.symbol.file_path))
            .then_with(|| a.symbol.start_line.cmp(&b.symbol.start_line))
            .then_with(|| a.symbol.name.cmp(&b.symbol.name))
            .then_with(|| a.symbol.id.cmp(&b.symbol.id))
    });
    seed_list
}

fn merge_seed(seeds: &mut HashMap<String, SeededSymbol>, symbol: Symbol, priority: SeedPriority) {
    match seeds.get_mut(&symbol.id) {
        Some(existing) if priority > existing.priority => {
            existing.priority = priority;
        }
        Some(_) => {}
        None => {
            seeds.insert(symbol.id.clone(), SeededSymbol { symbol, priority });
        }
    }
}

fn score_for_seed(priority: SeedPriority, top_score: f32, had_search_results: bool) -> f32 {
    match priority {
        SeedPriority::Explicit => top_score.max(1.0),
        SeedPriority::StackLine if had_search_results => top_score * 0.75,
        SeedPriority::StackLine => 0.75,
        SeedPriority::BroadFile if had_search_results => top_score * 0.35,
        SeedPriority::BroadFile => 0.35,
    }
}

fn symbols_for_signal_path(graph: &Graph, path: &str) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    let mut seen_ids = HashSet::new();

    let mut candidate_paths: Vec<String> = graph
        .paths()
        .iter()
        .filter(|indexed_path| path_matches_signal(indexed_path, path))
        .cloned()
        .collect();

    if candidate_paths.is_empty() {
        candidate_paths = signal_path_variants(path);
    }
    candidate_paths.sort();
    candidate_paths.dedup();

    for candidate_path in candidate_paths {
        for &id in graph.symbols_in_path(&candidate_path) {
            let symbol = to_symbol(graph, id);
            if seen_ids.insert(symbol.id.clone()) {
                symbols.push(symbol);
            }
        }
    }

    symbols
}

fn signal_path_variants(path: &str) -> Vec<String> {
    let normalized = path.replace('\\', "/");
    let trimmed = normalized.trim_start_matches("./").trim_start_matches('/');
    let mut variants = vec![normalized.clone(), trimmed.to_string()];

    let parts: Vec<&str> = trimmed.split('/').filter(|part| !part.is_empty()).collect();
    for start in 0..parts.len() {
        variants.push(parts[start..].join("/"));
    }

    variants.retain(|variant| !variant.is_empty());
    variants.sort();
    variants.dedup();
    variants
}

fn signal_symbol_names(signals: &TaskSignals) -> Vec<String> {
    let mut names = Vec::new();
    names.extend(signals.entry_symbols.iter().cloned());
    names.extend(
        signals
            .entry_symbols
            .iter()
            .filter_map(|symbol| symbol_leaf(symbol)),
    );
    names.extend(signals.entry_symbol_leaves.iter().cloned());
    names.extend(signals.stack_trace_symbols.iter().cloned());
    names.extend(
        signals
            .stack_trace_symbols
            .iter()
            .filter_map(|symbol| symbol_leaf(symbol)),
    );
    names
}

fn symbol_to_search_result(symbol: Symbol, score: f32) -> SymbolSearchResult {
    let extracted = symbol.extracted;
    let role =
        julie_index::search::scoring::classify_role(&extracted.file_path, &extracted.language);
    let test_role = julie_index::search::scoring::test_subrole(&extracted.file_path);
    SymbolSearchResult {
        id: extracted.id,
        name: extracted.name,
        signature: extracted.signature.unwrap_or_default(),
        doc_comment: extracted.doc_comment.unwrap_or_default(),
        file_path: extracted.file_path,
        kind: extracted.kind.to_string(),
        language: extracted.language,
        start_line: extracted.start_line,
        score,
        role: role.to_string(),
        test_role: test_role.to_string(),
    }
}

/// Populate `signals.failing_test_linked_symbol_ids` with symbols whose
/// `test_linkage`/`test_coverage` metadata references the failing test.
///
/// Called from the pipeline before scoring so that scoring can boost symbols
/// whose linked tests match the failing test signal.
pub fn hydrate_failing_test_links<'a>(
    rows: impl IntoIterator<Item = &'a SymbolRow>,
    signals: &mut TaskSignals,
) {
    let Some(failing_test) = signals.failing_test.clone() else {
        return;
    };
    let normalized_failing_test = failing_test.replace('\\', "/");
    let failing_test_name = failing_test
        .rsplit_once("::")
        .map(|(_, leaf)| leaf)
        .unwrap_or(failing_test.as_str());

    for row in rows {
        let Some(metadata) = row.metadata.as_ref() else {
            continue;
        };
        let linked = |field: &str| -> Vec<&str> {
            ["test_linkage", "test_coverage"]
                .iter()
                .find_map(|key| metadata.get(*key)?.get(field)?.as_array())
                .map(|values| values.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default()
        };
        let path_matches = linked("linked_test_paths")
            .into_iter()
            .any(|linked_path| path_matches_signal(linked_path, &normalized_failing_test));
        let name_matches = linked("linked_tests")
            .into_iter()
            .any(|linked_test| linked_test == failing_test_name);
        if path_matches || name_matches {
            signals
                .failing_test_linked_symbol_ids
                .insert(row.id.clone());
        }
    }
}
