use crate::base::{BaseExtractor, IdentifierKind, Symbol, SymbolKind};
use std::collections::HashMap;
use tree_sitter::Parser;

#[test]
fn test_identifier_kind_round_trips_supported_values() {
    assert_eq!(IdentifierKind::from_string("call"), IdentifierKind::Call);
    assert_eq!(
        IdentifierKind::from_string("variable_ref"),
        IdentifierKind::VariableRef
    );
    assert_eq!(
        IdentifierKind::from_string("type_usage"),
        IdentifierKind::TypeUsage
    );
    assert_eq!(
        IdentifierKind::from_string("member_access"),
        IdentifierKind::MemberAccess
    );
}

#[test]
fn test_identifier_kind_import_is_not_silently_coerced() {
    assert_eq!(IdentifierKind::try_from_string("import"), None);
}

#[test]
fn test_find_containing_symbol_from_map_filters_to_current_file_without_cloning_symbols() {
    let workspace_root = std::path::PathBuf::from("/workspace");
    let base = BaseExtractor::new(
        "rust".to_string(),
        "/workspace/src/lib.rs".to_string(),
        "fn outer() {\n    helper();\n}\n".to_string(),
        &workspace_root,
    );
    let current = symbol("current", "src/lib.rs", SymbolKind::Function, 1, 1, 3, 1);
    let other_file = symbol("other", "src/other.rs", SymbolKind::Function, 1, 1, 3, 1);
    let symbols = [current, other_file];
    let symbol_map: HashMap<String, &Symbol> = symbols
        .iter()
        .map(|symbol| (symbol.id.clone(), symbol))
        .collect();
    let mut parser = Parser::new();
    parser
        .set_language(&crate::language::get_tree_sitter_language("rust").unwrap())
        .unwrap();
    let tree = parser.parse(&base.content, None).unwrap();
    let call_node = find_node(tree.root_node(), "call_expression").unwrap();

    let containing = base
        .find_containing_symbol_from_map(&call_node, &symbol_map)
        .expect("call should be contained by the current file symbol");

    assert_eq!(containing.id, "current");
}

fn symbol(
    id: &str,
    file_path: &str,
    kind: SymbolKind,
    start_line: u32,
    start_column: u32,
    end_line: u32,
    end_column: u32,
) -> Symbol {
    Symbol {
        id: id.to_string(),
        name: id.to_string(),
        kind,
        language: "rust".to_string(),
        file_path: file_path.to_string(),
        start_line,
        start_column,
        end_line,
        end_column,
        start_byte: 0,
        end_byte: 100,
        signature: None,
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: None,
        content_type: None,
        annotations: Vec::new(),
    }
}

fn find_node<'a>(node: tree_sitter::Node<'a>, kind: &str) -> Option<tree_sitter::Node<'a>> {
    if node.kind() == kind {
        return Some(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(found) = find_node(child, kind) {
            return Some(found);
        }
    }
    None
}
