/// Import statement extraction
/// Handles import, from...import, and aliased imports
use super::super::base::{Symbol, SymbolKind, SymbolOptions, Visibility};
use super::PythonExtractor;
use tree_sitter::Node;

/// Extract imports from an import or import_from statement
pub fn extract_imports(extractor: &mut PythonExtractor, node: Node) -> Vec<Symbol> {
    let mut imports = Vec::new();

    match node.kind() {
        "import_statement" => {
            // Handle single import: import module [as alias]
            if let Some(import_symbol) = extract_single_import(extractor, &node) {
                imports.push(import_symbol);
            }
        }
        "import_from_statement" => {
            // Handle from import: from module import name1, name2, name3
            if let Some(module_node) = node.child_by_field_name("module_name") {
                let module = extractor.base_mut().get_node_text(&module_node);

                // Find all import names after the 'import' keyword
                let mut found_import_keyword = false;
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "import" {
                        found_import_keyword = true;
                        continue;
                    }

                    if found_import_keyword {
                        match child.kind() {
                            "dotted_name" => {
                                // Simple import: from module import name
                                let name = extractor.base_mut().get_node_text(&child);
                                let import_text = format!("from {} import {}", module, name);
                                let symbol =
                                    create_import_symbol(extractor, &node, name, import_text);
                                imports.push(symbol);
                            }
                            "aliased_import" => {
                                // Aliased import: from module import name as alias
                                if let Some((name, alias)) = extract_alias(extractor, &child) {
                                    let import_text =
                                        format!("from {} import {} as {}", module, name, alias);
                                    let symbol =
                                        create_import_symbol(extractor, &node, alias, import_text);
                                    imports.push(symbol);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        _ => {}
    }

    imports
}

/// Extract a single import statement
fn extract_single_import(extractor: &mut PythonExtractor, node: &Node) -> Option<Symbol> {
    let mut import_text = String::new();
    let mut name = String::new();

    // Check for aliased_import child
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "aliased_import" {
            // import module as alias
            if let Some((module_name, alias)) = extract_alias(extractor, &child) {
                import_text = format!("import {} as {}", module_name, alias);
                name = alias; // Use alias as the symbol name
            }
            break;
        } else if child.kind() == "dotted_name" {
            // Simple import: import module
            name = extractor.base_mut().get_node_text(&child);
            import_text = format!("import {}", name);
            break;
        }
    }

    if !name.is_empty() {
        Some(create_import_symbol(extractor, node, name, import_text))
    } else {
        None
    }
}

/// Extract alias from an aliased_import node
fn extract_alias(extractor: &PythonExtractor, node: &Node) -> Option<(String, String)> {
    // Extract "name as alias" pattern
    let mut name = String::new();
    let mut alias = String::new();

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "dotted_name" || child.kind() == "identifier" {
            if name.is_empty() {
                name = extractor.base().get_node_text(&child);
            } else {
                // Second name-like node is the alias
                alias = extractor.base().get_node_text(&child);
                break;
            }
        }
    }

    if !name.is_empty() && !alias.is_empty() {
        Some((name, alias))
    } else {
        None
    }
}

/// Create an import symbol
fn create_import_symbol(
    extractor: &mut PythonExtractor,
    node: &Node,
    name: String,
    signature: String,
) -> Symbol {
    // Extract doc comment (preceding comments)
    let doc_comment = extractor.base().find_doc_comment(node);

    extractor.base_mut().create_symbol(
        node,
        name,
        SymbolKind::Import,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: None,
            metadata: None,
            doc_comment,
        },
    )
}
