//! Go Ginkgo/Gomega call-style test extraction (Miller bridge test-roles, Wave-3).
//!
//! Ginkgo declares tests as call expressions (`call_expression` nodes in the
//! Go grammar), not named function declarations:
//!
//! ```go
//! var _ = Describe("math", func() {
//!     Context("addition", func() {
//!         BeforeEach(func() { })
//!         AfterEach(func() { })
//!         It("should add two numbers", func() {
//!             Expect(1 + 1).To(Equal(2))
//!         })
//!     })
//! })
//! ```
//!
//! Grammar shape (confirmed via live AST probe against tree-sitter-go):
//! - Node kind: `call_expression`
//! - Callee: `function` **field** → `identifier` node (text = `"Describe"`, `"It"`, …)
//! - Description string: `arguments` **field** → `argument_list` → first named child
//!   whose kind is `interpreted_string_literal` → decoded via `base.decode_string_literal`.
//! - Lifecycle calls (`BeforeEach`, `AfterEach`, `BeforeSuite`, `AfterSuite`,
//!   `JustBeforeEach`, `JustAfterEach`, `DeferCleanup`) take only a closure — no
//!   description string. The callee name is used as the symbol name.
//!
//! Focused/pending variants (`FDescribe`, `FIt`, `XDescribe`, `XIt`, `PDescribe`,
//! `PIt`, …) are included so Ginkgo focus/skip markers are still materialised.
//!
//! The standard Go `testing.T`-based idiom (`func TestXxx(t *testing.T)` + `_test.go`
//! path detection) was handled in task #48 via `classify_symbols_by_role`. This
//! adapter is purely additive.

use crate::base::{BaseExtractor, Symbol};
use crate::test_calls::{
    build_test_call_symbol, classify_call_exact, TestCallCategory, TestCallVocab,
};
use tree_sitter::Node;

/// Ginkgo v2 vocabulary (ecosystem-knowledge; verified against
/// <https://pkg.go.dev/github.com/onsi/ginkgo/v2> container/leaf/setup nodes).
///
/// - Container: `Describe` + focused/excluded/table variants, `Context`, `When`
/// - Test: `It`, `Specify` + focused/excluded variants
/// - Lifecycle: `BeforeEach`, `AfterEach`, `BeforeSuite`, `AfterSuite`,
///   `JustBeforeEach`, `JustAfterEach`, `DeferCleanup`
const GINKGO_VOCAB: TestCallVocab = TestCallVocab {
    test: &[
        "It",
        "FIt",
        "XIt",
        "PIt",
        "Specify",
        "FSpecify",
        "XSpecify",
        "PSpecify",
        "Entry",
        "FEntry",
        "XEntry",
        "PEntry",
    ],
    container: &[
        "Describe",
        "FDescribe",
        "XDescribe",
        "PDescribe",
        "Context",
        "FContext",
        "XContext",
        "PContext",
        "When",
        "FWhen",
        "XWhen",
        "PWhen",
        "DescribeTable",
        "FDescribeTable",
        "XDescribeTable",
        "PDescribeTable",
    ],
    lifecycle: &[
        "BeforeEach",
        "AfterEach",
        "BeforeSuite",
        "AfterSuite",
        "JustBeforeEach",
        "JustAfterEach",
        "DeferCleanup",
    ],
};

/// Materialize a Ginkgo `call_expression` as a test/container/lifecycle symbol.
/// Returns `None` for any call that is not a recognised Ginkgo DSL call (e.g.
/// `fmt.Println(...)`, `http.Get(...)`), so the caller can invoke this for every
/// `call_expression` node and only DSL calls become symbols.
pub(super) fn extract_ginkgo_test_call(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    if node.kind() != "call_expression" {
        return None;
    }

    // Callee lives in the `function` field of the `call_expression`.
    // Ginkgo DSL calls are bare identifiers (`Describe`, `It`, …), not selectors.
    let function_node = node.child_by_field_name("function")?;
    if function_node.kind() != "identifier" {
        return None; // skip selector_expression (e.g. `g.Describe(…)`)
    }
    let full_callee = base.get_node_text(&function_node);
    // Exact match only (#66): the `function.kind() != "identifier"` guard above
    // already rejects `selector_expression` callees — use the exact-matcher
    // uniformly so the JS-only `.`-split never applies.
    let category = classify_call_exact(&full_callee, &GINKGO_VOCAB)?;

    let name = match category {
        // Lifecycle: no description string; use the callee name.
        TestCallCategory::Lifecycle => full_callee.to_string(),
        // Describe/Context/It/Specify — first string argument is the description.
        _ => {
            let args_node = node.child_by_field_name("arguments")?; // argument_list
            let mut cursor = args_node.walk();
            let str_arg = args_node
                .named_children(&mut cursor)
                .find(|c| c.kind().contains("string_literal"))?;
            base.decode_string_literal(&str_arg)?
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
