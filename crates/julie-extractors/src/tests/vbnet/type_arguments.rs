//! VB.NET ordered/nested generic type-argument capture (Miller bridge Phase 2).
//!
//! VB.NET uses `List(Of String)` / `Dictionary(Of String, Integer)` syntax.
//! Grammar: `generic_type` → `namespace_name` (base name) + `type_argument_list`.
//!
//! Nested generics ride along as `children`; only outermost use sites produce
//! a `TypeArgumentUsage` row.

use crate::base::TypeArgumentUsage;
use crate::vbnet::VbNetExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<TypeArgumentUsage> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_vb_dotnet::LANGUAGE.into())
        .expect("load VB.NET grammar");
    let tree = parser.parse(code, None).expect("parse VB.NET");
    let workspace = PathBuf::from("/test/workspace");
    let mut ext = VbNetExtractor::new(
        "vbnet".to_string(),
        "test.vb".to_string(),
        code.to_string(),
        &workspace,
    );
    let symbols = ext.extract_symbols(&tree);
    ext.extract_identifiers(&tree, &symbols);
    ext.get_type_argument_usages()
}

fn top_level(usage: &TypeArgumentUsage) -> Vec<(u32, &str)> {
    usage
        .arguments
        .iter()
        .map(|a| (a.ordinal, a.type_name.as_str()))
        .collect()
}

#[test]
fn field_single_generic_records_one_argument() {
    // `Dim items As List(Of User)` — List is outermost; User is ordinal 0.
    let code = r#"
Class Example
    Dim items As List(Of User)
End Class
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (List(Of User)), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "User")]);
    assert!(usages[0].arguments[0].children.is_empty());
}

#[test]
fn dict_two_args_records_ordered_pair() {
    // `Dim map As Dictionary(Of String, User)` → args: [(0,"String"), (1,"User")]
    let code = r#"
Class Example
    Dim map As Dictionary(Of String, User)
End Class
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site, got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "String"), (1, "User")],);
}

#[test]
fn nested_generic_preserves_order_and_nesting() {
    // `Dim map As Dictionary(Of String, List(Of User))`
    // Dictionary is outermost → 1 row; List(Of User) is nested under ordinal 1.
    let code = r#"
Class Example
    Dim map As Dictionary(Of String, List(Of User))
End Class
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "Dictionary is the only outermost generic, got {usages:?}"
    );
    let args = &usages[0].arguments;
    assert_eq!(top_level(&usages[0]), vec![(0, "String"), (1, "List")]);
    assert!(args[0].children.is_empty(), "String has no nested args");
    assert_eq!(
        args[1]
            .children
            .iter()
            .map(|c| (c.ordinal, c.type_name.as_str()))
            .collect::<Vec<_>>(),
        vec![(0, "User")],
        "List(Of User) nested argument preserved under ordinal 1"
    );
}

#[test]
fn non_generic_type_records_no_arguments() {
    // `Dim name As User` — plain type, no `(Of ...)` → no TypeArgumentUsage rows.
    let code = r#"
Class Example
    Dim name As User
End Class
"#;
    let usages = capture(code);
    assert!(
        usages.is_empty(),
        "non-generic type must record no type arguments, got {usages:?}"
    );
}

#[test]
fn construction_new_records_argument() {
    // `New List(Of User)()` — construction use site; should capture the type argument.
    let code = r#"
Class Example
    Sub Init()
        Dim items = New List(Of User)()
    End Sub
End Class
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (New List(Of User)()), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "User")]);
    assert!(usages[0].arguments[0].children.is_empty());
}

#[test]
fn heritage_inherits_records_argument() {
    // `Inherits List(Of User)` — heritage use site; should capture the type argument.
    let code = r#"
Class MyList
    Inherits List(Of User)
End Class
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (Inherits List(Of User)), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "User")]);
    assert!(usages[0].arguments[0].children.is_empty());
}
