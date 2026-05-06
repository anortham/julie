use crate::base::{Symbol, SymbolKind, SymbolOptions, Visibility};
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
                    symbols.extend(self.extract_var_spec_symbols(child, parent_id));
                }
                "var_spec_list" => {
                    let mut nested_cursor = child.walk();
                    for nested_child in child.children(&mut nested_cursor) {
                        if nested_child.kind() == "var_spec" {
                            symbols.extend(self.extract_var_spec_symbols(nested_child, parent_id));
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
                    symbols.extend(self.extract_const_spec_symbols(child, parent_id));
                }
                "const_spec_list" => {
                    let mut nested_cursor = child.walk();
                    for nested_child in child.children(&mut nested_cursor) {
                        if nested_child.kind() == "const_spec" {
                            symbols
                                .extend(self.extract_const_spec_symbols(nested_child, parent_id));
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
                    .next_back()?
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
                    annotations: Vec::new(),
                },
            ))
        } else {
            None
        }
    }

    pub(super) fn extract_var_spec_symbols(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Vec<Symbol> {
        let mut cursor = node.walk();
        let mut identifiers = Vec::new();
        let mut var_type = None;
        let mut values = Vec::new();

        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" => identifiers.push((self.get_node_text(child), child)),
                "type_identifier" | "primitive_type" | "pointer_type" | "slice_type"
                | "map_type" => {
                    var_type = Some(self.extract_type_from_node(child));
                }
                "expression_list" => {
                    values = self.extract_spec_values(child);
                }
                _ => {}
            }
        }

        let doc_comment = self.base.find_doc_comment(&node);
        identifiers
            .into_iter()
            .enumerate()
            .map(|(index, (name, _name_node))| {
                let visibility = if self.is_public(&name) {
                    Some(Visibility::Public)
                } else {
                    Some(Visibility::Private)
                };

                let value = values.get(index).or_else(|| values.first());
                let signature = if let Some(typ) = var_type.as_deref() {
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

                self.base.create_symbol(
                    &node,
                    name,
                    SymbolKind::Variable,
                    SymbolOptions {
                        signature: Some(signature),
                        visibility,
                        parent_id: parent_id.map(|s| s.to_string()),
                        metadata: None,
                        doc_comment: doc_comment.clone(),
                        annotations: Vec::new(),
                    },
                )
            })
            .collect()
    }

    pub(super) fn extract_const_spec_symbols(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Vec<Symbol> {
        let mut cursor = node.walk();
        let mut identifiers = Vec::new();
        let mut const_type = None;
        let mut values = Vec::new();

        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" => identifiers.push((self.get_node_text(child), child)),
                "type_identifier" | "primitive_type" => {
                    const_type = Some(self.extract_type_from_node(child));
                }
                "expression_list" => {
                    values = self.extract_spec_values(child);
                }
                _ if child.kind().starts_with("literal")
                    || matches!(child.kind(), "true" | "false" | "nil") =>
                {
                    values.push(self.get_node_text(child));
                }
                _ => {}
            }
        }

        let doc_comment = self.base.find_doc_comment(&node);
        identifiers
            .into_iter()
            .enumerate()
            .map(|(index, (name, _name_node))| {
                let visibility = if self.is_public(&name) {
                    Some(Visibility::Public)
                } else {
                    Some(Visibility::Private)
                };

                let value = values.get(index).or_else(|| values.first());
                let signature = if let Some(val) = value {
                    if let Some(typ) = const_type.as_deref() {
                        format!("const {} {} = {}", name, typ, val)
                    } else {
                        format!("const {} = {}", name, val)
                    }
                } else {
                    format!("const {}", name)
                };

                self.base.create_symbol(
                    &node,
                    name,
                    SymbolKind::Constant,
                    SymbolOptions {
                        signature: Some(signature),
                        visibility,
                        parent_id: parent_id.map(|s| s.to_string()),
                        metadata: None,
                        doc_comment: doc_comment.clone(),
                        annotations: Vec::new(),
                    },
                )
            })
            .collect()
    }

    fn extract_spec_values(&self, node: Node) -> Vec<String> {
        let mut values = Vec::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if !matches!(child.kind(), "," | " ") {
                values.push(self.get_node_text(child));
            }
        }
        values
    }
}
