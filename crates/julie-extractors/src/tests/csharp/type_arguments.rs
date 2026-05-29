//! C# ordered/nested generic type-argument capture (Miller bridge Phase 2).
//!
//! Every generic *use site* (`IList<RootObject>` field, `new Dictionary<...>()`,
//! `cfg.CreateMap<A,B>()`, `services.AddScoped<IFoo,Foo>()`) must emit its
//! applied type arguments in order, with nesting preserved, attached to the
//! use-site identifier. Order is the whole point — assert exact ordinals, not
//! set membership. Nested generics are captured as children of their enclosing
//! usage (one usage per outermost generic), never double-counted.

use crate::base::TypeArgumentUsage;
use crate::csharp::CSharpExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<TypeArgumentUsage> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
        .expect("load C# grammar");
    let tree = parser.parse(code, None).expect("parse C#");
    let mut ext = CSharpExtractor::new(
        "csharp".to_string(),
        "test.cs".to_string(),
        code.to_string(),
        &PathBuf::from("/test/workspace"),
    );
    let symbols = ext.extract_symbols(&tree);
    ext.extract_identifiers(&tree, &symbols);
    ext.get_base().type_argument_usages.clone()
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
fn field_generic_type_records_single_ordered_argument() {
    let code = r#"
public class Repo {
    public IList<RootObject> Items;
}
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (IList<RootObject>), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "RootObject")]);
    assert!(usages[0].arguments[0].children.is_empty());
}

#[test]
fn nested_generic_preserves_order_and_nesting() {
    let code = r#"
public class Repo {
    public void M() {
        var d = new Dictionary<string, List<int>>();
    }
}
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "Dictionary is the only outermost generic use site (List<int> is nested), got {usages:?}"
    );
    let args = &usages[0].arguments;
    assert_eq!(top_level(&usages[0]), vec![(0, "string"), (1, "List")]);
    assert!(args[0].children.is_empty(), "string has no nested args");
    assert_eq!(
        args[1]
            .children
            .iter()
            .map(|c| (c.ordinal, c.type_name.as_str()))
            .collect::<Vec<_>>(),
        vec![(0, "int")],
        "List<int> nested argument preserved under ordinal 1"
    );
}

#[test]
fn invocation_generic_records_ordered_pair() {
    let code = r#"
public class Profile {
    public Profile(IMapperConfig cfg) {
        cfg.CreateMap<Account, AccountDto>();
    }
}
"#;
    let usages = capture(code);
    assert_eq!(usages.len(), 1, "one generic invocation, got {usages:?}");
    assert_eq!(
        top_level(&usages[0]),
        vec![(0, "Account"), (1, "AccountDto")],
        "CreateMap<A,B> source-vs-dest order must be preserved"
    );
}

#[test]
fn di_registration_generic_records_ordered_pair_without_method_gate() {
    // The DI capture must NOT be gated to a hardcoded method-name allowlist:
    // every generic invocation records its ordered args. AddScoped is just one.
    let code = r#"
public class Startup {
    public void ConfigureServices(IServiceCollection services) {
        services.AddScoped<IFoo, Foo>();
    }
}
"#;
    let usages = capture(code);
    assert_eq!(usages.len(), 1, "one generic registration, got {usages:?}");
    assert_eq!(top_level(&usages[0]), vec![(0, "IFoo"), (1, "Foo")]);
}

#[test]
fn non_generic_type_records_no_arguments() {
    let code = r#"
public class Repo {
    public List Items;
}
"#;
    // `List` with no `<...>` is not a generic application — zero rows.
    let usages = capture(code);
    assert!(
        usages.is_empty(),
        "non-generic use must record no type arguments, got {usages:?}"
    );
}

#[test]
fn construction_generic_records_argument() {
    // `new List<User>()` — construction use site; should capture the type argument.
    let code = r#"
public class Repo {
    public Repo() {
        var items = new List<User>();
    }
}
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (new List<User>()), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "User")]);
    assert!(usages[0].arguments[0].children.is_empty());
}

#[test]
fn heritage_generic_records_argument() {
    // `class MyList : List<User>` — heritage use site; should capture the type argument.
    let code = r#"
public class MyList : List<User> { }
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (: List<User>), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "User")]);
    assert!(usages[0].arguments[0].children.is_empty());
}
