//! View and SELECT alias extraction.
//!
//! Handles extraction of:
//! - SELECT query aliases (expression AS name)
//! - View columns from CREATE VIEW statements
//! - View columns from ERROR nodes containing CREATE VIEW

use crate::base::{BaseExtractor, SymbolKind, SymbolOptions, Symbol};
use std::collections::HashMap;
use tree_sitter::Node;

use super::SqlExtractor;
use crate::sql::helpers::CREATE_VIEW_RE;

impl SqlExtractor {
    /// Extract SELECT query aliases as fields
    pub(super) fn extract_select_aliases(
        &mut self,
        select_node: tree_sitter::Node,
        symbols: &mut Vec<Symbol>,
        parent_id: Option<&str>,
    ) {
        // Port extractSelectAliases logic using iterative approach to avoid borrow checker issues
        let term_nodes = self.base.find_nodes_by_type(&select_node, "term");

        for node in term_nodes {
            let mut children = Vec::new();
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    children.push(child);
                }
            }

            if children.len() >= 3 {
                for i in 0..(children.len() - 2) {
                    if children[i + 1].kind() == "keyword_as"
                        && children[i + 2].kind() == "identifier"
                    {
                        let expr_node = children[i];
                        let alias_name = self.base.get_node_text(&children[i + 2]);
                        let expr_text = self.base.get_node_text(&expr_node);

                        // Determine expression type for better signatures - CRITICAL: Window function handling
                        let expression = match expr_node.kind() {
                            "case" => "CASE expression".to_string(),
                            "window_function" => {
                                // Keep the OVER clause in the signature for window functions
                                if expr_text.contains("OVER (") {
                                    if let Some(over_index) = expr_text.find("OVER (") {
                                        if let Some(end_index) = expr_text[over_index..].find(')') {
                                            // Use safe UTF-8 aware substring extraction
                                            let total_len = over_index + end_index + 1;
                                            if expr_text.is_char_boundary(total_len) {
                                                expr_text[0..total_len].to_string()
                                            } else {
                                                expr_text.clone()
                                            }
                                        } else {
                                            expr_text.clone() // Keep full text if no closing paren
                                        }
                                    } else {
                                        expr_text.clone()
                                    }
                                } else {
                                    expr_text.clone() // Keep full text for window_function type
                                }
                            }
                            _ => {
                                if expr_text.contains("OVER (") {
                                    // Handle expressions with OVER clauses that aren't detected as window_function
                                    if let Some(over_index) = expr_text.find("OVER (") {
                                        if let Some(end_index) = expr_text[over_index..].find(')') {
                                            // Use safe UTF-8 aware substring extraction
                                            let total_len = over_index + end_index + 1;
                                            if expr_text.is_char_boundary(total_len) {
                                                expr_text[0..total_len].to_string()
                                            } else {
                                                expr_text.clone()
                                            }
                                        } else {
                                            expr_text.clone()
                                        }
                                    } else {
                                        expr_text.clone()
                                    }
                                } else if expr_text.contains("COUNT")
                                    || expr_text.contains("SUM")
                                    || expr_text.contains("AVG")
                                    || expr_text.contains("MAX")
                                    || expr_text.contains("MIN")
                                {
                                    format!(
                                        "{}()",
                                        expr_text.split('(').next().unwrap_or(&expr_text)
                                    )
                                } else {
                                    expr_text.clone()
                                }
                            }
                        };

                        let signature = format!("{} AS {}", expression, alias_name);

                        let mut metadata = HashMap::new();
                        metadata.insert("isSelectAlias".to_string(), serde_json::Value::Bool(true));
                        metadata
                            .insert("isComputedField".to_string(), serde_json::Value::Bool(true));

                        let options = SymbolOptions {
                            signature: Some(signature),
                            visibility: Some(crate::base::Visibility::Public),
                            parent_id: parent_id.map(|s| s.to_string()),
                            doc_comment: None,
                            metadata: Some(metadata),
                        };

                        let alias_symbol =
                            self.base
                                .create_symbol(&node, alias_name, SymbolKind::Field, options);
                        symbols.push(alias_symbol);
                        break;
                    }
                }
            }
        }
    }

    /// Extract view columns from CREATE VIEW statement
    pub(super) fn extract_view_columns(
        &mut self,
        view_node: tree_sitter::Node,
        symbols: &mut Vec<Symbol>,
        parent_view_id: &str,
    ) {
        // Port extractViewColumns logic
        let nodes = self.base.find_nodes_by_type(&view_node, "select_statement");
        for select_node in nodes {
            self.extract_select_aliases(select_node, symbols, Some(parent_view_id));
        }

        let select_nodes = self.base.find_nodes_by_type(&view_node, "select");
        for select_node in select_nodes {
            self.extract_select_aliases(select_node, symbols, Some(parent_view_id));
        }
    }

    /// Extract view columns from ERROR node
    pub(super) fn extract_view_columns_from_error_node(
        &mut self,
        node: tree_sitter::Node,
        symbols: &mut Vec<Symbol>,
        parent_view_id: &str,
    ) {
        // Port extractViewColumnsFromErrorNode logic
        let error_text = self.base.get_node_text(&node);

        // Only process if this ERROR node contains a CREATE VIEW statement
        if !error_text.contains("CREATE VIEW") {
            return;
        }

        let select_index = match error_text.find("SELECT") {
            Some(idx) => idx,
            None => return,
        };

        // Find the FROM clause to limit our search to the SELECT list only
        let from_regex = regex::Regex::new(r"\bFROM\s+[a-zA-Z_][a-zA-Z0-9_]*\s+[a-zA-Z_]").unwrap();
        let from_index = from_regex
            .find(&error_text[select_index..])
            .map(|from_match| select_index + from_match.start());

        let select_section = if let Some(from_idx) = from_index {
            if from_idx > select_index
                && error_text.is_char_boundary(select_index)
                && error_text.is_char_boundary(from_idx)
            {
                &error_text[select_index..from_idx]
            } else if error_text.is_char_boundary(select_index) {
                &error_text[select_index..]
            } else {
                &error_text
            }
        } else if error_text.is_char_boundary(select_index) {
            &error_text[select_index..]
        } else {
            &error_text
        };

        // Extract SELECT aliases using regex patterns
        let alias_regex = regex::Regex::new(
            r"(?:^|,|\s)\s*(.+?)\s+(?:[Aa][Ss]\s+)?([a-zA-Z_][a-zA-Z0-9_]*)\s*(?:,|$)",
        )
        .unwrap();

        for captures in alias_regex.captures_iter(select_section) {
            // Safe: if regex matched, capture groups should exist, but handle gracefully
            let full_expression = captures.get(1).map_or("", |m| m.as_str()).trim();
            let alias_name = captures.get(2).map_or("", |m| m.as_str());

            // Skip if capture groups were empty
            if full_expression.is_empty() || alias_name.is_empty() {
                continue;
            }

            // Skip single-character aliases (common table abbreviations like u, t, p)
            // and two-character aliases (common shorthand like ae, ur, ev)
            if alias_name.len() <= 2 {
                continue;
            }

            // Skip if the expression looks like a simple column reference
            if !full_expression.contains('(')
                && !full_expression.contains("COUNT")
                && !full_expression.contains("MIN")
                && !full_expression.contains("MAX")
                && !full_expression.contains("AVG")
                && !full_expression.contains("SUM")
                && !full_expression.contains("EXTRACT")
                && !full_expression.contains("CASE")
                && full_expression.split('.').count() <= 2
            {
                continue;
            }

            let signature = format!("{} AS {}", full_expression, alias_name);

            let mut metadata = HashMap::new();
            metadata.insert("isSelectAlias".to_string(), serde_json::Value::Bool(true));
            metadata.insert("isComputedField".to_string(), serde_json::Value::Bool(true));
            metadata.insert(
                "extractedFromError".to_string(),
                serde_json::Value::Bool(true),
            );

            let options = SymbolOptions {
                signature: Some(signature),
                visibility: Some(crate::base::Visibility::Public),
                parent_id: Some(parent_view_id.to_string()),
                doc_comment: None,
                metadata: Some(metadata),
            };

            let alias_symbol =
                self.base
                    .create_symbol(&node, alias_name.to_string(), SymbolKind::Field, options);
            symbols.push(alias_symbol);
        }
    }
}

/// Extract views from ERROR node text
pub(super) fn extract_views_from_error(
    error_text: &str,
    base: &mut BaseExtractor,
    node: &Node,
    symbols: &mut Vec<Symbol>,
    parent_id: Option<&str>,
) {
    if let Some(captures) = CREATE_VIEW_RE.captures(error_text) {
        if let Some(view_name) = captures.get(1) {
            let name = view_name.as_str().to_string();

            let mut metadata = HashMap::new();
            metadata.insert("isView".to_string(), serde_json::Value::Bool(true));
            metadata.insert(
                "extractedFromError".to_string(),
                serde_json::Value::Bool(true),
            );

            let options = SymbolOptions {
                signature: Some(format!("CREATE VIEW {}", name)),
                visibility: Some(crate::base::Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                doc_comment: None,
                metadata: Some(metadata),
            };

            let view_symbol =
                base.create_symbol(node, name.clone(), SymbolKind::Interface, options);
            symbols.push(view_symbol.clone());
        }
    }
}
