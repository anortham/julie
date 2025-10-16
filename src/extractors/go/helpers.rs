use tree_sitter::Node;

/// Helper methods for Go-specific utilities and node text extraction
impl super::GoExtractor {
    /// Check if identifier is public (Go visibility rules)
    /// In Go, identifiers starting with uppercase are public
    pub(super) fn is_public(&self, name: &str) -> bool {
        name.chars().next().is_some_and(|c| c.is_uppercase())
    }

    /// Get node text (helper method)
    pub(super) fn get_node_text(&self, node: Node) -> String {
        self.base.get_node_text(&node)
    }

    /// Extract the type string from a type node
    pub(super) fn extract_type_from_node(&self, node: Node) -> String {
        match node.kind() {
            "type_identifier" | "primitive_type" => self.get_node_text(node),
            "map_type" => {
                let mut parts = Vec::new();
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    parts.push(self.get_node_text(child));
                }
                parts.join("")
            }
            "slice_type" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() != "[" && child.kind() != "]" {
                        return format!("[]{}", self.extract_type_from_node(child));
                    }
                }
                self.get_node_text(node)
            }
            "array_type" => self.get_node_text(node),
            "pointer_type" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() != "*" {
                        return format!("*{}", self.extract_type_from_node(child));
                    }
                }
                self.get_node_text(node)
            }
            "channel_type" => {
                // Handle channel types like <-chan, chan<-, chan
                self.get_node_text(node)
            }
            "interface_type" => {
                // Handle interface{} and other interface types
                self.get_node_text(node)
            }
            "function_type" => {
                // Handle function types like func(int) string
                self.get_node_text(node)
            }
            "qualified_type" => {
                // Handle types like package.TypeName
                self.get_node_text(node)
            }
            "generic_type" => {
                // Handle generic types like Stack[T]
                self.get_node_text(node)
            }
            "type_arguments" => {
                // Handle type arguments like [T, U]
                self.get_node_text(node)
            }
            _ => self.get_node_text(node),
        }
    }

    /// Extract interface body content for union types and methods
    pub(super) fn extract_interface_body(&self, interface_node: Node) -> String {
        let mut body_parts = Vec::new();
        let mut cursor = interface_node.walk();

        for child in interface_node.children(&mut cursor) {
            if child.kind() == "type_elem" {
                body_parts.push(self.get_node_text(child));
            }
        }

        body_parts.join("; ")
    }

    /// Extract type from receiver parameter (handle *Type and Type)
    pub(super) fn extract_receiver_type_from_param(&self, param_decl: Node) -> String {
        let mut cursor = param_decl.walk();
        for child in param_decl.children(&mut cursor) {
            if child.kind() == "type_identifier" {
                return self.get_node_text(child);
            } else if child.kind() == "pointer_type" {
                // Handle pointer types like *User
                let type_id = child
                    .children(&mut child.walk())
                    .find(|c| c.kind() == "type_identifier");
                return type_id
                    .map(|tid| self.get_node_text(tid))
                    .unwrap_or_default();
            }
        }
        String::new()
    }

    /// Extract return type from function signatures like "func getName() string"
    pub(super) fn extract_return_type_from_signature(&self, signature: &str) -> Option<String> {
        if let Some(paren_end) = signature.rfind(')') {
            let after_paren = signature[paren_end + 1..].trim();
            if !after_paren.is_empty() && after_paren != "{" {
                return Some(
                    after_paren
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .to_string(),
                );
            }
        }
        None
    }

    /// Extract type from variable signatures like "var name string = value" or "const name string = value"
    pub(super) fn extract_variable_type_from_signature(&self, signature: &str) -> Option<String> {
        if signature.starts_with("var ") || signature.starts_with("const ") {
            let parts: Vec<&str> = signature.split_whitespace().collect();
            if parts.len() >= 3 {
                let potential_type = parts[2];
                if potential_type != "=" {
                    return Some(potential_type.to_string());
                }
            }
        }
        None
    }
}
