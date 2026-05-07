use crate::base::{Symbol, SymbolKind};
use crate::swift::SwiftExtractor;
use tree_sitter::Tree;

fn create_extractor_and_parse(code: &str) -> (SwiftExtractor, Tree) {
    use std::path::PathBuf;
    let workspace_root = PathBuf::from("/tmp/test");
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_swift::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();
    let extractor = SwiftExtractor::new(
        "swift".to_string(),
        "test.swift".to_string(),
        code.to_string(),
        &workspace_root,
    );
    (extractor, tree)
}

fn metadata_str<'a>(symbol: &'a Symbol, key: &str) -> Option<&'a str> {
    symbol.metadata.as_ref()?.get(key)?.as_str()
}

#[test]
fn test_swift_extensions_are_not_plain_class_symbols() {
    let swift_code = r#"
class Circle {}

extension Circle {
    func circumference() -> Double { 0.0 }
}

extension [Int] {
    func sum() -> Int { 0 }
}
"#;

    let (mut extractor, tree) = create_extractor_and_parse(swift_code);
    let symbols = extractor.extract_symbols(&tree);

    let circle_extension = symbols
        .iter()
        .find(|symbol| {
            symbol.name == "Circle"
                && symbol
                    .signature
                    .as_ref()
                    .is_some_and(|signature| signature.starts_with("extension Circle"))
        })
        .expect("Circle extension should be extracted");
    assert_ne!(circle_extension.kind, SymbolKind::Class);
    assert_eq!(metadata_str(circle_extension, "type"), Some("extension"));
    assert_eq!(
        metadata_str(circle_extension, "extendedType"),
        Some("Circle")
    );
    assert_eq!(
        metadata_str(circle_extension, "symbol_role"),
        Some("extension")
    );

    let circumference = symbols
        .iter()
        .find(|symbol| symbol.name == "circumference")
        .expect("extension method should be extracted");
    assert_eq!(
        circumference.parent_id,
        Some(circle_extension.id.clone()),
        "extension method should attach to its extension symbol"
    );

    let array_extension = symbols
        .iter()
        .find(|symbol| {
            symbol.name == "[Int]"
                && symbol
                    .signature
                    .as_ref()
                    .is_some_and(|signature| signature.starts_with("extension [Int]"))
        })
        .expect("array extension should be extracted");
    assert_ne!(array_extension.kind, SymbolKind::Class);
    assert_eq!(metadata_str(array_extension, "extendedType"), Some("[Int]"));
    assert_eq!(
        metadata_str(array_extension, "symbol_role"),
        Some("extension")
    );
}

#[test]
fn test_swift_annotations_attach_to_extensions_type_aliases_and_enum_cases() {
    let swift_code = r#"
class Circle {}

@available(iOS 17.0, *)
extension Circle {
    @objc dynamic var observableProperty: String = ""
}

@available(*, deprecated, message: "use ModernHandler")
typealias LegacyHandler = () -> Void

enum Status {
    @available(*, deprecated)
    case legacy
    case current
}
"#;

    let (mut extractor, tree) = create_extractor_and_parse(swift_code);
    let symbols = extractor.extract_symbols(&tree);

    let circle_extension = symbols
        .iter()
        .find(|symbol| {
            symbol.name == "Circle"
                && symbol
                    .signature
                    .as_ref()
                    .is_some_and(|signature| signature.contains("extension Circle"))
        })
        .expect("annotated extension should be extracted");
    assert_eq!(
        metadata_str(circle_extension, "annotationKeys"),
        Some("available")
    );

    let legacy_handler = symbols
        .iter()
        .find(|symbol| symbol.name == "LegacyHandler")
        .expect("annotated typealias should be extracted");
    assert_eq!(
        metadata_str(legacy_handler, "annotationKeys"),
        Some("available")
    );

    let legacy_case = symbols
        .iter()
        .find(|symbol| symbol.name == "legacy")
        .expect("annotated enum case should be extracted");
    assert_eq!(
        metadata_str(legacy_case, "annotationKeys"),
        Some("available")
    );

    let observable_property = symbols
        .iter()
        .find(|symbol| symbol.name == "observableProperty")
        .expect("annotated property should be extracted");
    assert_eq!(
        metadata_str(observable_property, "annotationKeys"),
        Some("objc")
    );
}
