//! Function extraction for Bash
//!
//! Handles extraction of function definitions and their positional parameters.

use crate::extractors::base::{Symbol, SymbolKind, SymbolOptions, Visibility};
use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;
use tree_sitter::Node;

static PARAM_NUMBER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\$(\d+)").unwrap());

impl super::BashExtractor {
    /// Extract a function definition from a function_definition node
    pub(super) fn extract_function(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        let name_node = self.find_name_node(node)?;
        let name = self.base.get_node_text(&name_node);

        let options = SymbolOptions {
            signature: Some(self.extract_function_signature(node)),
            visibility: Some(Visibility::Public), // Bash functions are generally accessible within the script
            parent_id: parent_id.map(|s| s.to_string()),
            doc_comment: self.base.find_doc_comment(&node),
            ..Default::default()
        };

        Some(
            self.base
                .create_symbol(&node, name, SymbolKind::Function, options),
        )
    }

    /// Extract positional parameters ($1, $2, etc.) from a function
    pub(super) fn extract_positional_parameters(
        &mut self,
        func_node: Node,
        parent_id: &str,
    ) -> Vec<Symbol> {
        let mut parameters = Vec::new();
        let mut seen_params = HashSet::new();

        // Collect parameter nodes first, then process them
        let mut param_nodes = Vec::new();
        self.collect_parameter_nodes(func_node, &mut param_nodes);

        for node in param_nodes {
            let param_text = self.base.get_node_text(&node);
            if let Some(captures) = PARAM_NUMBER_RE.captures(&param_text) {
                if let Some(param_number) = captures.get(1) {
                    let param_name = format!("${}", param_number.as_str());

                    if !seen_params.contains(&param_name) {
                        seen_params.insert(param_name.clone());

                        let options = SymbolOptions {
                            signature: Some(format!("{} (positional parameter)", param_name)),
                            visibility: Some(Visibility::Public),
                            parent_id: Some(parent_id.to_string()),
                            doc_comment: self.base.find_doc_comment(&node),
                            ..Default::default()
                        };

                        let param_symbol = self.base.create_symbol(
                            &node,
                            param_name,
                            SymbolKind::Variable,
                            options,
                        );
                        parameters.push(param_symbol);
                    }
                }
            }
        }

        parameters
    }
}
