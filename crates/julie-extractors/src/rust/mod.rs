/// Rust language extractor with support for:
/// - Structs, enums, traits, unions
/// - Functions, methods, impl blocks
/// - Modules, macros, type aliases
/// - Constants, statics
/// - Two-phase processing: extract symbols → process impl blocks
///
/// Implementation of comprehensive Rust extractor
use crate::base::{
    BaseExtractor, Identifier, PendingRelationship, Relationship, Symbol, SymbolKind,
};
use regex::Regex;
use std::sync::LazyLock;
use tree_sitter::{Node, Tree};

/// Matches return type in function signatures: `-> ReturnType`
static RETURN_TYPE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"->\s*([^{]+)").unwrap());

/// Matches type annotations: `: Type`
static VAR_TYPE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":\s*([^=\s{]+)").unwrap());

// Private modules
mod functions;
mod helpers;
mod identifiers;
mod relationships;
mod signatures;
mod types;

// Re-export types
pub use self::helpers::ImplBlockInfo;

// Use helpers in the orchestrator
use self::helpers::is_inside_impl;

/// Rust extractor that handles Rust-specific constructs
pub struct RustExtractor {
    base: BaseExtractor,
    impl_blocks: Vec<ImplBlockInfo>,
    is_processing_impl_blocks: bool,
    /// Pending relationships that need cross-file resolution after workspace indexing
    pending_relationships: Vec<PendingRelationship>,
}

impl RustExtractor {
    pub fn new(
        language: String,
        file_path: String,
        content: String,
        workspace_root: &std::path::Path,
    ) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content, workspace_root),
            impl_blocks: Vec::new(),
            is_processing_impl_blocks: false,
            pending_relationships: Vec::new(),
        }
    }

    /// Get pending relationships that need cross-file resolution
    pub fn get_pending_relationships(&self) -> Vec<PendingRelationship> {
        self.pending_relationships.clone()
    }

    /// Add a pending relationship (used during extraction)
    pub fn add_pending_relationship(&mut self, pending: PendingRelationship) {
        self.pending_relationships.push(pending);
    }

    /// Extract symbols using two-phase approach
    /// Phase 1: Extract all symbols except methods in impl blocks
    /// Phase 2: Process impl blocks and link methods to parent structs/traits
    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();

        // Phase 1: Extract symbols (skip impl block methods)
        self.impl_blocks.clear();
        self.is_processing_impl_blocks = false;
        self.walk_tree(tree.root_node(), &mut symbols, None);

        // Phase 2: Process impl blocks after all symbols are extracted
        // SAFETY FIX: Pass tree reference so we can reconstruct nodes from byte ranges
        self.is_processing_impl_blocks = true;
        self.process_impl_blocks(tree, &mut symbols);

        symbols
    }

    fn walk_tree(&mut self, node: Node, symbols: &mut Vec<Symbol>, parent_id: Option<String>) {
        if let Some(symbol) = self.extract_symbol(node, parent_id.clone()) {
            let symbol_id = symbol.id.clone();
            symbols.push(symbol);

            // Continue traversing with new parent_id for nested symbols
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                self.walk_tree(child, symbols, Some(symbol_id.clone()));
            }
        } else {
            // No symbol extracted, continue with current parent_id
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                self.walk_tree(child, symbols, parent_id.clone());
            }
        }
    }

    fn extract_symbol(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        match node.kind() {
            "struct_item" => types::extract_struct(self, node, parent_id),
            "enum_item" => types::extract_enum(self, node, parent_id),
            "trait_item" => types::extract_trait(self, node, parent_id),
            "impl_item" => {
                functions::extract_impl(self, node, parent_id);
                None // impl blocks don't create symbols directly
            }
            "function_item" => {
                // Skip if inside impl block during phase 1
                if is_inside_impl(node) && !self.is_processing_impl_blocks {
                    None
                } else {
                    functions::extract_function(self, node, parent_id)
                }
            }
            "function_signature_item" => {
                signatures::extract_function_signature(self, node, parent_id)
            }
            "associated_type" => signatures::extract_associated_type(self, node, parent_id),
            "field_declaration" => types::extract_field(self, node, parent_id),
            "enum_variant" => types::extract_enum_variant(self, node, parent_id),
            "union_item" => types::extract_union(self, node, parent_id),
            "macro_invocation" => signatures::extract_macro_invocation(self, node, parent_id),
            "mod_item" => types::extract_module(self, node, parent_id),
            "use_declaration" => signatures::extract_use(self, node, parent_id),
            "const_item" => types::extract_const(self, node, parent_id),
            "static_item" => types::extract_static(self, node, parent_id),
            "macro_definition" => types::extract_macro(self, node, parent_id),
            "type_item" => types::extract_type_alias(self, node, parent_id),
            _ => None,
        }
    }

    fn process_impl_blocks(&mut self, tree: &Tree, symbols: &mut Vec<Symbol>) {
        functions::process_impl_blocks(self, tree, symbols);
    }

    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        relationships::extract_relationships(self, tree, symbols)
    }

    pub fn extract_identifiers(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Identifier> {
        identifiers::extract_identifiers(self, tree, symbols)
    }

    /// Infer types from Rust signatures (function return types, variable types, field types)
    pub fn infer_types(&self, symbols: &[Symbol]) -> std::collections::HashMap<String, String> {
        let mut type_map = std::collections::HashMap::new();

        for symbol in symbols {
            // For functions/methods, try to extract return type from signature
            if matches!(symbol.kind, SymbolKind::Function | SymbolKind::Method) {
                if let Some(ref signature) = symbol.signature {
                    // Extract return type: "-> Type"
                    if let Some(captures) = RETURN_TYPE_RE.captures(signature) {
                        let return_type = captures[1].trim().to_string();
                        if !return_type.is_empty() {
                            type_map.insert(symbol.id.clone(), return_type);
                        }
                    }
                }
            }
            // For variables, properties, fields - extract type annotation
            else if matches!(
                symbol.kind,
                SymbolKind::Variable | SymbolKind::Property | SymbolKind::Field
            ) {
                if let Some(ref signature) = symbol.signature {
                    // Extract type from annotations: "name: Type" or "name: Type ="
                    if let Some(captures) = VAR_TYPE_RE.captures(signature) {
                        let type_str = captures[1].trim().to_string();
                        if !type_str.is_empty() {
                            type_map.insert(symbol.id.clone(), type_str);
                        }
                    }
                }
            }
        }

        type_map
    }

    // Accessors for use by submodules and tests
    pub(crate) fn get_base_mut(&mut self) -> &mut BaseExtractor {
        &mut self.base
    }

    pub(super) fn get_impl_blocks(&self) -> &[ImplBlockInfo] {
        &self.impl_blocks
    }

    pub(super) fn add_impl_block(&mut self, block: ImplBlockInfo) {
        self.impl_blocks.push(block);
    }
}
