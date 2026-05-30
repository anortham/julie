//! Shared call-style test extraction for test frameworks whose tests are call
//! expressions (`it("name", () => {})`, `test('adds', () {})`, `describe(...)`)
//! rather than named function declarations.
//!
//! The core — [`TestCallCategory`], [`TestCallVocab`], [`classify_call`], and
//! [`build_test_call_symbol`] — is grammar-agnostic: each language's extractor walks
//! its own grammar (call-node kind, callee field, string-literal kind) and supplies
//! its own vocabulary, then delegates symbol construction here so the captured
//! `is_test` / `test_container` / `test_lifecycle` metadata is identical across
//! languages. The JS/TS path ([`extract_test_call`] + [`is_test_runner_call`]) is
//! built on that core; Dart, Lua, R, and C/C++ adapters reuse the same builder.

use crate::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions};
use std::collections::HashMap;
use tree_sitter::Node;

/// Category of a captured test-DSL call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestCallCategory {
    /// An actual test case — `is_test = true`.
    Test,
    /// A grouping container (`describe`/`group`/`context`) — `test_container = true`.
    Container,
    /// A lifecycle hook (`beforeEach`/`setUp`/…) — `is_test = true` + `test_lifecycle = true`.
    Lifecycle,
}

/// Per-language test-DSL vocabulary: the callee base-names (the segment before any
/// `.skip`/`.only`/`.todo` modifier) that map to each category for one framework
/// family.
#[derive(Debug, Clone, Copy)]
pub struct TestCallVocab<'a> {
    pub test: &'a [&'a str],
    pub container: &'a [&'a str],
    pub lifecycle: &'a [&'a str],
}

/// Classify a callee name against a vocabulary, returning its category if it is a
/// recognized test-DSL call. Handles modifier chains (`it.skip`, `describe.only`,
/// `test.todo`) by matching the segment before the first `.`.
///
/// ⚠️ JS/TS-ONLY VARIANT. The leading-segment split is a Jest/Vitest/Mocha idiom
/// (`it.only`, `describe.skip`) where the DSL word leads the dotted chain. For any
/// OTHER language this is a FALSE-POSITIVE FOOTGUN: a qualified/member callee whose
/// receiver happens to be a vocab word — `it.register("w")` (member access),
/// `describe.default(...)` (an R S3 method name), `Context.Helper` (a dotted
/// bareword command) — collapses to its leading segment and misfires. Languages
/// without dotted DSL modifiers MUST use [`classify_call_exact`] instead. Only
/// JS/TS ([`extract_test_call`] / [`is_test_runner_call`]) may call this.
pub fn classify_call(callee: &str, vocab: &TestCallVocab) -> Option<TestCallCategory> {
    let base = callee.split('.').next().unwrap_or(callee);
    if vocab.test.contains(&base) {
        Some(TestCallCategory::Test)
    } else if vocab.container.contains(&base) {
        Some(TestCallCategory::Container)
    } else if vocab.lifecycle.contains(&base) {
        Some(TestCallCategory::Lifecycle)
    } else {
        None
    }
}

/// Exact-match classifier for frameworks WITHOUT dotted DSL modifiers (every
/// call-style language except JS/TS). A qualified/member callee (`it.register`,
/// `describe.default`, `Context.Helper`) never equals a (dotless) vocab entry, so
/// this closes the false-positive vector — both member-access (Mech A: dotted
/// `field_expression`/`navigation_expression`/`selector` text) and dotted-bareword
/// (Mech B: R S3 names, Bash/PowerShell command names that are a single dotted
/// token a node-kind guard cannot catch) — with no `.`-split. Adapters still walk
/// their own grammar to resolve the callee; this only decides the category.
pub fn classify_call_exact(callee: &str, vocab: &TestCallVocab) -> Option<TestCallCategory> {
    if vocab.test.contains(&callee) {
        Some(TestCallCategory::Test)
    } else if vocab.container.contains(&callee) {
        Some(TestCallCategory::Container)
    } else if vocab.lifecycle.contains(&callee) {
        Some(TestCallCategory::Lifecycle)
    } else {
        None
    }
}

/// Build a `Function` symbol for a captured test-DSL call. Grammar-agnostic: the
/// caller (a per-language adapter) walks the grammar to resolve `full_callee`, the
/// display `name`, and the `category`; this attaches the canonical metadata and
/// creates the symbol.
///
/// Metadata mirrors the historical JS behavior:
/// - `Test` / `Lifecycle` → `is_test = true`
/// - `Container` → `test_container = true`
/// - `Lifecycle` → additionally `test_lifecycle = true`
pub fn build_test_call_symbol(
    base: &mut BaseExtractor,
    node: &Node,
    full_callee: &str,
    name: String,
    category: TestCallCategory,
    parent_id: Option<&str>,
) -> Symbol {
    let signature = match category {
        TestCallCategory::Lifecycle => format!("{}()", full_callee),
        _ => format!("{}(\"{}\")", full_callee, name),
    };

    let mut metadata = HashMap::new();
    match category {
        TestCallCategory::Test => {
            metadata.insert("is_test".to_string(), serde_json::json!(true));
        }
        TestCallCategory::Container => {
            metadata.insert("test_container".to_string(), serde_json::json!(true));
        }
        TestCallCategory::Lifecycle => {
            metadata.insert("is_test".to_string(), serde_json::json!(true));
            metadata.insert("test_lifecycle".to_string(), serde_json::json!(true));
        }
    }

    base.create_symbol(
        node,
        name,
        SymbolKind::Function,
        SymbolOptions {
            signature: Some(signature),
            visibility: None,
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(metadata),
            doc_comment: None,
            annotations: Vec::new(),
        },
    )
}

// ---------------------------------------------------------------------------
// JavaScript / TypeScript (Jest, Vitest, Mocha, Bun)
// ---------------------------------------------------------------------------

/// Test block function names — `is_test = true`
pub const TEST_BLOCKS: &[&str] = &["it", "test"];

/// Container block function names — NOT `is_test` (containers for parent tracking)
pub const CONTAINER_BLOCKS: &[&str] = &["describe", "context", "suite"];

/// Lifecycle block function names — `is_test = true`
pub const LIFECYCLE_BLOCKS: &[&str] = &[
    "beforeEach",
    "afterEach",
    "beforeAll",
    "afterAll",
    "before",
    "after",
];

/// JS/TS test-DSL vocabulary (Jest/Vitest/Mocha/Bun).
const JS_VOCAB: TestCallVocab = TestCallVocab {
    test: TEST_BLOCKS,
    container: CONTAINER_BLOCKS,
    lifecycle: LIFECYCLE_BLOCKS,
};

/// Check whether a JS/TS function name (or its base before `.`) is a test runner
/// call. Handles `.skip`/`.only`/`.todo` variants (`it.skip` -> true via base `it`).
pub fn is_test_runner_call(name: &str) -> bool {
    classify_call(name, &JS_VOCAB).is_some()
}

/// Extract a JS/TS test `call_expression` node into a Symbol.
///
/// Returns `None` if the node is not a `call_expression`, the callee is not a
/// recognized test runner call, or the structure is unexpected (e.g. a test/
/// container block with no string-literal name argument).
pub fn extract_test_call(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    if node.kind() != "call_expression" {
        return None;
    }

    let function_node = node.child_by_field_name("function")?;
    let full_callee = base.get_node_text(&function_node);
    let category = classify_call(&full_callee, &JS_VOCAB)?;

    let name = match category {
        // Lifecycle calls take no name string; use the callee's base name.
        TestCallCategory::Lifecycle => full_callee
            .split('.')
            .next()
            .unwrap_or(&full_callee)
            .to_string(),
        // test/container blocks take the description as the first string argument.
        _ => {
            let args_node = node.child_by_field_name("arguments")?;
            let mut cursor = args_node.walk();
            let first_string = args_node
                .children(&mut cursor)
                .find(|c| c.kind() == "string" || c.kind() == "template_string")?;
            let raw = base.get_node_text(&first_string);
            raw.trim_matches(|c| c == '"' || c == '\'' || c == '`')
                .to_string()
        }
    };

    Some(build_test_call_symbol(
        base,
        &node,
        &full_callee,
        name,
        category,
        parent_id,
    ))
}
