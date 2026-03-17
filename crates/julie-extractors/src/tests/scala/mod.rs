//! Tests for Scala extractor

mod ast_debug;

use crate::base::SymbolKind;
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

fn extract_symbols(code: &str) -> Vec<crate::base::Symbol> {
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

// ========================================================================
// AST Exploration (run with --nocapture to see output)
// ========================================================================

#[test]
#[ignore]
fn debug_scala_ast() {
    let code = r#"
package com.example

import scala.collection.mutable.ListBuffer

sealed trait Animal {
  def speak(): String
}

case class Dog(name: String) extends Animal {
  override def speak(): String = s"Woof! I'm $name"
}

object DogFactory {
  def create(name: String): Dog = Dog(name)
}

abstract class Shape(val sides: Int) {
  def area(): Double
}

val pi: Double = 3.14159
var count: Int = 0
type StringList = List[String]
"#;
    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();
    debug_print_tree(tree.root_node(), code, 0);
}

fn debug_print_tree(node: tree_sitter::Node, source: &str, depth: usize) {
    let indent = "  ".repeat(depth);
    let text = node.utf8_text(source.as_bytes()).unwrap_or("<err>");
    let short = if text.len() > 60 {
        &text[..57]
    } else {
        text
    };
    println!(
        "{}{} [{}]: {}",
        indent,
        node.kind(),
        if node.is_named() { "named" } else { "anon" },
        short.replace('\n', "\\n")
    );
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        debug_print_tree(child, source, depth + 1);
    }
}

// ========================================================================
// Basic Symbol Extraction Tests
// ========================================================================

#[test]
fn test_scala_trait_extraction() {
    let code = r#"
sealed trait Animal {
  def speak(): String
}
"#;
    let symbols = extract_symbols(code);
    let traits: Vec<_> = symbols.iter().filter(|s| s.kind == SymbolKind::Trait).collect();
    assert_eq!(traits.len(), 1, "Expected 1 trait, found {:?}", traits);
    assert_eq!(traits[0].name, "Animal");
    assert!(
        traits[0].signature.as_ref().unwrap().contains("sealed"),
        "Signature should contain 'sealed': {:?}",
        traits[0].signature
    );
}

#[test]
fn test_scala_class_extraction() {
    let code = r#"
case class Dog(name: String) extends Animal {
  override def speak(): String = "woof"
}
"#;
    let symbols = extract_symbols(code);
    let classes: Vec<_> = symbols.iter().filter(|s| s.kind == SymbolKind::Class).collect();
    assert!(
        !classes.is_empty(),
        "Expected at least 1 class, got: {:?}",
        symbols.iter().map(|s| (&s.name, &s.kind)).collect::<Vec<_>>()
    );
    let dog = classes.iter().find(|s| s.name == "Dog");
    assert!(dog.is_some(), "Expected Dog class");
}

#[test]
fn test_scala_object_extraction() {
    let code = r#"
object DogFactory {
  def create(name: String): Dog = Dog(name)
}
"#;
    let symbols = extract_symbols(code);
    let objects: Vec<_> = symbols
        .iter()
        .filter(|s| {
            s.kind == SymbolKind::Class
                && s.metadata
                    .as_ref()
                    .and_then(|m| m.get("type"))
                    .and_then(|v| v.as_str())
                    == Some("object")
        })
        .collect();
    assert_eq!(objects.len(), 1, "Expected 1 object");
    assert_eq!(objects[0].name, "DogFactory");
}

#[test]
fn test_scala_function_extraction() {
    let code = r#"
sealed trait Animal {
  def speak(): String
}
"#;
    let symbols = extract_symbols(code);
    let methods: Vec<_> = symbols
        .iter()
        .filter(|s| matches!(s.kind, SymbolKind::Method | SymbolKind::Function))
        .collect();
    assert!(
        !methods.is_empty(),
        "Expected at least 1 method, got: {:?}",
        symbols.iter().map(|s| (&s.name, &s.kind)).collect::<Vec<_>>()
    );
    let speak = methods.iter().find(|s| s.name == "speak");
    assert!(speak.is_some(), "Expected 'speak' method");
}

#[test]
fn test_scala_val_extraction() {
    let code = r#"
val pi: Double = 3.14159
"#;
    let symbols = extract_symbols(code);
    let vals: Vec<_> = symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Constant)
        .collect();
    assert_eq!(vals.len(), 1, "Expected 1 val, got: {:?}",
        symbols.iter().map(|s| (&s.name, &s.kind)).collect::<Vec<_>>());
    assert_eq!(vals[0].name, "pi");
}

#[test]
fn test_scala_var_extraction() {
    let code = r#"
var count: Int = 0
"#;
    let symbols = extract_symbols(code);
    let vars: Vec<_> = symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Variable)
        .collect();
    assert_eq!(vars.len(), 1, "Expected 1 var, got: {:?}",
        symbols.iter().map(|s| (&s.name, &s.kind)).collect::<Vec<_>>());
    assert_eq!(vars[0].name, "count");
}

#[test]
fn test_scala_import_extraction() {
    let code = r#"
import scala.collection.mutable.ListBuffer
"#;
    let symbols = extract_symbols(code);
    let imports: Vec<_> = symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Import)
        .collect();
    assert_eq!(imports.len(), 1, "Expected 1 import, got: {:?}",
        symbols.iter().map(|s| (&s.name, &s.kind)).collect::<Vec<_>>());
}

#[test]
fn test_scala_package_extraction() {
    let code = r#"
package com.example
"#;
    let symbols = extract_symbols(code);
    let packages: Vec<_> = symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Namespace)
        .collect();
    assert_eq!(packages.len(), 1, "Expected 1 package, got: {:?}",
        symbols.iter().map(|s| (&s.name, &s.kind)).collect::<Vec<_>>());
}

#[test]
fn test_scala_type_alias_extraction() {
    let code = r#"
type StringList = List[String]
"#;
    let symbols = extract_symbols(code);
    let type_aliases: Vec<_> = symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Type)
        .collect();
    assert_eq!(type_aliases.len(), 1, "Expected 1 type alias, got: {:?}",
        symbols.iter().map(|s| (&s.name, &s.kind)).collect::<Vec<_>>());
    assert_eq!(type_aliases[0].name, "StringList");
}

#[test]
fn test_scala_abstract_class_extraction() {
    let code = r#"
abstract class Shape(val sides: Int) {
  def area(): Double
}
"#;
    let symbols = extract_symbols(code);
    let classes: Vec<_> = symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Class)
        .collect();
    assert!(
        !classes.is_empty(),
        "Expected at least 1 class, got: {:?}",
        symbols.iter().map(|s| (&s.name, &s.kind)).collect::<Vec<_>>()
    );
    let shape = classes.iter().find(|s| s.name == "Shape");
    assert!(shape.is_some(), "Expected Shape class");
    let shape = shape.unwrap();
    assert!(
        shape.signature.as_ref().unwrap().contains("abstract"),
        "Signature should contain 'abstract': {:?}",
        shape.signature
    );
}

#[test]
fn test_scala_companion_object() {
    let code = r#"
case class Dog(name: String)

object Dog {
  def apply(name: String): Dog = new Dog(name)
}
"#;
    let symbols = extract_symbols(code);
    let dog_objects: Vec<_> = symbols
        .iter()
        .filter(|s| {
            s.name == "Dog"
                && s.metadata
                    .as_ref()
                    .and_then(|m| m.get("type"))
                    .and_then(|v| v.as_str())
                    == Some("object")
        })
        .collect();
    assert_eq!(dog_objects.len(), 1, "Expected 1 Dog object");
    let companion = dog_objects[0]
        .metadata
        .as_ref()
        .and_then(|m| m.get("companion"))
        .and_then(|v| v.as_bool());
    assert_eq!(companion, Some(true), "Dog object should be marked as companion");
}

#[test]
fn test_scala_method_vs_function() {
    let code = r#"
def topLevel(): Unit = ()

class Foo {
  def method(): Unit = ()
}
"#;
    let symbols = extract_symbols(code);

    let top_level = symbols.iter().find(|s| s.name == "topLevel");
    assert!(top_level.is_some(), "Expected topLevel function");
    assert_eq!(top_level.unwrap().kind, SymbolKind::Function);

    let method = symbols.iter().find(|s| s.name == "method");
    assert!(method.is_some(), "Expected method");
    assert_eq!(method.unwrap().kind, SymbolKind::Method);
}

// ========================================================================
// Enum Tests
// ========================================================================

#[test]
fn test_scala_enum_extraction() {
    let code = r#"
enum Color {
  case Red, Green, Blue
  case Custom(hex: String)
}
"#;
    let symbols = extract_symbols(code);

    let enums: Vec<_> = symbols.iter().filter(|s| s.kind == SymbolKind::Enum).collect();
    assert_eq!(enums.len(), 1, "Expected 1 enum");
    assert_eq!(enums[0].name, "Color");

    let members: Vec<_> = symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::EnumMember)
        .collect();
    let member_names: Vec<_> = members.iter().map(|s| s.name.as_str()).collect();
    assert!(
        member_names.contains(&"Red"),
        "Missing Red, got: {:?}",
        member_names
    );
    assert!(
        member_names.contains(&"Green"),
        "Missing Green, got: {:?}",
        member_names
    );
    assert!(
        member_names.contains(&"Blue"),
        "Missing Blue, got: {:?}",
        member_names
    );
    assert!(
        member_names.contains(&"Custom"),
        "Missing Custom, got: {:?}",
        member_names
    );
}

// ========================================================================
// Case Class Signature Tests
// ========================================================================

#[test]
fn test_scala_case_class_signature() {
    let code = r#"
case class Dog(name: String) extends Animal {
  override def speak(): String = "woof"
}
"#;
    let symbols = extract_symbols(code);
    let dog = symbols.iter().find(|s| s.name == "Dog").unwrap();
    let sig = dog.signature.as_ref().unwrap();
    assert!(sig.contains("case"), "Signature should contain 'case': {}", sig);
    assert!(sig.contains("class Dog"), "Signature should contain 'class Dog': {}", sig);
    assert!(
        sig.contains("extends"),
        "Signature should contain 'extends': {}",
        sig
    );
}

// ========================================================================
// Return Type and Type Inference Tests
// ========================================================================

#[test]
fn test_scala_return_type_in_metadata() {
    let code = r#"
def greet(name: String): String = "hello"
"#;
    let symbols = extract_symbols(code);
    let greet = symbols.iter().find(|s| s.name == "greet").unwrap();
    let return_type = greet
        .metadata
        .as_ref()
        .and_then(|m| m.get("returnType"))
        .and_then(|v| v.as_str());
    assert_eq!(return_type, Some("String"), "Expected returnType metadata");
}

#[test]
fn test_scala_type_inference() {
    let code = r#"
def add(a: Int, b: Int): Int = a + b
val pi: Double = 3.14
"#;
    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = ScalaExtractor::new(
        "scala".to_string(),
        "test.scala".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let types = extractor.infer_types(&symbols);

    // Function with return type should be inferred
    let add_sym = symbols.iter().find(|s| s.name == "add").unwrap();
    assert_eq!(types.get(&add_sym.id).map(|s| s.as_str()), Some("Int"));

    // Val with type annotation should be inferred
    let pi_sym = symbols.iter().find(|s| s.name == "pi").unwrap();
    assert_eq!(types.get(&pi_sym.id).map(|s| s.as_str()), Some("Double"));
}

// ========================================================================
// Relationship Tests
// ========================================================================

#[test]
fn test_scala_inheritance_relationships() {
    let code = r#"
trait Animal {
  def speak(): String
}

case class Dog(name: String) extends Animal {
  def speak(): String = "woof"
}
"#;
    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = ScalaExtractor::new(
        "scala".to_string(),
        "test.scala".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);

    // Dog extends Animal — since Animal is a Trait, this should be Implements
    assert!(
        !relationships.is_empty(),
        "Expected at least 1 relationship, got 0. Symbols: {:?}",
        symbols.iter().map(|s| (&s.name, &s.kind)).collect::<Vec<_>>()
    );

    let dog_rel = relationships.iter().find(|r| {
        let from = symbols.iter().find(|s| s.id == r.from_symbol_id);
        from.is_some_and(|s| s.name == "Dog")
    });
    assert!(
        dog_rel.is_some(),
        "Expected Dog->Animal relationship. Relationships: {:?}",
        relationships
    );
}

// ========================================================================
// Identifier Tests
// ========================================================================

#[test]
fn test_scala_identifier_extraction() {
    let code = r#"
object Main {
  def greet(name: String): String = name.toUpperCase()

  def main(): Unit = {
    greet("world")
  }
}
"#;
    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = ScalaExtractor::new(
        "scala".to_string(),
        "test.scala".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let identifiers = extractor.extract_identifiers(&tree, &symbols);

    // Should find call identifiers
    let call_idents: Vec<_> = identifiers
        .iter()
        .filter(|i| i.kind == crate::base::IdentifierKind::Call)
        .collect();
    assert!(
        !call_idents.is_empty(),
        "Expected call identifiers, got none. All identifiers: {:?}",
        identifiers.iter().map(|i| (&i.name, &i.kind)).collect::<Vec<_>>()
    );
}

// ========================================================================
// Visibility Tests
// ========================================================================

#[test]
fn test_scala_visibility() {
    let code = r#"
class Foo {
  private def secret(): Unit = ()
  protected def familyOnly(): Unit = ()
  def public(): Unit = ()
}
"#;
    let symbols = extract_symbols(code);

    let secret = symbols.iter().find(|s| s.name == "secret");
    assert!(secret.is_some(), "Expected 'secret' method");
    assert_eq!(
        secret.unwrap().visibility,
        Some(crate::base::Visibility::Private)
    );

    let family = symbols.iter().find(|s| s.name == "familyOnly");
    assert!(family.is_some(), "Expected 'familyOnly' method");
    assert_eq!(
        family.unwrap().visibility,
        Some(crate::base::Visibility::Protected)
    );

    let public = symbols.iter().find(|s| s.name == "public");
    assert!(public.is_some(), "Expected 'public' method");
    assert_eq!(
        public.unwrap().visibility,
        Some(crate::base::Visibility::Public)
    );
}

// ========================================================================
// Full Fixture Test
// ========================================================================

#[test]
fn test_scala_full_fixture() {
    let code = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("fixtures/scala/basic.scala"),
    )
    .unwrap();

    let mut parser = init_parser();
    let tree = parser.parse(&code, None).unwrap();
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = ScalaExtractor::new(
        "scala".to_string(),
        "basic.scala".to_string(),
        code.clone(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);

    // Verify we extract a reasonable number of symbols
    assert!(
        symbols.len() >= 10,
        "Expected at least 10 symbols from fixture, got {}",
        symbols.len()
    );

    // Check specific symbol kinds are present
    let has_trait = symbols.iter().any(|s| s.kind == SymbolKind::Trait);
    let has_class = symbols.iter().any(|s| s.kind == SymbolKind::Class);
    let has_method = symbols.iter().any(|s| s.kind == SymbolKind::Method);
    let has_val = symbols.iter().any(|s| s.kind == SymbolKind::Constant);
    let has_var = symbols.iter().any(|s| s.kind == SymbolKind::Variable);
    let has_type_alias = symbols.iter().any(|s| s.kind == SymbolKind::Type);
    let has_import = symbols.iter().any(|s| s.kind == SymbolKind::Import);
    let has_package = symbols.iter().any(|s| s.kind == SymbolKind::Namespace);

    assert!(has_trait, "Missing trait");
    assert!(has_class, "Missing class");
    assert!(has_method, "Missing method");
    assert!(has_val, "Missing val/constant");
    assert!(has_var, "Missing var");
    assert!(has_type_alias, "Missing type alias");
    assert!(has_import, "Missing import");
    assert!(has_package, "Missing package");
}
