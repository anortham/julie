/// Elixir language extractor with support for:
/// - Modules (defmodule), protocols (defprotocol), implementations (defimpl)
/// - Functions (def/defp), macros (defmacro/defmacrop)
/// - Structs (defstruct), callbacks, typespecs
/// - Module attributes (@doc, @moduledoc, @type, @spec, @callback, @behaviour)
/// - Import directives (use, import, alias, require)
/// - Relationships: protocol implementation, behaviour adoption, function calls
/// - Identifier extraction for LSP-quality find_references
use crate::base::{BaseExtractor, Identifier, Relationship, Symbol};
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

mod attributes;
mod calls;
mod helpers;
mod identifiers;
mod relationships;
mod types_inference;

/// Elixir extractor that handles Elixir-specific constructs.
///
/// Elixir's tree-sitter grammar represents nearly everything as `call` nodes.
/// defmodule, def, defp, defmacro, defprotocol, defimpl, defstruct, use, import,
/// alias, and require are all generic `call` nodes distinguished by their target name.
pub struct ElixirExtractor {
    pub(crate) base: BaseExtractor,
    /// Stack of module names for building qualified names
    pub(crate) module_stack: Vec<String>,
    /// Collected @spec annotations keyed by function name
    pub(crate) specs: HashMap<String, String>,
    /// Pending relationships that need cross-file resolution
    pending_relationships: Vec<crate::base::PendingRelationship>,
}

impl ElixirExtractor {
    pub fn new(
        language: String,
        file_path: String,
        content: String,
        workspace_root: &std::path::Path,
    ) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content, workspace_root),
            module_stack: Vec::new(),
            specs: HashMap::new(),
            pending_relationships: Vec::new(),
        }
    }

    /// Extract all symbols from Elixir source code
    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.base.symbol_map.clear();
        self.module_stack.clear();
        self.specs.clear();

        self.traverse_node(&tree.root_node(), &mut symbols, None);
        symbols
    }

    /// Extract relationships between symbols
    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        relationships::extract_relationships(self, tree, symbols)
    }

    /// Extract identifier usages for LSP-quality references
    pub fn extract_identifiers(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Identifier> {
        identifiers::extract_identifiers(&mut self.base, tree, symbols)
    }

    /// Infer types from @spec annotations and other type hints
    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        types_inference::infer_types(&self.specs, symbols)
    }

    // ========================================================================
    // Tree Traversal
    // ========================================================================

    pub(crate) fn traverse_node(
        &mut self,
        node: &Node,
        symbols: &mut Vec<Symbol>,
        parent_id: Option<&str>,
    ) {
        match node.kind() {
            "call" => {
                if let Some((symbol, children_visited)) =
                    calls::dispatch_call(self, node, symbols, parent_id)
                {
                    let sym_id = symbol.id.clone();
                    symbols.push(symbol);
                    if !children_visited {
                        self.traverse_children(node, symbols, Some(&sym_id));
                    }
                    return; // Don't double-traverse children
                }
                // Not a definition call — fall through to traverse children
            }
            "unary_operator" => {
                if let Some(symbol) = attributes::extract_attribute(self, node, parent_id) {
                    symbols.push(symbol);
                    return;
                }
            }
            _ => {}
        }

        // Default: traverse children
        self.traverse_children(node, symbols, parent_id);
    }

    pub(crate) fn traverse_children(
        &mut self,
        node: &Node,
        symbols: &mut Vec<Symbol>,
        parent_id: Option<&str>,
    ) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.traverse_node(&child, symbols, parent_id);
        }
    }

    pub fn get_pending_relationships(&self) -> Vec<crate::base::PendingRelationship> {
        self.pending_relationships.clone()
    }
}
