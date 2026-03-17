//! Elixir Extractor
//!
//! Implementation of Elixir extractor using the call-dispatch pattern
//! inspired by Ruby's `calls.rs`.
//!
//! In Elixir, all definition macros (defmodule, def, defp, defmacro,
//! defprotocol, defimpl, defstruct) are generic `call` nodes in the AST.
//! We dispatch on the call target name to determine what's being defined.
//!
//! Module attributes (`@doc`, `@spec`, `@type`, etc.) are `unary_operator` nodes
//! with `@` as the operator.
//!
//! This extractor handles:
//! - Modules (defmodule)
//! - Functions (def/defp), macros (defmacro/defmacrop)
//! - Protocols (defprotocol), implementations (defimpl)
//! - Structs (defstruct)
//! - Imports (import, use, alias, require)
//! - Module attributes (@type, @callback, @spec, @behaviour)

mod attributes;
mod calls;
mod helpers;
mod identifiers;
mod relationships;
mod types_inference;

use crate::base::{BaseExtractor, Identifier, PendingRelationship, Relationship, Symbol};
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

pub struct ElixirExtractor {
    base: BaseExtractor,
    /// Pending relationships that need cross-file resolution after workspace indexing
    pending_relationships: Vec<PendingRelationship>,
    /// Track nested defmodule nesting for qualified names
    module_stack: Vec<String>,
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
            pending_relationships: Vec::new(),
            module_stack: Vec::new(),
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

    /// Get current module context for qualified names
    pub fn current_module(&self) -> Option<&str> {
        self.module_stack.last().map(|s| s.as_str())
    }

    /// Push a module onto the stack
    pub fn push_module(&mut self, name: String) {
        self.module_stack.push(name);
    }

    /// Pop a module from the stack
    pub fn pop_module(&mut self) {
        self.module_stack.pop();
    }

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
        let mut new_parent_id = parent_id.clone();
        let mut handled_children = false;

        match node.kind() {
            "call" => {
                // The core Elixir dispatch: all definitions are call nodes
                let result = calls::dispatch_call(self, &node, symbols, parent_id.as_deref());
                if let Some((sym, children_handled)) = result {
                    symbol = Some(sym);
                    handled_children = children_handled;
                }
            }
            "unary_operator" => {
                // Module attributes: @doc, @spec, @type, @callback, @behaviour
                symbol = attributes::extract_module_attribute(
                    &mut self.base,
                    &node,
                    parent_id.as_deref(),
                );
            }
            _ => {}
        }

        if let Some(ref sym) = symbol {
            symbols.push(sym.clone());
            new_parent_id = Some(sym.id.clone());
        }

        // Recursively visit children (unless the call handler already did)
        if !handled_children {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                self.visit_node(child, symbols, new_parent_id.clone());
            }
        }
    }

    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        types_inference::infer_types(symbols)
    }

    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        let mut relationships = Vec::new();
        relationships::extract_relationships(self, tree, symbols, &mut relationships);
        relationships
    }

    pub fn extract_identifiers(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Identifier> {
        identifiers::extract_identifiers(&mut self.base, tree, symbols)
    }

    pub(crate) fn base(&self) -> &BaseExtractor {
        &self.base
    }

    pub(crate) fn base_mut(&mut self) -> &mut BaseExtractor {
        &mut self.base
    }
}
