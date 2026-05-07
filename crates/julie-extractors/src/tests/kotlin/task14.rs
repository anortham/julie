use crate::base::SymbolKind;
use crate::kotlin::KotlinExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_kotlin_ng::LANGUAGE.into())
        .expect("Error loading Kotlin grammar");
    parser
}

#[test]
fn test_kotlin_annotations_attach_to_classes_properties_objects_and_type_aliases() {
    let code = r#"
@Serializable
class AnnotatedModel(
    @javax.inject.Inject private val dependency: Dependency
) {
    @Volatile
    var status: String = "ready"
}

@Singleton
object ServiceRegistry

@Suppress("UNCHECKED_CAST")
typealias Callback<T> = (T) -> Unit
"#;

    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = KotlinExtractor::new(
        "kotlin".to_string(),
        "test.kt".to_string(),
        code.to_string(),
        &workspace_root,
    );

    let symbols = extractor.extract_symbols(&tree);

    let annotated_model = symbols
        .iter()
        .find(|s| s.name == "AnnotatedModel")
        .expect("class should be extracted");
    assert_eq!(annotated_model.kind, SymbolKind::Class);
    let model_keys: Vec<_> = annotated_model
        .annotations
        .iter()
        .map(|annotation| annotation.annotation_key.as_str())
        .collect();
    assert_eq!(model_keys, vec!["serializable"]);

    let service_registry = symbols
        .iter()
        .find(|s| s.name == "ServiceRegistry")
        .expect("object should be extracted");
    assert_eq!(service_registry.kind, SymbolKind::Class);
    let object_keys: Vec<_> = service_registry
        .annotations
        .iter()
        .map(|annotation| annotation.annotation_key.as_str())
        .collect();
    assert_eq!(object_keys, vec!["singleton"]);

    let dependency = symbols
        .iter()
        .find(|s| s.name == "dependency" && s.kind == SymbolKind::Property)
        .expect("constructor property should be extracted");
    let dependency_keys: Vec<_> = dependency
        .annotations
        .iter()
        .map(|annotation| annotation.annotation_key.as_str())
        .collect();
    assert_eq!(dependency_keys, vec!["inject"]);

    let status = symbols
        .iter()
        .find(|s| s.name == "status" && s.kind == SymbolKind::Property)
        .expect("property should be extracted");
    let status_keys: Vec<_> = status
        .annotations
        .iter()
        .map(|annotation| annotation.annotation_key.as_str())
        .collect();
    assert_eq!(status_keys, vec!["volatile"]);

    let callback = symbols
        .iter()
        .find(|s| s.name == "Callback" && s.kind == SymbolKind::Type)
        .expect("type alias should be extracted");
    let callback_keys: Vec<_> = callback
        .annotations
        .iter()
        .map(|annotation| annotation.annotation_key.as_str())
        .collect();
    assert_eq!(callback_keys, vec!["suppress"]);
}
