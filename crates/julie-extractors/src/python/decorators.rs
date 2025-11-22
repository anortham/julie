/// Decorator extraction and handling
/// Supports @property, @staticmethod, @classmethod, and custom decorators
use super::PythonExtractor;
use tree_sitter::Node;

/// Extract decorators from a function or class definition
pub fn extract_decorators(extractor: &PythonExtractor, node: &Node) -> Vec<String> {
    let mut decorators = Vec::new();
    let mut decorated_node: Option<Node> = None;
    let base = extractor.base();

    // Check if current node is already a decorated_definition
    if node.kind() == "decorated_definition" {
        decorated_node = Some(*node);
    } else {
        // Walk up to find decorated_definition parent
        let mut current = *node;
        while let Some(parent) = current.parent() {
            if parent.kind() == "decorated_definition" {
                decorated_node = Some(parent);
                break;
            }
            current = parent;
        }
    }

    if let Some(decorated_node) = decorated_node {
        let mut cursor = decorated_node.walk();
        for child in decorated_node.children(&mut cursor) {
            if child.kind() == "decorator" {
                let mut decorator_text = base.get_node_text(&child);

                // Remove @ prefix (@ is ASCII, so this is safe)
                if decorator_text.starts_with('@') && decorator_text.is_char_boundary(1) {
                    decorator_text = decorator_text[1..].to_string();
                }

                // Extract just the decorator name without parameters
                // e.g., "lru_cache(maxsize=128)" -> "lru_cache"
                if let Some(paren_index) = decorator_text.find('(') {
                    // '(' is ASCII, so this should be safe, but verify
                    if decorator_text.is_char_boundary(paren_index) {
                        decorator_text = decorator_text[..paren_index].to_string();
                    }
                }

                decorators.push(decorator_text);
            }
        }
    }

    decorators
}
