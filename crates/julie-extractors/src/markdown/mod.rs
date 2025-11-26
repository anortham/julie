/// Markdown extractor - Extract sections as symbols for documentation embedding
///
/// This extractor treats markdown sections (headings) as symbols, enabling:
/// 1. Semantic search across documentation
/// 2. goto definition for heading navigation
/// 3. Knowledge graph connections between code and docs
use crate::base::{BaseExtractor, Identifier, Symbol, SymbolKind};
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
            // YAML frontmatter (--- delimited)
            "minus_metadata" => self.extract_frontmatter(node),
            // TOML frontmatter (+++ delimited)
            "plus_metadata" => self.extract_frontmatter(node),
            _ => None,
        }
    }

    /// Extract frontmatter (YAML or TOML) as a symbol
    ///
    /// Frontmatter contains document metadata like title, author, tags, etc.
    /// Also captures body content following frontmatter (before any heading)
    /// for semantic search of memory files, blog posts, etc.
    ///
    /// This is valuable for:
    /// 1. Semantic search (find docs by metadata AND content)
    /// 2. Documentation organization
    /// 3. Blog/static site content discovery
    /// 4. Development memory checkpoint search
    fn extract_frontmatter(&mut self, node: tree_sitter::Node) -> Option<Symbol> {
        use crate::base::SymbolOptions;

        let raw_text = self.base.get_node_text(&node);

        // Strip the delimiters (--- or +++) from start and end
        let frontmatter_content = self.strip_frontmatter_delimiters(&raw_text);

        // Skip empty frontmatter
        if frontmatter_content.trim().is_empty() {
            return None;
        }

        // Capture body content that follows frontmatter but precedes any heading
        // This is critical for memory files that have descriptions after frontmatter
        let body_content = self.capture_body_after_frontmatter(node);

        // Combine frontmatter and body content for rich semantic search
        let doc_comment = if body_content.is_empty() {
            frontmatter_content
        } else {
            format!("{}\n\n---\n\n{}", frontmatter_content, body_content)
        };

        let options = SymbolOptions {
            signature: None,
            visibility: None,
            parent_id: None, // Frontmatter is always top-level
            doc_comment: Some(doc_comment),
            ..Default::default()
        };

        let symbol = self.base.create_symbol(
            &node,
            "frontmatter".to_string(),
            SymbolKind::Property, // Metadata property
            options,
        );

        Some(symbol)
    }

    /// Capture body content that follows frontmatter but precedes any heading
    ///
    /// tree-sitter-md wraps content in "section" nodes. We need to:
    /// 1. Find sections after frontmatter
    /// 2. Extract content from sections that have NO heading (just body text)
    /// 3. Stop at sections that HAVE a heading
    fn capture_body_after_frontmatter(&mut self, frontmatter_node: tree_sitter::Node) -> String {
        let Some(parent) = frontmatter_node.parent() else {
            return String::new();
        };

        let mut body_content = String::new();
        let mut found_frontmatter = false;
        let mut cursor = parent.walk();

        for sibling in parent.children(&mut cursor) {
            if sibling.id() == frontmatter_node.id() {
                found_frontmatter = true;
                continue;
            }

            if found_frontmatter {
                // tree-sitter-md wraps content in "section" nodes
                if sibling.kind() == "section" {
                    // Check if this section has a heading
                    let has_heading = self.section_has_heading(&sibling);

                    if has_heading {
                        // Stop at first section with a heading
                        break;
                    } else {
                        // This section has no heading - extract its content
                        let section_content = self.extract_section_content(&sibling);
                        if !section_content.is_empty() {
                            if !body_content.is_empty() {
                                body_content.push_str("\n\n");
                            }
                            body_content.push_str(&section_content);
                        }
                    }
                }
            }
        }

        body_content
    }

    /// Check if a section node contains a heading (atx_heading)
    fn section_has_heading(&self, section_node: &tree_sitter::Node) -> bool {
        let mut cursor = section_node.walk();
        for child in section_node.children(&mut cursor) {
            if child.kind() == "atx_heading" || child.kind() == "heading" {
                return true;
            }
        }
        false
    }

    /// Extract all content from a section (paragraphs, lists, etc.)
    fn extract_section_content(&mut self, section_node: &tree_sitter::Node) -> String {
        let mut content = String::new();
        let mut cursor = section_node.walk();

        for child in section_node.children(&mut cursor) {
            if self.is_content_node(&child) {
                let text = self.base.get_node_text(&child);
                if !content.is_empty() {
                    content.push_str("\n\n");
                }
                content.push_str(&text);
            }
        }

        content
    }

    /// Strip frontmatter delimiters (--- or +++) from raw text
    fn strip_frontmatter_delimiters(&self, text: &str) -> String {
        let lines: Vec<&str> = text.lines().collect();

        if lines.len() < 2 {
            return String::new();
        }

        // Skip first line (opening delimiter) and last line (closing delimiter)
        // The closing delimiter might be on the last line or second-to-last if there's a trailing newline
        let start = 1;
        let end = if lines.last().map(|l| l.trim()).unwrap_or("") == "---"
            || lines.last().map(|l| l.trim()).unwrap_or("") == "+++"
        {
            lines.len() - 1
        } else if lines.len() > 2
            && (lines[lines.len() - 2].trim() == "---" || lines[lines.len() - 2].trim() == "+++")
        {
            lines.len() - 2
        } else {
            lines.len()
        };

        lines[start..end].join("\n")
    }

    /// Extract a section (heading) as a symbol
    fn extract_section(
        &mut self,
        node: tree_sitter::Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        // Find the heading and section content within the section
        let mut heading_node = None;
        let mut section_content = String::new();

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "atx_heading" || child.kind() == "heading" {
                heading_node = Some(child);
            } else if self.is_content_node(&child) {
                // Collect ALL content nodes (not just paragraphs) for RAG embedding
                // This includes: paragraphs, lists, code blocks, block quotes, tables, etc.
                let content_text = self.base.get_node_text(&child);
                if !section_content.is_empty() {
                    section_content.push_str("\n\n");
                }
                section_content.push_str(&content_text);
            }
        }

        if let Some(heading) = heading_node {
            return self.extract_heading(heading, parent_id, Some(section_content));
        }

        None
    }

    /// Check if a node contains content that should be included in section body
    /// This captures all markdown content types for comprehensive RAG embeddings
    fn is_content_node(&self, node: &tree_sitter::Node) -> bool {
        matches!(
            node.kind(),
            "paragraph"
                | "list"              // Unordered/ordered lists
                | "list_item"
                | "fenced_code_block" // ```code blocks```
                | "indented_code_block"
                | "block_quote"       // > quotes
                | "table"             // Tables
                | "thematic_break"    // ---
                | "html_block" // Raw HTML
        )
    }

    /// Extract heading text and create symbol
    fn extract_heading(
        &mut self,
        node: tree_sitter::Node,
        parent_id: Option<&str>,
        section_content: Option<String>,
    ) -> Option<Symbol> {
        use crate::base::SymbolOptions;

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

    pub fn extract_identifiers(&mut self, _tree: &Tree, _symbols: &[Symbol]) -> Vec<Identifier> {
        // Markdown is documentation - no code identifiers
        Vec::new()
    }
}
