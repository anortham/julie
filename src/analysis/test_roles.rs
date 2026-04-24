//! Post-extraction test role classification.
//!
//! Classifies symbols into test roles (test case, fixture setup, etc.) using
//! annotation-driven config from language TOMLs, with a convention-based fallback
//! from the extractor's `is_test` metadata flag.
//!
//! Runs in the indexing pipeline AFTER extraction and BEFORE the database write.

use std::collections::{HashMap, HashSet};

use crate::extractors::{Symbol, SymbolKind, TestRole};

/// Config-driven test role classifier built from language TOML annotation classes.
///
/// Each field holds the set of annotation keys that map to that role.
/// `classify_annotation` checks them in priority order so that a key
/// appearing in multiple sets resolves deterministically.
#[derive(Debug, Clone, Default)]
pub struct TestRoleConfig {
    pub test_case: HashSet<String>,
    pub parameterized_test: HashSet<String>,
    pub fixture_setup: HashSet<String>,
    pub fixture_teardown: HashSet<String>,
    pub test_container: HashSet<String>,
}

impl TestRoleConfig {
    /// Look up an annotation key and return the matching test role, if any.
    ///
    /// Priority order: test_case > parameterized_test > fixture_setup >
    /// fixture_teardown > test_container. This means if someone accidentally
    /// lists the same key in two sets, the higher-priority role wins.
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

/// Symbol kinds that represent containers (classes, modules, etc.).
fn is_container_kind(kind: &SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Class | SymbolKind::Struct | SymbolKind::Module | SymbolKind::Namespace
    )
}

/// Symbol kinds that represent callable code (functions, methods, constructors).
fn is_callable_kind(kind: &SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Function | SymbolKind::Method | SymbolKind::Constructor
    )
}

/// Classify a single symbol's test role.
///
/// 1. If `role_config` is provided, check each annotation against it.
///    For `TestContainer`, the symbol must have a container kind (Class, Struct,
///    Module, Namespace). For all other roles, the symbol must have a callable
///    kind (Function, Method, Constructor).
/// 2. Fall back to the extractor's `is_test` metadata flag (convention-based
///    languages like Rust, Go, Python). If `is_test` was set, return `TestCase`.
/// 3. Return `None` for non-test symbols.
pub fn classify_test_role(
    symbol: &Symbol,
    role_config: Option<&TestRoleConfig>,
) -> Option<TestRole> {
    // Step 1: annotation-driven classification
    if let Some(config) = role_config {
        for marker in &symbol.annotations {
            if let Some(role) = config.classify_annotation(&marker.annotation_key) {
                // Validate kind compatibility
                match role {
                    TestRole::TestContainer => {
                        if is_container_kind(&symbol.kind) {
                            return Some(role);
                        }
                        // Container annotation on a callable: skip this annotation,
                        // another annotation on the same symbol might match a callable role.
                    }
                    _ => {
                        if is_callable_kind(&symbol.kind) {
                            return Some(role);
                        }
                        // Callable role annotation on a container: skip similarly.
                    }
                }
            }
        }
    }

    // Step 2: convention-based fallback from extractor's is_test flag
    let is_test = symbol
        .metadata
        .as_ref()
        .and_then(|m| m.get("is_test"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if is_test && is_callable_kind(&symbol.kind) {
        return Some(TestRole::TestCase);
    }

    // Step 3: not a test symbol
    None
}

/// Classify all symbols in a batch and set metadata accordingly.
///
/// For each symbol that receives a role:
/// - Sets `metadata["test_role"]` to the role's string form
/// - Sets `metadata["is_test"] = true` for backward compatibility with
///   existing consumers that check `is_test`
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

/// Returns true if the symbol has any test role OR the legacy `is_test` flag.
///
/// Use for: excluding symbols from production rankings, filtering test code
/// out of search results, etc.
pub fn is_test_related(symbol: &Symbol) -> bool {
    let metadata = match &symbol.metadata {
        Some(m) => m,
        None => return false,
    };

    if metadata.contains_key("test_role") {
        return true;
    }

    metadata
        .get("is_test")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Returns true only for symbols with a scorable test role (test_case or
/// parameterized_test).
///
/// Use for: quality scoring, test linkage, assert density analysis. Fixture
/// setup/teardown and test containers are excluded because quality metrics
/// like assert density don't apply to them.
pub fn is_scorable_test(symbol: &Symbol) -> bool {
    let metadata = match &symbol.metadata {
        Some(m) => m,
        None => return false,
    };

    metadata
        .get("test_role")
        .and_then(|v| v.as_str())
        .map(|role| role == "test_case" || role == "parameterized_test")
        .unwrap_or(false)
}
