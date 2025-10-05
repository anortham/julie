// Bash Extractor - Complete port of Miller's bash-extractor.ts
//
// Handles Bash/shell-specific constructs for DevOps tracing:
// - Functions and their definitions
// - Variables (local, environment, exported)
// - External command calls (critical for cross-language tracing!)
// - Script arguments and parameters
// - Conditional logic and loops
// - Source/include relationships
// - Docker, kubectl, npm, and other DevOps tool calls
//
// Special focus on cross-language tracing since Bash scripts often orchestrate
// other programs (Python, Node.js, Go binaries, Docker containers, etc.).

use crate::extractors::base::{
    BaseExtractor, Identifier, IdentifierKind, Relationship, RelationshipKind, Symbol, SymbolKind, SymbolOptions, Visibility,
};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;
use tree_sitter::Tree;

// Static regexes compiled once for performance
static PARAM_NUMBER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\$(\d+)").unwrap());
static INTEGER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d+$").unwrap());
static FLOAT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d+\.\d+$").unwrap());
static BOOLEAN_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(true|false)$").unwrap());

pub struct BashExtractor {
    base: BaseExtractor,
}

impl BashExtractor {
    pub fn new(language: String, file_path: String, content: String) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content),
        }
    }

    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.walk_tree_for_symbols(tree.root_node(), &mut symbols, None);
        symbols
    }

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

            // If this is a function, extract its positional parameters
            if sym.kind == SymbolKind::Function {
                let parameters = self.extract_positional_parameters(node, &sym.id);
                symbols.extend(parameters);
            }

            current_parent_id = Some(sym.id.clone());
        }

        // Recursively process child nodes
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_for_symbols(child, symbols, current_parent_id.clone());
        }
    }

    fn extract_symbol_from_node(
        &mut self,
        node: tree_sitter::Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        match node.kind() {
            "function_definition" => self.extract_function(node, parent_id),
            "variable_assignment" => self.extract_variable(node, parent_id),
            "declaration_command" => self.extract_declaration(node, parent_id),
            "command" | "simple_command" => self.extract_command(node, parent_id),
            "for_statement" | "while_statement" | "if_statement" => {
                self.extract_control_flow(node, parent_id)
            }
            _ => None,
        }
    }

    fn extract_function(
        &mut self,
        node: tree_sitter::Node,
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

    fn extract_positional_parameters(
        &mut self,
        func_node: tree_sitter::Node,
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

    #[allow(clippy::only_used_in_recursion)] // &self used in recursive calls
    fn collect_parameter_nodes<'a>(
        &self,
        node: tree_sitter::Node<'a>,
        param_nodes: &mut Vec<tree_sitter::Node<'a>>,
    ) {
        if matches!(node.kind(), "simple_expansion" | "expansion") {
            param_nodes.push(node);
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_parameter_nodes(child, param_nodes);
        }
    }

    fn extract_variable(
        &mut self,
        node: tree_sitter::Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        let name_node = self.find_variable_name_node(node)?;
        let name = self.base.get_node_text(&name_node);

        // Check if it's an environment variable or local variable
        let is_environment = self.is_environment_variable(node, &name);
        let is_exported = self.is_exported_variable(node);

        let options = SymbolOptions {
            signature: Some(self.extract_variable_signature(node)),
            visibility: if is_exported {
                Some(Visibility::Public)
            } else {
                Some(Visibility::Private)
            },
            parent_id: parent_id.map(|s| s.to_string()),
            doc_comment: Some(self.extract_variable_documentation(
                node,
                is_environment,
                is_exported,
                false,
            )),
            ..Default::default()
        };

        let symbol_kind = if is_environment {
            SymbolKind::Constant
        } else {
            SymbolKind::Variable
        };
        Some(self.base.create_symbol(&node, name, symbol_kind, options))
    }

    fn extract_declaration(
        &mut self,
        node: tree_sitter::Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        // Handle declare, export, readonly commands
        let declaration_text = self.base.get_node_text(&node);
        let declaration_type = declaration_text.split_whitespace().next()?;

        // Look for variable assignments within the declaration
        let assignments = self.get_children_of_type(node, "variable_assignment");
        if let Some(assignment) = assignments.first() {
            let assignment = *assignment;
            let name_node = self.find_variable_name_node(assignment)?;
            let name = self.base.get_node_text(&name_node);

            // Check if it's readonly: either 'readonly' command or 'declare -r'
            let is_readonly = declaration_type == "readonly"
                || declaration_type.contains("readonly")
                || (declaration_type == "declare" && declaration_text.contains(" -r "));

            // Check if it's an environment variable (but not if it's readonly)
            let is_environment = !is_readonly && self.is_environment_variable(assignment, &name);
            let is_exported = declaration_type == "export";

            let options = SymbolOptions {
                signature: Some(format!("{} {}", declaration_type, name)),
                visibility: if is_exported {
                    Some(Visibility::Public)
                } else {
                    Some(Visibility::Private)
                },
                parent_id: parent_id.map(|s| s.to_string()),
                doc_comment: Some(self.extract_variable_documentation(
                    assignment,
                    is_environment,
                    is_exported,
                    is_readonly,
                )),
                ..Default::default()
            };

            let symbol_kind = if is_readonly {
                SymbolKind::Constant
            } else {
                SymbolKind::Variable
            };
            return Some(
                self.base
                    .create_symbol(&assignment, name, symbol_kind, options),
            );
        }

        None
    }

    fn extract_command(
        &mut self,
        node: tree_sitter::Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        // Extract external commands - this is crucial for cross-language tracing!
        let command_name_node = self.find_command_name_node(node)?;
        let command_name = self.base.get_node_text(&command_name_node);

        // Focus on commands that call other programs/languages
        let cross_language_commands = [
            "python",
            "python3",
            "node",
            "npm",
            "bun",
            "deno",
            "go",
            "cargo",
            "rustc",
            "java",
            "javac",
            "mvn",
            "dotnet",
            "php",
            "ruby",
            "gem",
            "docker",
            "kubectl",
            "helm",
            "terraform",
            "git",
            "curl",
            "wget",
            "ssh",
            "scp",
        ];

        let is_interesting = cross_language_commands.contains(&command_name.as_str())
            || command_name.starts_with("./")
            || command_name.contains('/');

        if is_interesting {
            let options = SymbolOptions {
                signature: Some(self.extract_command_signature(node)),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                doc_comment: Some(self.get_command_documentation(&command_name)),
                ..Default::default()
            };

            Some(
                self.base
                    .create_symbol(&node, command_name, SymbolKind::Function, options),
            )
        } else {
            None
        }
    }

    fn extract_control_flow(
        &mut self,
        node: tree_sitter::Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        // Extract control flow constructs for understanding script logic
        let control_type = node.kind().replace("_statement", "");
        let name = format!("{} block", control_type);

        let options = SymbolOptions {
            signature: Some(self.extract_control_flow_signature(node)),
            visibility: Some(Visibility::Private),
            parent_id: parent_id.map(|s| s.to_string()),
            doc_comment: Some(format!("[{} control flow]", control_type.to_uppercase())),
            ..Default::default()
        };

        Some(
            self.base
                .create_symbol(&node, name, SymbolKind::Method, options),
        )
    }

    // Helper methods for variable analysis
    fn is_environment_variable(&self, _node: tree_sitter::Node, name: &str) -> bool {
        // Common environment variables
        let env_vars = [
            "PATH",
            "HOME",
            "USER",
            "PWD",
            "SHELL",
            "TERM",
            "NODE_ENV",
            "PYTHON_PATH",
            "JAVA_HOME",
            "GOPATH",
            "DOCKER_HOST",
            "KUBECONFIG",
        ];

        env_vars.contains(&name)
            || regex::Regex::new(r"^[A-Z_][A-Z0-9_]*$")
                .unwrap()
                .is_match(name)
    }

    fn is_exported_variable(&self, node: tree_sitter::Node) -> bool {
        // Check if the assignment is preceded by 'export'
        let mut current = node.prev_named_sibling();
        while let Some(sibling) = current {
            let text = self.base.get_node_text(&sibling);
            if text == "export" {
                return true;
            }
            current = sibling.prev_named_sibling();
        }
        false
    }

    // Signature extraction methods
    fn extract_function_signature(&self, node: tree_sitter::Node) -> String {
        let name_node = self.find_name_node(node);
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "unknown".to_string());
        format!("function {}()", name)
    }

    fn extract_variable_signature(&self, node: tree_sitter::Node) -> String {
        let name_node = self.find_variable_name_node(node);
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "unknown".to_string());

        // Get the full assignment text and extract value
        let full_text = self.base.get_node_text(&node);
        if let Some(equal_index) = full_text.find('=') {
            let value = full_text.get(equal_index + 1..).unwrap_or("").trim();
            if !value.is_empty() {
                return format!("{}={}", name, value);
            }
        }

        name
    }

    fn extract_command_signature(&self, node: tree_sitter::Node) -> String {
        // Get the full command with arguments
        let command_text = self.base.get_node_text(&node);

        // Limit length for readability
        if command_text.len() > 100 {
            format!("{}...", &command_text[..97])
        } else {
            command_text
        }
    }

    fn extract_control_flow_signature(&self, node: tree_sitter::Node) -> String {
        let control_type = node.kind().replace("_statement", "");

        // Try to extract the condition for if/while
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if matches!(child.kind(), "test_command" | "condition") {
                let condition = self.base.get_node_text(&child);
                let condition = if condition.len() > 50 {
                    format!("{}...", &condition[..47])
                } else {
                    condition
                };
                return format!("{} ({})", control_type, condition);
            }
        }

        format!("{} block", control_type)
    }

    // Documentation helpers
    fn extract_variable_documentation(
        &self,
        _node: tree_sitter::Node,
        is_environment: bool,
        is_exported: bool,
        is_readonly: bool,
    ) -> String {
        let mut annotations = Vec::new();

        if is_readonly {
            annotations.push("READONLY");
        }
        if is_environment {
            annotations.push("Environment Variable");
        }
        if is_exported {
            annotations.push("Exported");
        }

        if annotations.is_empty() {
            String::new()
        } else {
            format!("[{}]", annotations.join(", "))
        }
    }

    fn get_command_documentation(&self, command_name: &str) -> String {
        let command_docs = [
            ("python", "[Python Interpreter Call]"),
            ("python3", "[Python 3 Interpreter Call]"),
            ("node", "[Node.js Runtime Call]"),
            ("npm", "[NPM Package Manager Call]"),
            ("bun", "[Bun Runtime Call]"),
            ("go", "[Go Command Call]"),
            ("cargo", "[Rust Cargo Call]"),
            ("java", "[Java Runtime Call]"),
            ("dotnet", "[.NET CLI Call]"),
            ("docker", "[Docker Container Call]"),
            ("kubectl", "[Kubernetes CLI Call]"),
            ("terraform", "[Infrastructure as Code Call]"),
            ("git", "[Version Control Call]"),
        ]
        .iter()
        .cloned()
        .collect::<HashMap<&str, &str>>();

        command_docs
            .get(command_name)
            .unwrap_or(&"[External Program Call]")
            .to_string()
    }

    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        let mut relationships = Vec::new();
        self.walk_tree_for_relationships(tree.root_node(), symbols, &mut relationships);
        relationships
    }

    fn walk_tree_for_relationships(
        &mut self,
        node: tree_sitter::Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        match node.kind() {
            "command" | "simple_command" => {
                self.extract_command_relationships(node, symbols, relationships);
            }
            "command_substitution" => {
                self.extract_command_substitution_relationships(node, symbols, relationships);
            }
            "file_redirect" => {
                self.extract_file_relationships(node, symbols, relationships);
            }
            _ => {}
        }

        // Recursively process child nodes
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_for_relationships(child, symbols, relationships);
        }
    }

    fn extract_command_relationships(
        &mut self,
        node: tree_sitter::Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        // Extract relationships between functions and the commands they call
        if let Some(command_name_node) = self.find_command_name_node(node) {
            let command_name = self.base.get_node_text(&command_name_node);
            let command_symbol = symbols
                .iter()
                .find(|s| s.name == command_name && s.kind == SymbolKind::Function);

            if let Some(cmd_sym) = command_symbol {
                // Find the parent function that calls this command
                let mut current = node.parent();
                while let Some(parent_node) = current {
                    if parent_node.kind() == "function_definition" {
                        if let Some(func_name_node) = self.find_name_node(parent_node) {
                            let func_name = self.base.get_node_text(&func_name_node);
                            let func_symbol = symbols
                                .iter()
                                .find(|s| s.name == func_name && s.kind == SymbolKind::Function);

                            if let Some(func_sym) = func_symbol {
                                if func_sym.id != cmd_sym.id {
                                    let relationship = self.base.create_relationship(
                                        func_sym.id.clone(),
                                        cmd_sym.id.clone(),
                                        RelationshipKind::Calls,
                                        &node,
                                        Some(1.0),
                                        None,
                                    );
                                    relationships.push(relationship);
                                }
                            }
                        }
                        break;
                    }
                    current = parent_node.parent();
                }
            }
        }
    }

    fn extract_command_substitution_relationships(
        &mut self,
        _node: tree_sitter::Node,
        _symbols: &[Symbol],
        _relationships: &mut Vec<Relationship>,
    ) {
        // Extract relationships for command substitutions $(command) or `command`
        // These show data flow dependencies
        // TODO: Implement if needed for additional relationship extraction
    }

    fn extract_file_relationships(
        &mut self,
        _node: tree_sitter::Node,
        _symbols: &[Symbol],
        _relationships: &mut Vec<Relationship>,
    ) {
        // Extract relationships for file redirections and pipes
        // These show data flow between commands
        // TODO: Implement if needed for additional relationship extraction
    }

    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        let mut types = HashMap::new();

        for symbol in symbols {
            if matches!(symbol.kind, SymbolKind::Variable | SymbolKind::Constant) {
                // Infer type from signature
                let signature = symbol.signature.as_deref().unwrap_or("");
                let mut var_type = "string".to_string();

                if let Some(value_part) = signature.split('=').nth(1) {
                    let value = value_part.trim().trim_matches(|c| c == '"' || c == '\'');

                    if INTEGER_RE.is_match(value) {
                        var_type = "integer".to_string();
                    } else if FLOAT_RE.is_match(value) {
                        var_type = "float".to_string();
                    } else if BOOLEAN_RE.is_match(&value.to_lowercase()) {
                        var_type = "boolean".to_string();
                    } else if value.starts_with('/') || value.contains('/') {
                        var_type = "path".to_string();
                    }
                }

                types.insert(symbol.name.clone(), var_type);
            }
        }

        types
    }

    // Helper methods for finding specific node types
    fn find_name_node<'a>(&self, node: tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
        // Look for function name nodes
        if let Some(name_field) = node.child_by_field_name("name") {
            return Some(name_field);
        }

        // Fallback: look for 'word' or 'identifier' children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if matches!(child.kind(), "word" | "identifier") {
                return Some(child);
            }
        }
        None
    }

    #[allow(clippy::manual_find)] // Manual loops required for borrow checker
    fn find_variable_name_node<'a>(
        &self,
        node: tree_sitter::Node<'a>,
    ) -> Option<tree_sitter::Node<'a>> {
        // Look for variable name in assignments
        if let Some(name_field) = node.child_by_field_name("name") {
            return Some(name_field);
        }

        // Look for variable_name child
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "variable_name" {
                return Some(child);
            }
        }

        // Fallback: look for word child (first one usually)
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "word" {
                return Some(child);
            }
        }
        None
    }

    #[allow(clippy::manual_find)] // Manual loops required for borrow checker
    fn find_command_name_node<'a>(
        &self,
        node: tree_sitter::Node<'a>,
    ) -> Option<tree_sitter::Node<'a>> {
        // Look for command name field
        if let Some(name_field) = node.child_by_field_name("name") {
            return Some(name_field);
        }

        // Look for command_name child
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "command_name" {
                return Some(child);
            }
        }

        // Fallback: first word child
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "word" {
                return Some(child);
            }
        }
        None
    }

    fn get_children_of_type<'a>(
        &self,
        node: tree_sitter::Node<'a>,
        node_type: &str,
    ) -> Vec<tree_sitter::Node<'a>> {
        let mut children = Vec::new();
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            if child.kind() == node_type {
                children.push(child);
            }
        }

        children
    }

    #[allow(dead_code)]
    #[allow(clippy::only_used_in_recursion)] // &self used in recursive calls
    fn walk_tree<'a, F>(&self, node: tree_sitter::Node<'a>, callback: &mut F)
    where
        F: FnMut(tree_sitter::Node<'a>),
    {
        callback(node);
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree(child, callback);
        }
    }

    // ========================================================================
    // Identifier Extraction (for LSP-quality find_references)
    // ========================================================================

    /// Extract all identifier usages (command calls, array access, etc.)
    /// Following the Rust extractor reference implementation pattern
    pub fn extract_identifiers(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Identifier> {
        // Create symbol map for fast lookup
        let symbol_map: HashMap<String, &Symbol> = symbols.iter().map(|s| (s.id.clone(), s)).collect();

        // Walk the tree and extract identifiers
        self.walk_tree_for_identifiers(tree.root_node(), &symbol_map);

        // Return the collected identifiers
        self.base.identifiers.clone()
    }

    /// Recursively walk tree extracting identifiers from each node
    fn walk_tree_for_identifiers(
        &mut self,
        node: tree_sitter::Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) {
        // Extract identifier from this node if applicable
        self.extract_identifier_from_node(node, symbol_map);

        // Recursively walk children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_for_identifiers(child, symbol_map);
        }
    }

    /// Extract identifier from a single node based on its kind
    fn extract_identifier_from_node(
        &mut self,
        node: tree_sitter::Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) {
        match node.kind() {
            // Command invocations: build_app, npm install, etc.
            "command" => {
                // Extract command name using the existing helper
                if let Some(command_name_node) = self.find_command_name_node(node) {
                    let name = self.base.get_node_text(&command_name_node);
                    let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                    self.base.create_identifier(
                        &command_name_node,
                        name,
                        IdentifierKind::Call,
                        containing_symbol_id,
                    );
                }
            }

            // Array/subscript access: ${arr[0]}, ${data[key]}
            "subscript" => {
                // The subscript node should have a name or identifier child
                // Extract the variable name being accessed
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "variable_name" || child.kind() == "simple_expansion" {
                        let name = self.base.get_node_text(&child);
                        // Clean up the name - remove $ prefix if present
                        let clean_name = name.trim_start_matches('$').to_string();
                        let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                        self.base.create_identifier(
                            &child,
                            clean_name,
                            IdentifierKind::MemberAccess,
                            containing_symbol_id,
                        );
                        break;
                    }
                }
            }

            _ => {
                // Skip other node types for now
            }
        }
    }

    /// Find the ID of the symbol that contains this node
    /// CRITICAL: Only search symbols from THIS FILE (file-scoped filtering)
    fn find_containing_symbol_id(
        &self,
        node: tree_sitter::Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) -> Option<String> {
        // CRITICAL FIX: Only search symbols from THIS FILE, not all files
        // Bug was: searching all symbols in DB caused wrong file symbols to match
        let file_symbols: Vec<Symbol> = symbol_map
            .values()
            .filter(|s| s.file_path == self.base.file_path)
            .map(|&s| s.clone())
            .collect();

        self.base
            .find_containing_symbol(&node, &file_symbols)
            .map(|s| s.id.clone())
    }
}
