// Swift language extractor - comprehensive symbol and relationship extraction
// Organized into focused modules for maintainability and clarity

pub(super) mod callables;
pub(super) mod enum_cases;
pub(super) mod extensions;
pub(super) mod identifiers;
pub(super) mod properties;
pub(super) mod protocol;
pub(super) mod relationships;
pub(super) mod signatures;
pub(super) mod types;

use crate::extractors::base::{BaseExtractor, Symbol};
use tree_sitter::{Node, Tree};

/// Swift extractor for extracting symbols and relationships from Swift source code
/// Implementation of comprehensive Swift extractor with full Swift language support
pub struct SwiftExtractor {
    base: BaseExtractor,
}

impl SwiftExtractor {
    pub fn new(
        language: String,
        file_path: String,
        content: String,
        workspace_root: &std::path::Path,
    ) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content, workspace_root),
        }
    }

    /// Extract all symbols from Swift source code
    /// Implementation of extractSymbols method with comprehensive Swift support
    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.visit_node(tree.root_node(), &mut symbols, None);
        symbols
    }

    fn visit_node(&mut self, node: Node, symbols: &mut Vec<Symbol>, parent_id: Option<String>) {
        if !node.is_named() {
            return;
        }

        let mut symbol: Option<Symbol> = None;
        let mut current_parent_id = parent_id.clone();

        match node.kind() {
            "class_declaration" => {
                symbol = Some(self.extract_class(node, parent_id.as_deref()));
            }
            "struct_declaration" => {
                symbol = Some(self.extract_struct(node, parent_id.as_deref()));
            }
            "protocol_declaration" => {
                symbol = Some(self.extract_protocol(node, parent_id.as_deref()));
            }
            "enum_declaration" => {
                symbol = Some(self.extract_enum(node, parent_id.as_deref()));
            }
            "enum_case_declaration" => {
                self.extract_enum_cases(node, symbols, parent_id.as_deref());
            }
            "enum_entry" => {
                symbol = Some(self.extract_enum_case(node, parent_id.as_deref()));
            }
            "function_declaration" => {
                symbol = Some(self.extract_function(node, parent_id.as_deref()));
            }
            "protocol_function_declaration" => {
                symbol = Some(self.extract_protocol_function(node, parent_id.as_deref()));
            }
            "protocol_property_declaration" => {
                symbol = Some(self.extract_protocol_property(node, parent_id.as_deref()));
            }
            "associatedtype_declaration" => {
                symbol = Some(self.extract_associated_type(node, parent_id.as_deref()));
            }
            "subscript_declaration" => {
                symbol = Some(self.extract_subscript(node, parent_id.as_deref()));
            }
            "init_declaration" => {
                symbol = Some(self.extract_initializer(node, parent_id.as_deref()));
            }
            "deinit_declaration" => {
                symbol = Some(self.extract_deinitializer(node, parent_id.as_deref()));
            }
            "variable_declaration" => {
                if let Some(var_symbol) = self.extract_variable(node, parent_id.as_deref()) {
                    symbol = Some(var_symbol);
                }
            }
            "property_declaration" => {
                symbol = Some(self.extract_property(node, parent_id.as_deref()));
            }
            "extension_declaration" => {
                symbol = Some(self.extract_extension(node, parent_id.as_deref()));
            }
            "import_declaration" => {
                symbol = Some(self.extract_import(node, parent_id.as_deref()));
            }
            "typealias_declaration" => {
                symbol = Some(self.extract_type_alias(node, parent_id.as_deref()));
            }
            _ => {}
        }

        if let Some(ref sym) = symbol {
            symbols.push(sym.clone());
            current_parent_id = Some(sym.id.clone());
        }

        // Recursively visit children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node(child, symbols, current_parent_id.clone());
        }
    }
}
