//! Output formatting for call path traces (ASCII tree visualization)

use crate::extractors::Symbol;
use anyhow::Result;
use std::collections::HashSet;

use super::types::{CallPathNode, MatchType};

/// Format call trees as ASCII tree visualization
pub fn format_call_trees(
    trees: &[(Symbol, Vec<CallPathNode>)],
    symbol: &str,
    direction: &str,
    max_depth: u32,
) -> Result<String> {
    if trees.is_empty() {
        return Ok(format!(
            "No call paths found for '{}'\nTry enabling cross_language or using fast_refs",
            symbol
        ));
    }

    // Calculate statistics
    let total_nodes: usize = trees.iter().map(|(_, nodes)| count_nodes(nodes)).sum();
    let all_languages: HashSet<String> = trees
        .iter()
        .flat_map(|(_, nodes)| collect_languages(nodes))
        .collect();

    let direction_label = if direction == "upstream" {
        "callers"
    } else {
        "callees"
    };

    let mut output = String::new();

    // Header
    output.push_str(&format!("Call Path Trace: '{}'\n", symbol));
    output.push_str(&format!(
        "Direction: {} | Depth: {} | Cross-language: enabled\n",
        direction, max_depth
    ));
    output.push_str(&format!(
        "Found {} {} across {} languages\n\n",
        total_nodes,
        direction_label,
        all_languages.len()
    ));

    // Render each tree
    for (i, (root, nodes)) in trees.iter().enumerate() {
        output.push_str(&format!(
            "Path {}:\n{} ({}:{})\n",
            i + 1,
            root.name,
            root.file_path,
            root.start_line
        ));

        // Render child nodes recursively
        for (j, node) in nodes.iter().enumerate() {
            let is_last = j == nodes.len() - 1;
            render_node(node, &mut output, "", is_last);
        }
        output.push('\n');
    }

    Ok(output)
}

/// Recursively render a node in ASCII tree format
fn render_node(node: &CallPathNode, output: &mut String, prefix: &str, is_last: bool) {
    // Choose tree characters
    let connector = if is_last { "└─" } else { "├─" };
    let extension = if is_last { "  " } else { "│ " };

    // Format match type indicator
    let match_indicator = match node.match_type {
        MatchType::Direct => "→",
        MatchType::NamingVariant => "≈",
    };

    // Write node
    output.push_str(&format!(
        "{}{} {} {} ({}:{})\n",
        prefix,
        connector,
        match_indicator,
        node.symbol.name,
        node.symbol.file_path,
        node.symbol.start_line,
    ));

    // Render children
    let new_prefix = format!("{}{}", prefix, extension);
    for (i, child) in node.children.iter().enumerate() {
        let child_is_last = i == node.children.len() - 1;
        render_node(child, output, &new_prefix, child_is_last);
    }
}

/// Count total nodes in tree
fn count_nodes(nodes: &[CallPathNode]) -> usize {
    nodes.iter().map(|n| 1 + count_nodes(&n.children)).sum()
}

/// Collect all languages in tree
fn collect_languages(nodes: &[CallPathNode]) -> HashSet<String> {
    let mut languages = HashSet::new();
    for node in nodes {
        languages.insert(node.symbol.language.clone());
        languages.extend(collect_languages(&node.children));
    }
    languages
}
