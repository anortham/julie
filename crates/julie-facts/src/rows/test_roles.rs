//! Test-role classification from annotation classes and extractor metadata.
//! Copied from `julie_index::analysis::test_roles` over the extractor `Symbol`.

use std::collections::{HashMap, HashSet};

use julie_extractors::{Symbol, SymbolKind, TestRole};

/// Annotation keys that map to each test role for one language.
#[derive(Debug, Clone, Default)]
pub struct TestRoleConfig {
    pub test_case: HashSet<String>,
    pub parameterized_test: HashSet<String>,
    pub fixture_setup: HashSet<String>,
    pub fixture_teardown: HashSet<String>,
    pub test_container: HashSet<String>,
    /// Base types whose presence makes a container a `TestContainer` without an
    /// annotation, matched by last path segment.
    pub test_base_types: HashSet<String>,
}

impl TestRoleConfig {
    pub fn classify_annotation(&self, annotation_key: &str) -> Option<TestRole> {
        if self.test_case.contains(annotation_key) {
            Some(TestRole::TestCase)
        } else if self.parameterized_test.contains(annotation_key) {
            Some(TestRole::ParameterizedTest)
        } else if self.fixture_setup.contains(annotation_key) {
            Some(TestRole::FixtureSetup)
        } else if self.fixture_teardown.contains(annotation_key) {
            Some(TestRole::FixtureTeardown)
        } else if self.test_container.contains(annotation_key) {
            Some(TestRole::TestContainer)
        } else {
            None
        }
    }
}

fn is_container_kind(kind: &SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Class | SymbolKind::Struct | SymbolKind::Module | SymbolKind::Namespace
    )
}

fn is_callable_kind(kind: &SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Function | SymbolKind::Method | SymbolKind::Constructor
    )
}

fn last_type_segment(name: &str) -> &str {
    name.rsplit(['.', ':']).next().unwrap_or(name).trim()
}

fn symbol_base_types(symbol: &Symbol) -> Vec<String> {
    let Some(metadata) = symbol.metadata.as_ref() else {
        return Vec::new();
    };
    for key in ["base_types", "superclasses"] {
        if let Some(array) = metadata.get(key).and_then(|v| v.as_array()) {
            let names: Vec<String> = array
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect();
            if !names.is_empty() {
                return names;
            }
        }
    }
    Vec::new()
}

fn metadata_bool(symbol: &Symbol, key: &str) -> bool {
    symbol
        .metadata
        .as_ref()
        .and_then(|m| m.get(key))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

const TEARDOWN_PHRASES: &[&str] = &["teardown", "cleanup", "dispose", "finalize"];
const TEARDOWN_WORDS: &[&str] = &["after", "end"];

fn split_camel_case(s: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = 0;
    let chars: Vec<char> = s.chars().collect();
    for i in 1..chars.len() {
        let prev = chars[i - 1];
        let curr = chars[i];
        let split_before_upper = prev.is_lowercase() && curr.is_uppercase();
        let split_acronym =
            i >= 2 && chars[i - 2].is_uppercase() && prev.is_uppercase() && curr.is_lowercase();
        if split_before_upper || split_acronym {
            let split_pos = if split_acronym { i - 1 } else { i };
            if split_pos > start {
                let byte_start: usize = chars[..start].iter().map(|c| c.len_utf8()).sum();
                let byte_end: usize = chars[..split_pos].iter().map(|c| c.len_utf8()).sum();
                result.push(&s[byte_start..byte_end]);
                start = split_pos;
            }
        }
    }
    if start < chars.len() {
        let byte_start: usize = chars[..start].iter().map(|c| c.len_utf8()).sum();
        result.push(&s[byte_start..]);
    }
    result
}

fn lifecycle_name_words(name: &str) -> Vec<String> {
    name.split(|ch: char| !ch.is_alphanumeric())
        .filter(|part| !part.is_empty())
        .flat_map(split_camel_case)
        .map(str::to_lowercase)
        .collect()
}

fn lifecycle_role_from_name(name: &str) -> TestRole {
    let squashed = name
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    if TEARDOWN_PHRASES
        .iter()
        .any(|phrase| squashed.contains(phrase))
    {
        return TestRole::FixtureTeardown;
    }
    let words = lifecycle_name_words(name);
    if words
        .iter()
        .any(|word| TEARDOWN_WORDS.contains(&word.as_str()))
    {
        return TestRole::FixtureTeardown;
    }
    TestRole::FixtureSetup
}

pub fn classify_test_role(
    symbol: &Symbol,
    role_config: Option<&TestRoleConfig>,
) -> Option<TestRole> {
    if let Some(config) = role_config {
        for marker in &symbol.annotations {
            if let Some(role) = config.classify_annotation(&marker.annotation_key) {
                let kind_matches = match role {
                    TestRole::TestContainer => is_container_kind(&symbol.kind),
                    _ => is_callable_kind(&symbol.kind),
                };
                if kind_matches {
                    return Some(role);
                }
            }
        }
        if !config.test_base_types.is_empty() && is_container_kind(&symbol.kind) {
            let configured: HashSet<&str> = config
                .test_base_types
                .iter()
                .map(|t| last_type_segment(t))
                .collect();
            if symbol_base_types(symbol)
                .iter()
                .any(|base| configured.contains(last_type_segment(base)))
            {
                return Some(TestRole::TestContainer);
            }
        }
    }
    if metadata_bool(symbol, "test_container") {
        return Some(TestRole::TestContainer);
    }
    if metadata_bool(symbol, "test_lifecycle") && is_callable_kind(&symbol.kind) {
        return Some(lifecycle_role_from_name(&symbol.name));
    }
    if metadata_bool(symbol, "is_test") && is_callable_kind(&symbol.kind) {
        return Some(TestRole::TestCase);
    }
    None
}

/// Set `metadata["test_role"]` and `metadata["is_test"]` on every symbol that
/// receives a role.
pub fn classify_symbols_by_role(
    symbols: &mut [Symbol],
    role_configs: &HashMap<String, TestRoleConfig>,
) {
    for symbol in symbols.iter_mut() {
        let config = role_configs.get(&symbol.language);
        if let Some(role) = classify_test_role(symbol, config) {
            let metadata = symbol.metadata.get_or_insert_with(HashMap::new);
            metadata.insert(
                "test_role".to_string(),
                serde_json::Value::String(role.as_str().to_string()),
            );
            metadata.insert("is_test".to_string(), serde_json::Value::Bool(true));
        }
    }
}
