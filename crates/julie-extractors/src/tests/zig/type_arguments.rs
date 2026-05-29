//! Zig scoped generic type-argument capture (Miller bridge Phase 2).
//!
//! In Zig, generics are comptime functions: `ArrayList(i32)` is a
//! `call_expression`, grammatically indistinguishable from a regular call.
//! Capture is scoped to **type-position** calls only:
//!   `var items: ArrayList(User) = undefined;`  → type-position → record
//!   `const y = parseData(bytes);`              → value-position → skip
//!
//! The `arguments` node of the in-type-position `call_expression` is passed to
//! the shared `extract_type_arguments` infrastructure. Nested generics
//! (`Map(Key, ArrayList(User))`) ride along as `children`.

use crate::base::TypeArgumentUsage;
use crate::zig::ZigExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<TypeArgumentUsage> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_zig::LANGUAGE.into())
        .expect("load Zig grammar");
    let tree = parser.parse(code, None).expect("parse Zig");
    let mut ext = ZigExtractor::new(
        "zig".to_string(),
        "test.zig".to_string(),
        code.to_string(),
        &PathBuf::from("/test/workspace"),
    );
    let symbols = ext.extract_symbols(&tree);
    ext.extract_identifiers(&tree, &symbols);
    ext.get_type_argument_usages()
}

/// Flatten a usage's top-level arguments to `(ordinal, type_name)` pairs.
fn top_level(usage: &TypeArgumentUsage) -> Vec<(u32, &str)> {
    usage
        .arguments
        .iter()
        .map(|arg| (arg.ordinal, arg.type_name.as_str()))
        .collect()
}

#[test]
fn var_single_generic_in_type_position_records_one_argument() {
    // `var items: ArrayList(User)` — type-position call, User at ordinal 0.
    let code = r#"var items: ArrayList(User) = undefined;"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (ArrayList(User)), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "User")]);
    assert!(usages[0].arguments[0].children.is_empty());
}

#[test]
fn two_arg_generic_records_ordered_pair() {
    // `var map: HashMap(Key, Value)` — two ordered type arguments.
    let code = r#"var map: HashMap(Key, Value) = undefined;"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (HashMap(Key,Value)), got {usages:?}"
    );
    assert_eq!(
        top_level(&usages[0]),
        vec![(0, "Key"), (1, "Value")],
        "HashMap(Key,Value) ordinal order must be preserved"
    );
}

#[test]
fn nested_generic_preserves_order_and_nesting() {
    // `var nested: Map(Key, ArrayList(User))` — Map is outermost; ArrayList nested.
    // Only one TypeArgumentUsage row (for Map); ArrayList rides as a child.
    let code = r#"var nested: Map(Key, ArrayList(User)) = undefined;"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "Map is the only outermost use site (ArrayList(User) is nested), got {usages:?}"
    );
    assert_eq!(
        top_level(&usages[0]),
        vec![(0, "Key"), (1, "ArrayList")],
        "outer arguments: Key at 0, ArrayList at 1"
    );
    assert_eq!(
        usages[0].arguments[1]
            .children
            .iter()
            .map(|c| (c.ordinal, c.type_name.as_str()))
            .collect::<Vec<_>>(),
        vec![(0, "User")],
        "ArrayList(User) nested child preserved under ordinal 1"
    );
}

#[test]
fn non_type_position_call_records_no_arguments() {
    // `const y = parseData(bytes)` — value-position call, not a type annotation.
    // Must NOT produce any TypeArgumentUsage rows.
    let code = r#"
const bytes = [_]u8{1, 2, 3};
const y = parseData(bytes);
"#;
    let usages = capture(code);
    assert!(
        usages.is_empty(),
        "value-position call must not record type arguments, got {usages:?}"
    );
}
