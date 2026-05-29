//! PowerShell ordered/nested generic type-argument capture (Miller bridge Phase 2).
//!
//! PowerShell uses bracket syntax for .NET generic types:
//!   `[List[User]]`, `[Dictionary[string, int]]`
//!
//! The grammar nodes used are:
//!   `generic_type_name` — base name (wraps a `type_name` child)
//!   `generic_type_arguments` — the `[...]` argument list (children are `type_spec`)
//!   `type_spec` — each argument position; contains `type_name` for leaf args or
//!                 `generic_type_name` + `generic_type_arguments` for nested generics
//!
//! Nested generics ride along as `children` of the outermost usage:
//!   `[Map[Key, List[User]]]` → one row for Map; List is a child, not a separate row.

use crate::base::TypeArgumentUsage;
use crate::powershell::PowerShellExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<TypeArgumentUsage> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_powershell::LANGUAGE.into())
        .expect("load PowerShell grammar");
    let tree = parser.parse(code, None).expect("parse PowerShell");
    let mut ext = PowerShellExtractor::new(
        "powershell".to_string(),
        "test.ps1".to_string(),
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
fn typed_var_single_generic_records_one_argument() {
    // `[List[User]]$items` — single outermost generic, User at ordinal 0.
    let code = r#"[List[User]]$items = @()"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (List[User]), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "User")]);
    assert!(usages[0].arguments[0].children.is_empty());
}

#[test]
fn dict_two_args_records_ordered_pair() {
    // `[Dictionary[Key, Value]]$map` — two ordered type arguments.
    let code = r#"[Dictionary[Key, Value]]$map = @{}"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (Dictionary[Key,Value]), got {usages:?}"
    );
    assert_eq!(
        top_level(&usages[0]),
        vec![(0, "Key"), (1, "Value")],
        "Dictionary[Key,Value] ordinal order must be preserved"
    );
}

#[test]
fn nested_generic_preserves_order_and_nesting() {
    // `[Map[Key, List[User]]]$nested` — Map is outermost; List[User] is nested.
    // Only one TypeArgumentUsage row (for Map); List rides as a child.
    let code = r#"[Map[Key, List[User]]]$nested = $null"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "Map is the only outermost use site (List[User] is nested), got {usages:?}"
    );
    assert_eq!(
        top_level(&usages[0]),
        vec![(0, "Key"), (1, "List")],
        "outer arguments: Key at 0, List at 1"
    );
    assert_eq!(
        usages[0].arguments[1]
            .children
            .iter()
            .map(|c| (c.ordinal, c.type_name.as_str()))
            .collect::<Vec<_>>(),
        vec![(0, "User")],
        "List[User] nested child preserved under ordinal 1"
    );
}

#[test]
fn static_new_construction_records_type_argument() {
    // `[List[int]]::new()` — static construction use site.
    // Grammar: invokation_expression → type_literal → type_spec → generic_type_name("List")
    //                                                              generic_type_arguments → type_spec("int")
    // The universal `generic_type_name` arm fires; parent chain is type_spec → type_literal
    // (NOT generic_type_arguments), so is_nested = false → captured as outermost row.
    // int is ordinal 0.
    let code = r#"$list = [List[int]]::new()"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site ([List[int]]::new()), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "int")]);
    assert!(usages[0].arguments[0].children.is_empty());
}

#[test]
fn non_generic_type_records_no_arguments() {
    // `[Container]$x` with no `[...]` — zero rows.
    let code = r#"[Container]$x = $null"#;
    let usages = capture(code);
    assert!(
        usages.is_empty(),
        "non-generic use must record no type arguments, got {usages:?}"
    );
}
