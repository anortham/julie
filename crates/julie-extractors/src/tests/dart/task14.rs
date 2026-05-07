use crate::base::SymbolKind;
use crate::dart::DartExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_dart::LANGUAGE.into())
        .expect("Error loading Dart grammar");
    parser
}

#[test]
fn test_dart_annotations_attach_to_classes_properties_and_type_aliases() {
    let code = r#"
@deprecated
class LegacyHandler {
  @JsonKey(name: 'user_name')
  String userName = '';
}

@deprecated
typedef LegacyCallback = void Function(String value);
"#;

    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = DartExtractor::new(
        "dart".to_string(),
        "annotations.dart".to_string(),
        code.to_string(),
        &workspace_root,
    );

    let symbols = extractor.extract_symbols(&tree);

    let legacy_handler = symbols
        .iter()
        .find(|s| s.name == "LegacyHandler")
        .expect("class should be extracted");
    assert_eq!(legacy_handler.kind, SymbolKind::Class);
    let class_keys: Vec<_> = legacy_handler
        .annotations
        .iter()
        .map(|annotation| annotation.annotation_key.as_str())
        .collect();
    assert_eq!(class_keys, vec!["deprecated"]);

    let user_name = symbols
        .iter()
        .find(|s| s.name == "userName" && s.kind == SymbolKind::Field)
        .expect("field should be extracted");
    let property_keys: Vec<_> = user_name
        .annotations
        .iter()
        .map(|annotation| annotation.annotation_key.as_str())
        .collect();
    assert_eq!(property_keys, vec!["jsonkey"]);

    let legacy_callback = symbols
        .iter()
        .find(|s| s.name == "LegacyCallback" && s.kind == SymbolKind::Type)
        .expect("typedef should be extracted");
    let typedef_keys: Vec<_> = legacy_callback
        .annotations
        .iter()
        .map(|annotation| annotation.annotation_key.as_str())
        .collect();
    assert_eq!(typedef_keys, vec!["deprecated"]);
}
