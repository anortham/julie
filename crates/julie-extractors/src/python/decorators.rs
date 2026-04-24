/// Decorator extraction and handling
/// Supports @property, @staticmethod, @classmethod, and custom decorators
use super::PythonExtractor;
use tree_sitter::Node;

/// Extract decorators from a function or class definition
pub fn extract_decorators(extractor: &PythonExtractor, node: &Node) -> Vec<String> {
    extract_decorator_texts(extractor, node)
        .into_iter()
        .map(decorator_name_from_text)
        .collect()
}

/// Extract raw decorator text from a function or class definition.
pub fn extract_decorator_texts(extractor: &PythonExtractor, node: &Node) -> Vec<String> {
    let mut decorators = Vec::new();
    let base = extractor.base();

    if let Some(decorated_node) = find_decorated_node(node) {
        let mut cursor = decorated_node.walk();
        for child in decorated_node.children(&mut cursor) {
            if child.kind() == "decorator" {
                decorators.push(base.get_node_text(&child));
            }
        }
    }

    decorators
}

fn find_decorated_node<'a>(node: &Node<'a>) -> Option<Node<'a>> {
    // Check if current node is already a decorated_definition
    if node.kind() == "decorated_definition" {
        return Some(*node);
    }

    // Walk up to find decorated_definition parent
    let mut current = *node;
    while let Some(parent) = current.parent() {
        if parent.kind() == "decorated_definition" {
            return Some(parent);
        }
        current = parent;
    }

    None
}

fn decorator_name_from_text(mut decorator_text: String) -> String {
    // Remove @ prefix (@ is ASCII, so this is safe)
    if decorator_text.starts_with('@') && decorator_text.is_char_boundary(1) {
        decorator_text = decorator_text[1..].to_string();
    }

    // Extract name without parameters: "lru_cache(maxsize=128)" -> "lru_cache"
    if let Some(paren_index) = decorator_text.find('(') {
        if decorator_text.is_char_boundary(paren_index) {
            decorator_text = decorator_text[..paren_index].to_string();
        }
    }

    decorator_text
}
