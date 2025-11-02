//! Output formatting for call path traces (ASCII tree and summary text)

use crate::extractors::Symbol;
use anyhow::Result;
use std::collections::HashSet;

use super::types::{CallPath, CallPathNode, MatchType, SerializablePathNode};

/// Format call trees based on output format preference
pub fn format_call_trees(
    trees: &[(Symbol, Vec<CallPathNode>)],
    symbol: &str,
    direction: &str,
    max_depth: u32,
    output_format: &str,
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

    // Choose output format based on parameter
    if output_format == "tree" {
        // ASCII tree visualization for humans
        build_ascii_tree(trees, total_nodes, &all_languages, direction_label, symbol, direction, max_depth)
    } else {
        // JSON-focused summary for AI agents (default)
        Ok(format!(
            "Traced {} call paths for '{}' (direction: {}, depth: {}, cross_language: {})\nFound {} {} across {} languages\n\nFull call path details are in structured_content.call_paths",
            trees.len(),
            symbol,
            direction,
            max_depth,
            true, // Cross-language always enabled
            total_nodes,
            direction_label,
            all_languages.len()
        ))
    }
}

/// Build ASCII tree visualization for human readability
fn build_ascii_tree(
    trees: &[(Symbol, Vec<CallPathNode>)],
    total_nodes: usize,
    all_languages: &HashSet<String>,
    direction_label: &str,
    symbol: &str,
    direction: &str,
    max_depth: u32,
) -> Result<String> {
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
        MatchType::Semantic => "~",
    };

    // Format similarity if present
    let similarity_str = if let Some(sim) = node.similarity {
        format!(" [sim: {:.2}]", sim)
    } else {
        String::new()
    };

    // Write node
    output.push_str(&format!(
        "{}{} {} {} ({}:{}){}\n",
        prefix,
        connector,
        match_indicator,
        node.symbol.name,
        node.symbol.file_path,
        node.symbol.start_line,
        similarity_str
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

/// Convert trees to serializable format for structured output
pub fn trees_to_call_paths(trees: &[(Symbol, Vec<CallPathNode>)]) -> Vec<CallPath> {
    trees
        .iter()
        .map(|(root, nodes)| {
            let max_depth = calculate_max_depth(nodes);
            CallPath {
                root_symbol: root.name.clone(),
                root_file: root.file_path.clone(),
                root_language: root.language.clone(),
                nodes: nodes.iter().map(|n| node_to_serializable(n)).collect(),
                total_depth: max_depth,
            }
        })
        .collect()
}

/// Convert CallPathNode to serializable format
fn node_to_serializable(node: &CallPathNode) -> SerializablePathNode {
    let match_type_str = match node.match_type {
        MatchType::Direct => "direct",
        MatchType::NamingVariant => "naming_variant",
        MatchType::Semantic => "semantic",
    };

    let relationship_str = node.relationship_kind.as_ref().map(|k| {
        use crate::extractors::RelationshipKind;
        match k {
            RelationshipKind::Calls => "calls",
            RelationshipKind::Extends => "extends",
            RelationshipKind::Implements => "implements",
            RelationshipKind::Uses => "uses",
            RelationshipKind::Returns => "returns",
            RelationshipKind::Parameter => "parameter",
            RelationshipKind::Imports => "imports",
            RelationshipKind::Instantiates => "instantiates",
            RelationshipKind::References => "references",
            RelationshipKind::Defines => "defines",
            RelationshipKind::Overrides => "overrides",
            RelationshipKind::Contains => "contains",
            RelationshipKind::Joins => "joins",
            RelationshipKind::Composition => "composition",
        }
        .to_string()
    });

    SerializablePathNode {
        symbol_name: node.symbol.name.clone(),
        file_path: node.symbol.file_path.clone(),
        language: node.symbol.language.clone(),
        line: node.symbol.start_line,
        match_type: match_type_str.to_string(),
        relationship_kind: relationship_str,
        similarity: node.similarity,
        level: node.level,
        children: node
            .children
            .iter()
            .map(|c| node_to_serializable(c))
            .collect(),
    }
}

/// Calculate maximum depth in tree
fn calculate_max_depth(nodes: &[CallPathNode]) -> u32 {
    nodes
        .iter()
        .map(|n| {
            let child_depth = if n.children.is_empty() {
                0
            } else {
                calculate_max_depth(&n.children)
            };
            n.level + child_depth
        })
        .max()
        .unwrap_or(0)
}
