/// Helper utilities for Python symbol extraction
/// Includes AST navigation, argument extraction, and string handling
use super::PythonExtractor;
use tree_sitter::Node;

/// Walk up the AST tree to find the nearest enclosing class_definition
/// and return its generated symbol ID. Returns None if not inside a class.
pub fn find_parent_class_id(extractor: &PythonExtractor, node: &Node) -> Option<String> {
    let mut current = *node;
    while let Some(parent) = current.parent() {
        if parent.kind() == "class_definition" {
            let class_name = match parent.child_by_field_name("name") {
                Some(name_node) => extractor.base().get_node_text(&name_node),
                None => {
                    current = parent;
                    continue;
                }
            };

            let start_pos = parent.start_position();
            let parent_id = extractor.base().generate_id(
                &class_name,
                start_pos.row as u32,
                start_pos.column as u32,
            );

            return Some(parent_id);
        }
        current = parent;
    }
    None
}

/// Extract argument list from a superclasses node
pub fn extract_argument_list(extractor: &PythonExtractor, node: &Node) -> Vec<String> {
    let mut args = Vec::new();
    let base = extractor.base();

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "identifier" | "attribute" => {
                args.push(base.get_node_text(&child));
            }
            "subscript" => {
                // Handle generic types like Generic[K, V]
                args.push(base.get_node_text(&child));
            }
            "keyword_argument" => {
                // Handle keyword arguments like metaclass=SingletonMeta
                let mut child_cursor = child.walk();
                let children: Vec<_> = child.children(&mut child_cursor).collect();
                if let (Some(keyword_node), Some(value_node)) = (children.first(), children.last())
                {
                    if keyword_node.kind() == "identifier"
                        && base.get_node_text(keyword_node) == "metaclass"
                    {
                        args.push(format!(
                            "{}={}",
                            base.get_node_text(keyword_node),
                            base.get_node_text(value_node)
                        ));
                    }
                }
            }
            _ => {}
        }
    }

    args
}

/// Helper to strip string delimiters (quotes) from Python strings
/// Handles triple quotes (""" or '''), double quotes ("), and single quotes (')
pub fn strip_string_delimiters(s: &str) -> String {
    // Try delimiters in order: triple quotes first (3 chars), then single quotes (1 char)
    let delimiters = [("\"\"\"", 3), ("'''", 3), ("\"", 1), ("'", 1)];

    for (delimiter, strip_count) in &delimiters {
        if s.starts_with(delimiter) && s.ends_with(delimiter) && s.len() >= strip_count * 2 {
            return s[*strip_count..s.len() - strip_count].to_string();
        }
    }

    // No matching delimiter found, return as-is
    s.to_string()
}
