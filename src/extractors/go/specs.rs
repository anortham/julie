use crate::extractors::base::{Symbol, SymbolKind, SymbolOptions, Visibility};
use tree_sitter::Node;

/// Extraction of import, variable, and constant specifications
impl super::GoExtractor {
    pub(super) fn extract_import_symbols(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            match child.kind() {
                "import_spec" => {
                    if let Some(symbol) = self.extract_import_spec(child, parent_id) {
                        symbols.push(symbol);
                    }
                }
                "import_spec_list" => {
                    let mut nested_cursor = child.walk();
                    for nested_child in child.children(&mut nested_cursor) {
                        if nested_child.kind() == "import_spec" {
                            if let Some(symbol) = self.extract_import_spec(nested_child, parent_id)
                            {
                                symbols.push(symbol);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        symbols
    }

    pub(super) fn extract_var_symbols(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            match child.kind() {
                "var_spec" => {
                    if let Some(symbol) = self.extract_var_spec(child, parent_id) {
                        symbols.push(symbol);
                    }
                }
                "var_spec_list" => {
                    let mut nested_cursor = child.walk();
                    for nested_child in child.children(&mut nested_cursor) {
                        if nested_child.kind() == "var_spec" {
                            if let Some(symbol) = self.extract_var_spec(nested_child, parent_id) {
                                symbols.push(symbol);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        symbols
    }

    pub(super) fn extract_const_symbols(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            match child.kind() {
                "const_spec" => {
                    if let Some(symbol) = self.extract_const_spec(child, parent_id) {
                        symbols.push(symbol);
                    }
                }
                "const_spec_list" => {
                    let mut nested_cursor = child.walk();
                    for nested_child in child.children(&mut nested_cursor) {
                        if nested_child.kind() == "const_spec" {
                            if let Some(symbol) = self.extract_const_spec(nested_child, parent_id) {
                                symbols.push(symbol);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        symbols
    }

    pub(super) fn extract_import_spec(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        let mut cursor = node.walk();
        let mut alias = None;
        let mut path = None;

        for child in node.children(&mut cursor) {
            match child.kind() {
                "package_identifier" => alias = Some(self.get_node_text(child)), // Uses package_identifier for alias
                "interpreted_string_literal" => path = Some(self.get_node_text(child)),
                _ => {}
            }
        }

        if let Some(import_path) = path {
            // Skip blank imports (_)
            if alias.as_deref() == Some("_") {
                return None;
            }

            // Extract package name from path
            let package_name = if let Some(ref a) = alias {
                a.clone()
            } else {
                // Extract package name from import path
                import_path
                    .trim_matches('"')
                    .split('/')
                    .next_back()
                    .unwrap_or("unknown")
                    .to_string()
            };

            let signature = if let Some(ref a) = alias {
                format!("import {} {}", a, import_path)
            } else {
                format!("import {}", import_path)
            };

            let doc_comment = self.base.find_doc_comment(&node);

            Some(self.base.create_symbol(
                &node,
                package_name,
                SymbolKind::Import,
                SymbolOptions {
                    signature: Some(signature),
                    visibility: Some(Visibility::Public),
                    parent_id: parent_id.map(|s| s.to_string()),
                    metadata: None,
                    doc_comment,
                },
            ))
        } else {
            None
        }
    }

    pub(super) fn extract_var_spec(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        let mut cursor = node.walk();
        let mut identifier = None;
        let mut var_type = None;
        let mut value = None;

        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" => identifier = Some(self.get_node_text(child)),
                "type_identifier" | "primitive_type" | "pointer_type" | "slice_type"
                | "map_type" => {
                    var_type = Some(self.extract_type_from_node(child));
                }
                "expression_list" => {
                    // Extract the first expression as the value
                    let mut expr_cursor = child.walk();
                    for expr_child in child.children(&mut expr_cursor) {
                        if !matches!(expr_child.kind(), "," | " ") {
                            value = Some(self.get_node_text(expr_child));
                            break;
                        }
                    }
                }
                _ => {}
            }
        }

        if let Some(name) = identifier {
            let visibility = if self.is_public(&name) {
                Some(Visibility::Public)
            } else {
                Some(Visibility::Private)
            };

            let signature = if let Some(typ) = var_type {
                if let Some(val) = value {
                    format!("var {} {} = {}", name, typ, val)
                } else {
                    format!("var {} {}", name, typ)
                }
            } else if let Some(val) = value {
                format!("var {} = {}", name, val)
            } else {
                format!("var {}", name)
            };

            let doc_comment = self.base.find_doc_comment(&node);

            Some(self.base.create_symbol(
                &node,
                name,
                SymbolKind::Variable,
                SymbolOptions {
                    signature: Some(signature),
                    visibility,
                    parent_id: parent_id.map(|s| s.to_string()),
                    metadata: None,
                    doc_comment,
                },
            ))
        } else {
            None
        }
    }

    pub(super) fn extract_const_spec(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        let mut cursor = node.walk();
        let mut identifier = None;
        let mut const_type = None;
        let mut value = None;

        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" => identifier = Some(self.get_node_text(child)),
                "type_identifier" | "primitive_type" => {
                    const_type = Some(self.extract_type_from_node(child));
                }
                "expression_list" => {
                    // Extract the first expression as the value
                    let mut expr_cursor = child.walk();
                    for expr_child in child.children(&mut expr_cursor) {
                        if !matches!(expr_child.kind(), "," | " ") {
                            value = Some(self.get_node_text(expr_child));
                            break;
                        }
                    }
                }
                _ if child.kind().starts_with("literal")
                    || matches!(child.kind(), "true" | "false" | "nil") =>
                {
                    value = Some(self.get_node_text(child));
                }
                _ => {}
            }
        }

        if let Some(name) = identifier {
            let visibility = if self.is_public(&name) {
                Some(Visibility::Public)
            } else {
                Some(Visibility::Private)
            };

            let signature = if let Some(val) = value {
                if let Some(typ) = const_type {
                    format!("const {} {} = {}", name, typ, val)
                } else {
                    format!("const {} = {}", name, val)
                }
            } else {
                format!("const {}", name)
            };

            let doc_comment = self.base.find_doc_comment(&node);

            Some(self.base.create_symbol(
                &node,
                name,
                SymbolKind::Constant,
                SymbolOptions {
                    signature: Some(signature),
                    visibility,
                    parent_id: parent_id.map(|s| s.to_string()),
                    metadata: None,
                    doc_comment,
                },
            ))
        } else {
            None
        }
    }
}
