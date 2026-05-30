//! Scala ScalaTest / MUnit call-style test extraction (Miller bridge, Wave-3).
//!
//! ScalaTest and MUnit express tests as call/infix expressions, not named
//! methods, so the declaration-walking extractor misses them. Two grammar
//! shapes, both verified against tree-sitter-scala 0.25 `node-types.json` + an
//! AST probe:
//!
//! 1. Curried call form — FunSuite / MUnit `test("n") { }`, FunSpec
//!    `describe("n") { it("m") { } }`:
//!    ```text
//!    call_expression
//!      [function] call_expression          <- the f("n") clause
//!        [function] identifier  'test'      <- callee (vocab)
//!        [arguments] arguments -> string    <- description
//!      [arguments] block                    <- the { } body
//!    ```
//! 2. FlatSpec / WordSpec infix form — `"subject" should "behaviour" in { }`:
//!    ```text
//!    infix_expression  [operator] 'in'  [right] block
//!      [left] infix_expression  [operator] 'should'|'must'|'can'|'will'
//!        [left] string   <- subject
//!        [right] string  <- behaviour
//!    ```
//!
//! Only the grammar walking is Scala-local; classification + symbol construction
//! delegate to the shared `crate::test_calls` core so the captured `is_test` /
//! `test_container` metadata is byte-identical to every other call-style path.

use crate::base::{BaseExtractor, Symbol};
use crate::test_calls::{
    build_test_call_symbol, classify_call_exact, TestCallCategory, TestCallVocab,
};
use tree_sitter::Node;

/// ScalaTest / MUnit vocabulary.
/// - `test` (FunSuite, MUnit), `it` (FunSpec / WordSpec / FlatSpec result),
///   `scenario` (FeatureSpec) are test cases.
/// - `describe` / `context` (FunSpec), `feature` (FeatureSpec) are containers.
/// - Scala lifecycle hooks (`beforeEach` / `afterAll`) are METHOD OVERRIDES
///   (`def`, caught by the declaration path), not calls, so the call-style
///   lifecycle slice is empty.
const SCALA_VOCAB: TestCallVocab = TestCallVocab {
    test: &["test", "it", "scenario"],
    container: &["describe", "context", "feature"],
    lifecycle: &[],
};

/// FlatSpec / WordSpec behaviour verbs introducing a test clause
/// (`"subject" should "behaviour" in { ... }`).
const FLATSPEC_VERBS: &[&str] = &["should", "must", "can", "will"];

/// Materialize a curried-call ScalaTest/MUnit DSL call (`test("n") { }`,
/// `describe("n") { }`, `it("m") { }`) as a test/container symbol. Returns
/// `None` for any `call_expression` that is not a recognized DSL test clause, so
/// the caller can invoke it for every `call_expression`.
pub(super) fn extract_scala_test_call(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    if node.kind() != "call_expression" {
        return None;
    }
    // The brace body is the OUTER call's `arguments` (a `block`); the callee +
    // description live in the OUTER call's `function`, which is the INNER
    // `f("name")` call_expression. A call without a block body (the inner
    // `test("name")` itself, or `assert(x)`) is not a DSL test clause.
    let body = node.child_by_field_name("arguments")?;
    if body.kind() != "block" {
        return None;
    }
    let inner = node.child_by_field_name("function")?;
    if inner.kind() != "call_expression" {
        return None;
    }
    let callee_node = inner.child_by_field_name("function")?;
    let callee = base.get_node_text(&callee_node);
    // Exact match only (#66): a curried member call (`feature.enable("x") { }`,
    // inner callee = `field_expression` "feature.enable") never equals a dotless
    // ScalaTest/MUnit clause name, so the exact-matcher rejects it without the
    // JS-only leading-segment split.
    let category = classify_call_exact(&callee, &SCALA_VOCAB)?;

    // Description = first `string` in the inner call's argument list.
    let inner_args = inner.child_by_field_name("arguments")?;
    let mut cursor = inner_args.walk();
    let string_node = inner_args
        .children(&mut cursor)
        .find(|c| c.kind() == "string")?;
    let name = base.decode_string_literal(&string_node)?;

    Some(build_test_call_symbol(
        base, node, &callee, name, category, parent_id,
    ))
}

/// Materialize a FlatSpec / WordSpec infix test clause
/// (`"subject" should "behaviour" in { ... }`) as an `is_test` symbol named
/// `"subject should behaviour"`. Returns `None` for every other infix
/// expression (the arm is invoked for all `infix_expression` nodes, and Scala
/// uses infix for arithmetic/comparison/etc., so the guards are deliberately
/// tight: operator `in` + block body + a `<verb>` behaviour clause on the left).
pub(super) fn extract_scala_flatspec_test(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    if node.kind() != "infix_expression" {
        return None;
    }
    // Outer clause: `<behaviour-infix> in { ... }`
    let op = node.child_by_field_name("operator")?;
    if base.get_node_text(&op) != "in" {
        return None;
    }
    let body = node.child_by_field_name("right")?;
    if body.kind() != "block" {
        return None;
    }
    // Left side: `"subject" <verb> "behaviour"`
    let left = node.child_by_field_name("left")?;
    if left.kind() != "infix_expression" {
        return None;
    }
    let verb_node = left.child_by_field_name("operator")?;
    let verb = base.get_node_text(&verb_node);
    if !FLATSPEC_VERBS.contains(&verb.as_str()) {
        return None;
    }
    let subject_node = left.child_by_field_name("left")?;
    let behaviour_node = left.child_by_field_name("right")?;
    let subject = base
        .decode_string_literal(&subject_node)
        .unwrap_or_else(|| base.get_node_text(&subject_node));
    let behaviour = base
        .decode_string_literal(&behaviour_node)
        .unwrap_or_else(|| base.get_node_text(&behaviour_node));
    let name = format!("{subject} {verb} {behaviour}");

    Some(build_test_call_symbol(
        base,
        node,
        &verb,
        name,
        TestCallCategory::Test,
        parent_id,
    ))
}
