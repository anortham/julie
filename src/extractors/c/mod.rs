//! C language symbol extractor
//!
//! Provides symbol extraction, relationship tracking, and identifier discovery for C code
//! using tree-sitter parsing. This module is organized into focused submodules:
//!
//! - `helpers` - Node finding, name extraction, and tree navigation utilities
//! - `signatures` - Signature building methods for various C constructs
//! - `types` - Type and attribute extraction from the syntax tree
//! - `declarations` - Extraction of declarations (includes, macros, functions, variables, typedefs, structs, enums)
//! - `relationships` - Relationship extraction (calls, imports)
//! - `identifiers` - Identifier usage tracking (calls, member access)

use crate::extractors::base::{BaseExtractor, Identifier, Relationship, Symbol};
use tree_sitter::Tree;

// Internal modules
mod declarations;
mod helpers;
mod identifiers;
mod relationships;
mod signatures;
mod types;

/// Main C extractor struct combining all extraction functionality
pub struct CExtractor {
    base: BaseExtractor,
}

impl CExtractor {
    /// Create a new C extractor for the given file
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

    /// Extract all symbols from the syntax tree
    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.visit_node(tree.root_node(), &mut symbols, None);

        // Post-process: Fix function pointer typedef names and struct alignment attributes
        declarations::fix_function_pointer_typedef_names(&mut symbols);
        declarations::fix_struct_alignment_attributes(&mut symbols);

        symbols
    }

    /// Extract all relationships (calls, imports) from the syntax tree
    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        let mut relationships = Vec::new();
        relationships::extract_relationships_from_node(
            self,
            tree.root_node(),
            symbols,
            &mut relationships,
        );
        relationships
    }

    /// Extract all identifier usages (function calls, member access, etc.)
    pub fn extract_identifiers(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Identifier> {
        identifiers::extract_identifiers(self, tree, symbols)
    }

    /// Infer types from C signatures (function return types, variable types)
    pub fn infer_types(&self, symbols: &[Symbol]) -> std::collections::HashMap<String, String> {
        let mut type_map = std::collections::HashMap::new();

        for symbol in symbols {
            if let Some(ref signature) = symbol.signature {
                if let Some(inferred_type) =
                    self.extract_type_from_signature(signature, &symbol.kind, &symbol.name)
                {
                    type_map.insert(symbol.id.clone(), inferred_type);
                }
            }
        }

        type_map
    }

    fn extract_type_from_signature(
        &self,
        signature: &str,
        kind: &crate::extractors::base::SymbolKind,
        name: &str,
    ) -> Option<String> {
        use crate::extractors::base::SymbolKind;

        match kind {
            SymbolKind::Function | SymbolKind::Method => {
                // C function signatures: "int get_count()", "char* get_name()"
                // Extract return type (everything before function name)
                if let Some(name_pos) = signature.find(name) {
                    let type_part = signature[..name_pos].trim();
                    if !type_part.is_empty() {
                        return Some(type_part.to_string());
                    }
                }
            }
            SymbolKind::Variable | SymbolKind::Property => {
                // C variable declarations: "int count", "char* name"
                // Extract type (everything before variable name)
                if let Some(name_pos) = signature.find(name) {
                    let type_part = signature[..name_pos].trim();
                    if !type_part.is_empty() {
                        return Some(type_part.to_string());
                    }
                }
            }
            _ => {}
        }

        None
    }

    /// Recursively visit nodes in the tree, extracting symbols
    fn visit_node(
        &mut self,
        node: tree_sitter::Node,
        symbols: &mut Vec<Symbol>,
        parent_id: Option<String>,
    ) {
        if !node.is_named() {
            return;
        }

        let mut symbol: Option<Symbol> = None;

        // Port switch statement logic for C constructs
        match node.kind() {
            "preproc_include" => {
                symbol = Some(declarations::extract_include(
                    self,
                    node,
                    parent_id.as_deref(),
                ));
            }
            "preproc_def" | "preproc_function_def" => {
                symbol = Some(declarations::extract_macro(
                    self,
                    node,
                    parent_id.as_deref(),
                ));
            }
            "declaration" => {
                let declaration_symbols =
                    declarations::extract_declaration(self, node, parent_id.as_deref());
                symbols.extend(declaration_symbols);
            }
            "function_definition" => {
                symbol = Some(declarations::extract_function_definition(
                    self,
                    node,
                    parent_id.as_deref(),
                ));
            }
            "struct_specifier" => {
                symbol = Some(declarations::extract_struct(
                    self,
                    node,
                    parent_id.as_deref(),
                ));
            }
            "enum_specifier" => {
                symbol = Some(declarations::extract_enum(self, node, parent_id.as_deref()));
                // Also extract enum values as separate constants
                if let Some(ref enum_symbol) = symbol {
                    let enum_values =
                        declarations::extract_enum_value_symbols(self, node, &enum_symbol.id);
                    symbols.extend(enum_values);
                }
            }
            "type_definition" => {
                symbol = Some(declarations::extract_type_definition(
                    self,
                    node,
                    parent_id.as_deref(),
                ));
            }
            "linkage_specification" => {
                symbol =
                    declarations::extract_linkage_specification(self, node, parent_id.as_deref());
            }
            "expression_statement" => {
                // Handle cases like "} PACKED NetworkHeader;" where NetworkHeader is in expression_statement
                symbol = declarations::extract_from_expression_statement(
                    self,
                    node,
                    parent_id.as_deref(),
                );
            }
            _ => {}
        }

        let current_parent_id = if let Some(sym) = symbol {
            let symbol_id = sym.id.clone();
            symbols.push(sym);
            Some(symbol_id)
        } else {
            parent_id
        };

        // Recursively visit children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node(child, symbols, current_parent_id.clone());
        }
    }
}
