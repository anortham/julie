// C++ Type Usage Identifier Tests
//
// Validates that type_identifier nodes in type reference positions
// produce IdentifierKind::TypeUsage identifiers, while declaration
// names (class/struct/enum definitions, template params) are skipped.

use crate::base::IdentifierKind;
use crate::tests::cpp::parse_cpp;

#[cfg(test)]
mod type_usage_tests {
    use super::*;

    #[test]
    fn test_cpp_type_usage_basic() {
        let code = r#"
class MyClass {};
struct MyStruct {};

void process(MyClass param) {
    MyStruct local;
    MyClass* ptr;
}

MyClass createInstance();
"#;

        let (mut extractor, tree) = parse_cpp(code);
        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        let type_usages: Vec<_> = identifiers
            .iter()
            .filter(|id| id.kind == IdentifierKind::TypeUsage)
            .collect();

        // Should find MyClass in: parameter type, pointer type, return type
        let myclass_usages: Vec<_> = type_usages
            .iter()
            .filter(|id| id.name == "MyClass")
            .collect();
        assert!(
            myclass_usages.len() >= 3,
            "Expected at least 3 MyClass type usages (param, pointer, return), got {}. All type_usages: {:?}",
            myclass_usages.len(),
            type_usages.iter().map(|id| &id.name).collect::<Vec<_>>()
        );

        // Should find MyStruct in: local variable type
        let mystruct_usages: Vec<_> = type_usages
            .iter()
            .filter(|id| id.name == "MyStruct")
            .collect();
        assert!(
            !mystruct_usages.is_empty(),
            "Expected MyStruct type usage for local variable declaration"
        );
    }

    #[test]
    fn test_cpp_type_usage_skips_declarations() {
        let code = r#"
class Widget {};
struct Config {};
enum Color { Red, Green, Blue };
typedef int MyInt;

void use_types(Widget w, Config c) {
    Color color;
}
"#;

        let (mut extractor, tree) = parse_cpp(code);
        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        let type_usages: Vec<_> = identifiers
            .iter()
            .filter(|id| id.kind == IdentifierKind::TypeUsage)
            .collect();

        // Widget appears as class declaration (skip) AND parameter type (keep)
        let widget_usages: Vec<_> = type_usages
            .iter()
            .filter(|id| id.name == "Widget")
            .collect();
        assert_eq!(
            widget_usages.len(),
            1,
            "Widget should appear once as type usage (param), not as declaration. Got: {}",
            widget_usages.len()
        );

        // Config: same -- declaration should be skipped, parameter kept
        let config_usages: Vec<_> = type_usages
            .iter()
            .filter(|id| id.name == "Config")
            .collect();
        assert_eq!(
            config_usages.len(),
            1,
            "Config should appear once as type usage (param)"
        );

        // Color: declaration skipped, local var type kept
        let color_usages: Vec<_> = type_usages.iter().filter(|id| id.name == "Color").collect();
        assert_eq!(
            color_usages.len(),
            1,
            "Color should appear once as type usage (local var)"
        );
    }

    #[test]
    fn test_cpp_type_usage_skips_template_params() {
        let code = r#"
template<typename T>
class Container {
    T value;
};

template<typename U, typename V>
V transform(U input);
"#;

        let (mut extractor, tree) = parse_cpp(code);
        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        let type_usages: Vec<_> = identifiers
            .iter()
            .filter(|id| id.kind == IdentifierKind::TypeUsage)
            .collect();

        // Single-letter type params (T, U, V) should be filtered as noise
        let single_letter: Vec<_> = type_usages
            .iter()
            .filter(|id| id.name.len() == 1 && id.name.chars().next().unwrap().is_ascii_uppercase())
            .collect();
        assert!(
            single_letter.is_empty(),
            "Single-letter template params should be filtered as noise, got: {:?}",
            single_letter.iter().map(|id| &id.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_cpp_type_usage_template_arguments() {
        let code = r#"
class MyClass {};
class MyOther {};

template<typename T>
class Container {};

void foo() {
    Container<MyClass> items;
    Container<MyOther> others;
}
"#;

        let (mut extractor, tree) = parse_cpp(code);
        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        let type_usages: Vec<_> = identifiers
            .iter()
            .filter(|id| id.kind == IdentifierKind::TypeUsage)
            .collect();

        // MyClass should appear as a template argument type usage
        let myclass_in_template: Vec<_> = type_usages
            .iter()
            .filter(|id| id.name == "MyClass")
            .collect();
        assert!(
            !myclass_in_template.is_empty(),
            "MyClass should appear as type usage in template argument"
        );

        // Container should also appear as a type usage
        let container_usages: Vec<_> = type_usages
            .iter()
            .filter(|id| id.name == "Container")
            .collect();
        assert!(
            !container_usages.is_empty(),
            "Container should appear as type usage when used as template type"
        );
    }

    #[test]
    fn test_cpp_type_usage_containing_symbol() {
        let code = r#"
class Widget {};

class Factory {
    Widget create() {
        Widget w;
        return w;
    }
};
"#;

        let (mut extractor, tree) = parse_cpp(code);
        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        let widget_usages: Vec<_> = identifiers
            .iter()
            .filter(|id| id.name == "Widget" && id.kind == IdentifierKind::TypeUsage)
            .collect();

        // At least one Widget usage should have a containing symbol
        let with_containing: Vec<_> = widget_usages
            .iter()
            .filter(|id| id.containing_symbol_id.is_some())
            .collect();
        assert!(
            !with_containing.is_empty(),
            "Widget type usages inside methods should have containing symbol IDs"
        );
    }
}
