//! Razor ordered/nested generic type-argument capture (Miller bridge Phase 2).
//!
//! Razor embeds C# code in `@code { }` blocks. Its tree-sitter grammar
//! includes the full C# generic syntax (`generic_name` + `type_argument_list`).
//! The same identifier arm and outermost check as the C# extractor applies
//! here — `identifier` inside `generic_name` is the outermost use site.
//!
//! Nested generics ride along as `children` (one TypeArgumentUsage per outermost).

use crate::base::TypeArgumentUsage;
use crate::razor::RazorExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<TypeArgumentUsage> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_razor::LANGUAGE.into())
        .expect("load Razor grammar");
    let tree = parser.parse(code, None).expect("parse Razor");
    let mut ext = RazorExtractor::new(
        "razor".to_string(),
        "test.razor".to_string(),
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
fn field_single_generic_records_one_argument() {
    // `List<IBrowserFile>` field in @code — single outermost generic.
    let code = r#"@code {
    List<IBrowserFile> files;
}"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (List<IBrowserFile>), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "IBrowserFile")]);
    assert!(usages[0].arguments[0].children.is_empty());
}

#[test]
fn dict_two_args_records_ordered_pair() {
    // `Dictionary<string, IBrowserFile>` — two ordered type arguments.
    let code = r#"@code {
    Dictionary<string, IBrowserFile> lookup;
}"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "one generic use site (Dictionary<string,IBrowserFile>), got {usages:?}"
    );
    assert_eq!(
        top_level(&usages[0]),
        vec![(0, "string"), (1, "IBrowserFile")],
        "Dictionary args must be in declaration order"
    );
}

#[test]
fn nested_generic_preserves_order_and_nesting() {
    // `Dictionary<string, List<int>>` — Dict is outermost; List<int> is nested.
    let code = r#"@code {
    Dictionary<string, List<int>> nested;
}"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "Dictionary is the only outermost use site (List<int> is nested), got {usages:?}"
    );
    let args = &usages[0].arguments;
    assert_eq!(
        top_level(&usages[0]),
        vec![(0, "string"), (1, "List")],
        "outer args: string at 0, List at 1"
    );
    assert_eq!(
        args[1]
            .children
            .iter()
            .map(|c| (c.ordinal, c.type_name.as_str()))
            .collect::<Vec<_>>(),
        vec![(0, "int")],
        "List<int> nested children under ordinal 1"
    );
}

#[test]
fn non_generic_type_records_no_arguments() {
    // Plain `Container` with no `<...>` — zero rows.
    let code = r#"@code {
    Container items;
}"#;
    let usages = capture(code);
    assert!(
        usages.is_empty(),
        "non-generic use must record no type arguments, got {usages:?}"
    );
}

#[test]
fn construction_generic_records_argument() {
    // `new List<IBrowserFile>()` in a code block — construction use site.
    let code = r#"@code {
    void Init() {
        var items = new List<IBrowserFile>();
    }
}"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (new List<IBrowserFile>()), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "IBrowserFile")]);
    assert!(usages[0].arguments[0].children.is_empty());
}

#[test]
fn heritage_generic_records_argument() {
    // `class MyList : List<IBrowserFile>` — heritage use site in a code block.
    let code = r#"@code {
    class MyList : List<IBrowserFile> { }
}"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (: List<IBrowserFile>), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "IBrowserFile")]);
    assert!(usages[0].arguments[0].children.is_empty());
}
