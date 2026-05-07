use crate::base::{Symbol, SymbolKind};
use crate::scala::ScalaExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_scala::LANGUAGE.into())
        .expect("Error loading Scala grammar");
    parser
}

fn extract_symbols(code: &str) -> Vec<Symbol> {
    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = ScalaExtractor::new(
        "scala".to_string(),
        "test.scala".to_string(),
        code.to_string(),
        &workspace_root,
    );
    extractor.extract_symbols(&tree)
}

#[test]
fn test_scala_annotations_attach_to_classes_properties_objects_and_type_aliases() {
    let code = r#"
@entity
class User

@singleton
object Registry {
  @tracked val state: Int = 0
}

@opaque
type UserId = String

@ops
extension (value: String)
  def shout: String = value.toUpperCase()
"#;

    let symbols = extract_symbols(code);
    let annotation_keys = |name: &str| -> Vec<&str> {
        symbols
            .iter()
            .find(|symbol| symbol.name == name)
            .unwrap_or_else(|| panic!("expected symbol {name}"))
            .annotations
            .iter()
            .map(|marker| marker.annotation_key.as_str())
            .collect()
    };

    assert_eq!(annotation_keys("User"), vec!["entity"]);
    assert_eq!(annotation_keys("Registry"), vec!["singleton"]);
    assert_eq!(annotation_keys("state"), vec!["tracked"]);
    assert_eq!(annotation_keys("UserId"), vec!["opaque"]);

    let extension_method = symbols
        .iter()
        .find(|symbol| symbol.name == "shout")
        .unwrap_or_else(|| panic!("expected extension method symbol"));
    let extension_annotation_keys: Vec<_> = extension_method
        .annotations
        .iter()
        .map(|marker| marker.annotation_key.as_str())
        .collect();
    assert!(
        extension_annotation_keys.contains(&"ops"),
        "expected extension-related annotation on method, got {:?}",
        extension_annotation_keys
    );
}

#[test]
fn test_scala_case_class_fields_are_property_symbols() {
    let code = r#"
case class User(name: String, age: Int)
"#;
    let symbols = extract_symbols(code);

    let class_symbol = symbols
        .iter()
        .find(|symbol| symbol.name == "User" && symbol.kind == SymbolKind::Class)
        .expect("expected case class symbol");

    let fields: Vec<_> = symbols
        .iter()
        .filter(|symbol| {
            symbol.kind == SymbolKind::Property
                && symbol.parent_id.as_deref() == Some(class_symbol.id.as_str())
        })
        .collect();
    assert_eq!(
        fields.len(),
        2,
        "expected case-class constructor fields as properties, got: {:?}",
        symbols
            .iter()
            .map(|symbol| (&symbol.name, &symbol.kind, &symbol.parent_id))
            .collect::<Vec<_>>()
    );

    let name = fields
        .iter()
        .find(|symbol| symbol.name == "name")
        .expect("missing name constructor field");
    let age = fields
        .iter()
        .find(|symbol| symbol.name == "age")
        .expect("missing age constructor field");

    assert_eq!(
        name.metadata
            .as_ref()
            .and_then(|metadata| metadata.get("propertyType"))
            .and_then(|value| value.as_str()),
        Some("String")
    );
    assert_eq!(
        age.metadata
            .as_ref()
            .and_then(|metadata| metadata.get("propertyType"))
            .and_then(|value| value.as_str()),
        Some("Int")
    );
    assert_eq!(name.signature.as_deref(), Some("val name: String"));
    assert_eq!(age.signature.as_deref(), Some("val age: Int"));
}

#[test]
fn test_scala_val_kind_depends_on_scope() {
    let code = r#"
val topLevel: Int = 1

class Example {
  val member: Int = 2

  def run(): Int = {
    val local: Int = 3
    local
  }
}

object Cache {
  val cached: Int = 4
}
"#;
    let symbols = extract_symbols(code);

    let top_level = symbols
        .iter()
        .find(|symbol| symbol.name == "topLevel")
        .expect("top-level val should be extracted");
    assert_ne!(
        top_level.kind,
        SymbolKind::Property,
        "top-level val must not be modeled as property"
    );

    let local = symbols
        .iter()
        .find(|symbol| symbol.name == "local")
        .expect("local val should be extracted");
    assert_ne!(
        local.kind,
        SymbolKind::Property,
        "local val must not be modeled as property"
    );

    let member = symbols
        .iter()
        .find(|symbol| symbol.name == "member")
        .expect("class member val should be extracted");
    assert_eq!(
        member.kind,
        SymbolKind::Property,
        "class member val should be modeled as property"
    );

    let cached = symbols
        .iter()
        .find(|symbol| symbol.name == "cached")
        .expect("object member val should be extracted");
    assert_eq!(
        cached.kind,
        SymbolKind::Property,
        "object member val should be modeled as property"
    );
}
