/// Markdown extractor - Extract sections as symbols for documentation embedding
///
/// This extractor treats markdown sections (headings) as symbols, enabling:
/// 1. Semantic search across documentation
/// 2. goto definition for heading navigation
/// 3. Knowledge graph connections between code and docs

use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind};
use std::path::Path;
use tree_sitter::Tree;

pub struct MarkdownExtractor {
    pub(crate) base: BaseExtractor,
}

impl MarkdownExtractor {
    pub fn new(
        language: String,
        file_path: String,
        source_code: String,
        workspace_root: &Path,
    ) -> Self {
        let base = BaseExtractor::new(language, file_path, source_code, workspace_root);
        Self { base }
    }

    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.walk_tree_for_symbols(tree.root_node(), &mut symbols, None);
        symbols
    }

    /// Walk the tree and extract heading symbols
    fn walk_tree_for_symbols(
        &mut self,
        node: tree_sitter::Node,
        symbols: &mut Vec<Symbol>,
        parent_id: Option<String>,
    ) {
        let symbol = self.extract_symbol_from_node(node, parent_id.as_deref());
        let mut current_parent_id = parent_id;

        if let Some(ref sym) = symbol {
            symbols.push(sym.clone());
            current_parent_id = Some(sym.id.clone());
        }

        // Recursively process child nodes
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_for_symbols(child, symbols, current_parent_id.clone());
        }
    }

    /// Extract symbol from a node based on its type
    fn extract_symbol_from_node(
        &mut self,
        node: tree_sitter::Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        match node.kind() {
            // tree-sitter-md uses "section" nodes for headings
            "section" => self.extract_section(node, parent_id),
            _ => None,
        }
    }

    /// Extract a section (heading) as a symbol
    fn extract_section(&mut self, node: tree_sitter::Node, parent_id: Option<&str>) -> Option<Symbol> {
        // Find the heading and section content within the section
        let mut heading_node = None;
        let mut section_content = String::new();

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "atx_heading" || child.kind() == "heading" {
                heading_node = Some(child);
            } else if child.kind() == "paragraph" {
                // Collect paragraph content for doc_comment
                let para_text = self.base.get_node_text(&child);
                if !section_content.is_empty() {
                    section_content.push_str("\n\n");
                }
                section_content.push_str(&para_text);
            }
        }

        if let Some(heading) = heading_node {
            return self.extract_heading(heading, parent_id, Some(section_content));
        }

        None
    }

    /// Extract heading text and create symbol
    fn extract_heading(
        &mut self,
        node: tree_sitter::Node,
        parent_id: Option<&str>,
        section_content: Option<String>,
    ) -> Option<Symbol> {
        use crate::extractors::base::SymbolOptions;

        // Extract the heading text (skip the # markers)
        let heading_text = self.extract_heading_text(node)?;

        // Determine heading level (1-6) - could be used in metadata later
        let _level = self.determine_heading_level(node);

        // Include section content as doc_comment for RAG embedding
        let doc_comment = section_content.filter(|s| !s.is_empty());

        let options = SymbolOptions {
            signature: None, // No signature for headings
            visibility: None,
            parent_id: parent_id.map(|s| s.to_string()),
            doc_comment,
            ..Default::default()
        };

        let symbol = self.base.create_symbol(
            &node,
            heading_text,
            SymbolKind::Module, // Treat sections as modules for semantic grouping
            options,
        );

        Some(symbol)
    }

    /// Extract the text content of a heading (without # markers)
    fn extract_heading_text(&self, node: tree_sitter::Node) -> Option<String> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            // Look for inline content or heading_content
            if child.kind() == "inline" || child.kind() == "heading_content" {
                let text = self.base.get_node_text(&child);
                return Some(text);
            }
        }

        // Fallback: get entire node text and strip # markers
        let text = self.base.get_node_text(&node);
        let text = text.trim_start_matches('#').trim();
        Some(text.to_string())
    }

    /// Determine heading level from number of # markers
    fn determine_heading_level(&self, node: tree_sitter::Node) -> usize {
        let text = self.base.get_node_text(&node);

        // Count leading # characters
        text.chars().take_while(|&c| c == '#').count().max(1).min(6)
    }
}
