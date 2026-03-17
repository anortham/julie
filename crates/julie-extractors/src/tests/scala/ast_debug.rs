//! AST exploration tests for Scala (run with --ignored --nocapture)

use tree_sitter::Parser;

fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_scala::LANGUAGE.into())
        .expect("Error loading Scala grammar");
    parser
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

#[test]
#[ignore]
fn debug_scala_enum_ast() {
    let code = r#"
enum Color {
  case Red, Green, Blue
  case Custom(hex: String)
}
"#;
    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();
    debug_print_tree(tree.root_node(), code, 0);
}

#[test]
#[ignore]
fn debug_scala_extends_ast() {
    let code = r#"
sealed trait Animal {
  def speak(): String
}

case class Dog(name: String) extends Animal {
  override def speak(): String = "woof"
}

class Cat extends Animal with Serializable {
  def speak(): String = "meow"
}
"#;
    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();
    debug_print_tree(tree.root_node(), code, 0);
}

#[test]
#[ignore]
fn debug_scala_import_ast() {
    let code = r#"
import scala.collection.mutable.{ListBuffer => LB, _}
"#;
    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();
    debug_print_tree(tree.root_node(), code, 0);
}

#[test]
#[ignore]
fn debug_scala_package_ast() {
    let code = r#"
package com.example
"#;
    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();
    debug_print_tree(tree.root_node(), code, 0);
}
