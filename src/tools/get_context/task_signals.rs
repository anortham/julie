use std::collections::{HashMap, HashSet};

use crate::search::index::SymbolSearchResult;

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
