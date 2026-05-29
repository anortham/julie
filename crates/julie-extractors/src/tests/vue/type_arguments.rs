//! Vue ordered/nested generic type-argument capture (Miller bridge Phase 2).
//!
//! Vue `<script lang="ts">` sections are parsed with tree-sitter-typescript so they
//! express TypeScript generic syntax: `Array<User>`, `Map<string, Array<User>>`.
//!
//! Nested generics are captured as `children` of the enclosing usage (one
//! `TypeArgumentUsage` per outermost generic), never double-counted.

use crate::base::TypeArgumentUsage;
use crate::vue::VueExtractor;
use std::path::PathBuf;

fn capture(code: &str) -> Vec<TypeArgumentUsage> {
    let workspace_root = PathBuf::from("/test/workspace");
    let mut ext = VueExtractor::new(
        "vue".to_string(),
        "test.vue".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = ext.extract_symbols(None);
    ext.extract_identifiers(&symbols);
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
fn ts_generic_type_annotation_records_single_argument() {
    // `const items: Array<User>` inside a TypeScript script section.
    // Array is outermost; User is ordinal 0.
    let code = r#"
<script lang="ts">
class User {}
const items: Array<User> = []
</script>
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (Array<User>), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "User")]);
    assert!(usages[0].arguments[0].children.is_empty());
}

#[test]
fn ts_nested_generic_preserves_order_and_nesting() {
    // `const map: Map<string, Array<User>>` — Map is outermost; Array[User] is nested.
    // Top-level: (0,"string"), (1,"Array"). Array carries child (0,"User").
    // `Array` inside Map's args must NOT produce a second TypeArgumentUsage row.
    let code = r#"
<script lang="ts">
class User {}
const map: Map<string, Array<User>> = new Map()
</script>
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "Map is the only outermost generic (Array<User> is nested), got {usages:?}"
    );
    let args = &usages[0].arguments;
    assert_eq!(top_level(&usages[0]), vec![(0, "string"), (1, "Array")]);
    assert!(args[0].children.is_empty(), "string has no nested args");
    assert_eq!(
        args[1]
            .children
            .iter()
            .map(|c| (c.ordinal, c.type_name.as_str()))
            .collect::<Vec<_>>(),
        vec![(0, "User")],
        "Array<User> nested argument preserved under ordinal 1"
    );
}

#[test]
fn ts_two_arg_generic_records_ordered_pair() {
    // `const m: Map<string, number>` — flat two-arg generic (no nesting).
    // Top-level: (0,"string"), (1,"number"). No children on either.
    let code = r#"
<script lang="ts">
const m: Map<string, number> = new Map()
</script>
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (Map<string, number>), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "string"), (1, "number")]);
    assert!(usages[0].arguments[0].children.is_empty(), "string has no children");
    assert!(usages[0].arguments[1].children.is_empty(), "number has no children");
}

#[test]
fn ts_non_generic_type_records_no_arguments() {
    // Plain `const name: User` with no `<...>` — not a generic use site.
    let code = r#"
<script lang="ts">
class User {}
const name: User = new User()
</script>
"#;
    let usages = capture(code);
    assert!(
        usages.is_empty(),
        "non-generic type must record no type arguments, got {usages:?}"
    );
}

#[test]
fn ts_new_expression_records_type_args() {
    // `new Map<string, User>()` — construction use site inside a TS script section.
    // The `new_expression` node carries a `type_arguments` direct named child.
    let code = r#"
<script lang="ts">
class User {}
const m = new Map<string, User>()
</script>
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (new Map<string, User>()), got {usages:?}"
    );
    assert_eq!(
        usages[0]
            .arguments
            .iter()
            .map(|a| (a.ordinal, a.type_name.as_str()))
            .collect::<Vec<_>>(),
        vec![(0, "string"), (1, "User")],
    );
}

#[test]
fn ts_extends_clause_records_type_args() {
    // `class Comp extends Base<User>` — heritage use site inside a TS script section.
    // The `extends_clause` carries a `type_arguments` field.
    let code = r#"
<script lang="ts">
class User {}
class Comp extends Base<User> {}
</script>
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (extends Base<User>), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "User")]);
    assert!(usages[0].arguments[0].children.is_empty());
}
