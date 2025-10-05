// PowerShell language extractor - port of Miller's PowerShell extractor
// Handles PowerShell-specific constructs for Windows/Azure DevOps

use crate::extractors::base::{
    BaseExtractor, Identifier, IdentifierKind, Relationship, RelationshipKind, Symbol, SymbolKind, SymbolOptions, Visibility,
};
use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;
use tree_sitter::{Node, Tree};

// Static regexes compiled once for performance
static TYPE_ANNOTATION_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[(\w+)\]").unwrap());
static INTEGER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d+$").unwrap());
static FLOAT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d+\.\d+$").unwrap());
static BOOL_VAR_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\$(true|false)$").unwrap());
static TYPE_BRACKET_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\[.*\]$").unwrap());
static FUNCTION_NAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"function\s+([A-Za-z][A-Za-z0-9-_]*)").unwrap());

/// PowerShell language extractor that handles PowerShell-specific constructs for Windows/Azure DevOps:
/// - Functions (simple and advanced with [CmdletBinding()])
/// - Variables (scoped, environment, automatic variables)
/// - Classes, methods, properties, and enums (PowerShell 5.0+)
/// - Azure PowerShell cmdlets and Windows management commands
/// - Module imports, exports, and using statements
/// - Parameter definitions with attributes and validation
/// - Cross-platform DevOps tool calls (docker, kubectl, az CLI)
///
/// Special focus on Windows/Azure DevOps tracing to complement Bash for complete
/// cross-platform deployment automation coverage.
pub struct PowerShellExtractor {
    base: BaseExtractor,
}

impl PowerShellExtractor {
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
        node: Node,
        symbols: &mut Vec<Symbol>,
        parent_id: Option<String>,
    ) {
        let mut current_parent_id = parent_id;

        if let Some(symbol) = self.extract_symbol_from_node(node, current_parent_id.as_deref()) {
            // If this is a function, extract its parameters
            if symbol.kind == SymbolKind::Function {
                let parameters = self.extract_function_parameters(node, &symbol.id);
                symbols.extend(parameters);
            }

            current_parent_id = Some(symbol.id.clone());
            symbols.push(symbol);
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_for_symbols(child, symbols, current_parent_id.clone());
        }
    }

    fn extract_symbol_from_node(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        match node.kind() {
            "function_statement" => self.extract_function(node, parent_id),
            "param_block" => self.extract_advanced_function(node, parent_id),
            "assignment_expression" => self.extract_variable(node, parent_id),
            "variable" => self.extract_variable_reference(node, parent_id),
            "class_statement" => self.extract_class(node, parent_id),
            "class_method_definition" => self.extract_method(node, parent_id),
            "class_property_definition" => self.extract_property(node, parent_id),
            "enum_statement" => self.extract_enum(node, parent_id),
            "enum_member" => self.extract_enum_member(node, parent_id),
            "import_statement" | "using_statement" => self.extract_import(node, parent_id),
            "command" | "command_expression" | "pipeline" => self.extract_command(node, parent_id),
            _ => None,
        }
    }

    fn extract_function(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let name_node = self.find_function_name_node(node)?;
        let name = self.base.get_node_text(&name_node);

        // Check if it's an advanced function with [CmdletBinding()]
        let is_advanced = self.has_attribute(node, "CmdletBinding");

        let signature = self.extract_function_signature(node);
        let doc_comment = if is_advanced {
            Some("Advanced PowerShell function with [CmdletBinding()]".to_string())
        } else {
            None
        };

        Some(self.base.create_symbol(
            &node,
            name,
            SymbolKind::Function,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public), // PowerShell functions are generally public
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: None,
                doc_comment,
            },
        ))
    }

    fn extract_advanced_function(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        // For param_block nodes (advanced functions), extract function name from ERROR node content
        let function_name = self.extract_function_name_from_param_block(node)?;

        // Check for CmdletBinding attribute
        let _has_cmdlet_binding = self.has_attribute(node, "CmdletBinding");

        let signature = self.extract_advanced_function_signature(node, &function_name);

        Some(self.base.create_symbol(
            &node,
            function_name,
            SymbolKind::Function,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: None,
                doc_comment: Some(
                    "Advanced PowerShell function with [CmdletBinding()]".to_string(),
                ),
            },
        ))
    }

    fn extract_function_parameters(&mut self, func_node: Node, parent_id: &str) -> Vec<Symbol> {
        let mut parameters = Vec::new();

        // Handle simple functions - look for param_block with parameter_definition
        let param_blocks = self.find_nodes_by_type(func_node, "param_block");
        for param_block in param_blocks {
            let param_defs = self.find_nodes_by_type(param_block, "parameter_definition");

            for param_def in param_defs {
                if let Some(name_node) = self.find_parameter_name_node(param_def) {
                    let param_name = self.base.get_node_text(&name_node).replace("$", "");
                    let is_mandatory = self.has_parameter_attribute(param_def, "Mandatory");

                    let signature = self.extract_parameter_signature(param_def);
                    let doc_comment = if is_mandatory {
                        Some("Mandatory parameter".to_string())
                    } else {
                        Some("Optional parameter".to_string())
                    };

                    let param_symbol = self.base.create_symbol(
                        &param_def,
                        param_name,
                        SymbolKind::Variable,
                        SymbolOptions {
                            signature: Some(signature),
                            visibility: Some(Visibility::Public),
                            parent_id: Some(parent_id.to_string()),
                            metadata: None,
                            doc_comment,
                        },
                    );

                    parameters.push(param_symbol);
                }
            }
        }

        // Handle advanced functions - look for parameter_list with script_parameter
        let param_lists = self.find_nodes_by_type(func_node, "parameter_list");
        for param_list in param_lists {
            let script_params = self.find_nodes_by_type(param_list, "script_parameter");

            for script_param in script_params {
                // Find the direct child variable node (the parameter name), not any variable in the subtree
                let mut cursor = script_param.walk();
                let children: Vec<_> = script_param.children(&mut cursor).collect();
                if let Some(variable_node) = children
                    .into_iter()
                    .find(|child| child.kind() == "variable")
                {
                    let param_name = self.base.get_node_text(&variable_node).replace("$", "");
                    let is_mandatory = self.has_parameter_attribute(script_param, "Mandatory");

                    let signature = self.extract_script_parameter_signature(script_param);
                    let doc_comment = if is_mandatory {
                        Some("Mandatory parameter".to_string())
                    } else {
                        Some("Optional parameter".to_string())
                    };

                    let param_symbol = self.base.create_symbol(
                        &script_param,
                        param_name,
                        SymbolKind::Variable,
                        SymbolOptions {
                            signature: Some(signature),
                            visibility: Some(Visibility::Public),
                            parent_id: Some(parent_id.to_string()),
                            metadata: None,
                            doc_comment,
                        },
                    );

                    parameters.push(param_symbol);
                }
            }
        }

        parameters
    }

    fn extract_variable(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let name_node = self.find_variable_name_node(node)?;
        let mut name = self.base.get_node_text(&name_node);

        // Remove $ prefix and scope qualifiers
        name = name
            .replace("$", "")
            .replace("Global:", "")
            .replace("Script:", "")
            .replace("Local:", "")
            .replace("Using:", "");

        // Determine scope and visibility
        let full_text = self.base.get_node_text(&name_node);
        let is_global = full_text.contains("Global:");
        let is_script = full_text.contains("Script:");
        let is_environment = full_text.contains("env:") || self.is_environment_variable(&name);
        let is_automatic = self.is_automatic_variable(&name);

        let signature = self.extract_variable_signature(node);
        let visibility = if is_global {
            Visibility::Public
        } else {
            Visibility::Private
        };
        let doc_comment =
            self.get_variable_documentation(is_environment, is_automatic, is_global, is_script);

        Some(self.base.create_symbol(
            &node,
            name,
            SymbolKind::Variable,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: None,
                doc_comment: if doc_comment.is_empty() {
                    None
                } else {
                    Some(doc_comment)
                },
            },
        ))
    }

    fn extract_variable_reference(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        let mut name = self.base.get_node_text(&node);

        // Remove $ prefix and scope qualifiers
        name = name
            .replace("$", "")
            .replace("Global:", "")
            .replace("Script:", "")
            .replace("Local:", "")
            .replace("Using:", "")
            .replace("env:", "");

        // Only extract automatic variables, environment variables, and special variables
        // to avoid creating symbols for every variable reference
        let is_automatic = self.is_automatic_variable(&name);
        let is_environment =
            self.is_environment_variable(&name) || self.base.get_node_text(&node).contains("env:");

        if !is_automatic && !is_environment {
            return None; // Skip regular variable references
        }

        // Determine scope and visibility
        let full_text = self.base.get_node_text(&node);
        let is_global = is_automatic || full_text.contains("Global:");

        let visibility = if is_global {
            Visibility::Public
        } else {
            Visibility::Private
        };
        let doc_comment =
            self.get_variable_documentation(is_environment, is_automatic, is_global, false);

        Some(self.base.create_symbol(
            &node,
            name,
            SymbolKind::Variable,
            SymbolOptions {
                signature: Some(full_text), // Use the full variable reference as signature
                visibility: Some(visibility),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: None,
                doc_comment: if doc_comment.is_empty() {
                    None
                } else {
                    Some(doc_comment)
                },
            },
        ))
    }

    fn extract_class(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let name_node = self.find_class_name_node(node)?;
        let name = self.base.get_node_text(&name_node);

        let signature = self.extract_class_signature(node);

        Some(self.base.create_symbol(
            &node,
            name,
            SymbolKind::Class,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: None,
                doc_comment: None,
            },
        ))
    }

    fn extract_method(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let name_node = self.find_method_name_node(node)?;
        let name = self.base.get_node_text(&name_node);
        let is_static = self.has_modifier(node, "static");
        let is_hidden = self.has_modifier(node, "hidden");

        let signature = self.extract_method_signature(node);
        let visibility = if is_hidden {
            Visibility::Private
        } else {
            Visibility::Public
        };
        let doc_comment = if is_static {
            Some("Static method".to_string())
        } else {
            None
        };

        Some(self.base.create_symbol(
            &node,
            name,
            SymbolKind::Method,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: None,
                doc_comment,
            },
        ))
    }

    fn extract_property(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let name_node = self.find_property_name_node(node)?;
        let mut name = self.base.get_node_text(&name_node);
        name = name.replace("$", ""); // Remove $ prefix

        let is_hidden = self.has_modifier(node, "hidden");
        let signature = self.extract_property_signature(node);
        let visibility = if is_hidden {
            Visibility::Private
        } else {
            Visibility::Public
        };

        Some(self.base.create_symbol(
            &node,
            name,
            SymbolKind::Property,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: None,
                doc_comment: None,
            },
        ))
    }

    fn extract_enum(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let name_node = self.find_enum_name_node(node)?;
        let name = self.base.get_node_text(&name_node);

        let signature = format!("enum {}", name);

        Some(self.base.create_symbol(
            &node,
            name,
            SymbolKind::Enum,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: None,
                doc_comment: None,
            },
        ))
    }

    fn extract_enum_member(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let name_node = self.find_enum_member_name_node(node)?;
        let name = self.base.get_node_text(&name_node);
        let value = self.extract_enum_member_value(node);

        let signature = if let Some(val) = value {
            format!("{} = {}", name, val)
        } else {
            name.clone()
        };

        Some(self.base.create_symbol(
            &node,
            name,
            SymbolKind::EnumMember,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: None,
                doc_comment: None,
            },
        ))
    }

    fn extract_import(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let module_name_node = self.find_module_name_node(node)?;
        let mut module_name = self.base.get_node_text(&module_name_node);
        module_name = module_name.replace("'", "").replace("\"", ""); // Remove quotes

        let node_text = self.base.get_node_text(&node);
        let is_using = node_text.starts_with("using");
        let is_dot_sourcing = node_text.starts_with(".");

        let doc_comment = if is_using {
            Some("Using statement".to_string())
        } else if is_dot_sourcing {
            Some("Dot sourcing".to_string())
        } else {
            Some("Module import".to_string())
        };

        Some(self.base.create_symbol(
            &node,
            module_name,
            SymbolKind::Import,
            SymbolOptions {
                signature: Some(node_text.trim().to_string()),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: None,
                doc_comment,
            },
        ))
    }

    fn extract_command(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        // Check for dot sourcing first (special case with different AST structure)
        let mut cursor = node.walk();
        let dot_source_node = node.children(&mut cursor).find(|child| {
            child.kind() == "command_invokation_operator" && self.base.get_node_text(child) == "."
        });

        if dot_source_node.is_some() {
            return self.extract_dot_sourcing(node, parent_id);
        }

        let command_name_node = self.find_command_name_node(node)?;
        let command_name = self.base.get_node_text(&command_name_node);

        // Check for import/module commands first
        let import_commands = ["Import-Module", "Export-ModuleMember", "using"];
        if import_commands.contains(&command_name.as_str()) {
            return self.extract_import_command(node, &command_name, parent_id);
        }

        // Focus on Azure, Windows, and cross-platform DevOps commands
        let devops_commands = [
            // Azure PowerShell
            "Connect-AzAccount",
            "Set-AzContext",
            "New-AzResourceGroup",
            "New-AzResourceGroupDeployment",
            "New-AzContainerGroup",
            "New-AzAksCluster",
            "Get-AzAksCluster",
            // Windows Management
            "Enable-WindowsOptionalFeature",
            "Install-WindowsFeature",
            "Set-ItemProperty",
            "Set-Service",
            "Start-Service",
            "New-Item",
            "Copy-Item",
            // Cross-platform DevOps
            "docker",
            "kubectl",
            "az",
            // PowerShell Core
            "Invoke-Command",
        ];

        let is_interesting = devops_commands.contains(&command_name.as_str())
            || command_name.starts_with("Connect-")
            || command_name.starts_with("New-")
            || command_name.starts_with("Set-")
            || command_name.starts_with("Get-");

        if is_interesting {
            let signature = self.extract_command_signature(node);
            let doc_comment = self.get_command_documentation(&command_name);

            Some(self.base.create_symbol(
                &node,
                command_name,
                SymbolKind::Function,
                SymbolOptions {
                    signature: Some(signature),
                    visibility: Some(Visibility::Public),
                    parent_id: parent_id.map(|s| s.to_string()),
                    metadata: None,
                    doc_comment: Some(doc_comment),
                },
            ))
        } else {
            None
        }
    }

    fn extract_import_command(
        &mut self,
        node: Node,
        command_name: &str,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        let node_text = self.base.get_node_text(&node);
        let mut module_name = String::new();
        let signature = node_text.trim().to_string();

        if command_name == "Import-Module" {
            // Extract module name from "Import-Module Az.Accounts" or "Import-Module -Name 'Custom.Tools'"
            if let Some(captures) = regex::Regex::new(
                r#"Import-Module\s+(?:-Name\s+["']?([^"'\s]+)["']?|([A-Za-z0-9.-]+))"#,
            )
            .unwrap()
            .captures(&node_text)
            {
                module_name = captures
                    .get(1)
                    .or_else(|| captures.get(2))
                    .map_or("unknown".to_string(), |m| m.as_str().to_string());
            }
        } else if command_name == "using" {
            // Extract from "using namespace System.Collections.Generic" or "using module Az.Storage"
            if let Some(captures) =
                regex::Regex::new(r"using\s+(?:namespace|module)\s+([A-Za-z0-9.-_]+)")
                    .unwrap()
                    .captures(&node_text)
            {
                module_name = captures
                    .get(1)
                    .map_or("unknown".to_string(), |m| m.as_str().to_string());
            }
        } else if command_name == "Export-ModuleMember" {
            // Extract the type being exported (Function, Variable, Alias)
            if let Some(captures) = regex::Regex::new(r"Export-ModuleMember\s+-(\w+)")
                .unwrap()
                .captures(&node_text)
            {
                module_name = captures
                    .get(1)
                    .map_or("unknown".to_string(), |m| m.as_str().to_string());
            } else {
                // Fallback: try to extract from the full text
                if node_text.contains("-Function") {
                    module_name = "Function".to_string();
                } else if node_text.contains("-Variable") {
                    module_name = "Variable".to_string();
                } else if node_text.contains("-Alias") {
                    module_name = "Alias".to_string();
                } else {
                    module_name = "ModuleMember".to_string();
                }
            }
        }

        if module_name.is_empty() || module_name == "unknown" {
            return None;
        }

        let is_using = command_name == "using";
        let is_export = command_name == "Export-ModuleMember";

        let symbol_kind = if is_export {
            SymbolKind::Export
        } else {
            SymbolKind::Import
        };
        let doc_comment = if is_export {
            Some("Module export".to_string())
        } else if is_using {
            Some("Using statement".to_string())
        } else {
            Some("Module import".to_string())
        };

        Some(self.base.create_symbol(
            &node,
            module_name,
            symbol_kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: None,
                doc_comment,
            },
        ))
    }

    fn extract_dot_sourcing(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        // Extract script path from dot sourcing like '. "$PSScriptRoot\CommonFunctions.ps1"'
        let mut cursor = node.walk();
        let command_name_expr_node = node
            .children(&mut cursor)
            .find(|child| child.kind() == "command_name_expr")?;

        let script_path = self.base.get_node_text(&command_name_expr_node);
        let signature = self.base.get_node_text(&node).trim().to_string();

        // Extract just the filename for the symbol name
        let mut file_name = script_path.replace("'", "").replace("\"", ""); // Remove quotes
        let last_slash = file_name.rfind('\\').max(file_name.rfind('/'));
        if let Some(pos) = last_slash {
            file_name = file_name[(pos + 1)..].to_string();
        }

        // Remove .ps1 extension for cleaner symbol name
        if file_name.ends_with(".ps1") {
            file_name = file_name[..file_name.len() - 4].to_string();
        }

        Some(self.base.create_symbol(
            &node,
            file_name,
            SymbolKind::Import,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: None,
                doc_comment: Some("Dot sourcing script".to_string()),
            },
        ))
    }

    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        let mut relationships = Vec::new();
        self.walk_tree_for_relationships(tree.root_node(), symbols, &mut relationships);
        relationships
    }

    fn walk_tree_for_relationships(
        &self,
        node: Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        match node.kind() {
            "command_expression" | "pipeline_expression" => {
                self.extract_command_relationships(node, symbols, relationships);
            }
            "class_definition" => {
                self.extract_inheritance_relationships(node, symbols, relationships);
            }
            _ => {}
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_for_relationships(child, symbols, relationships);
        }
    }

    fn extract_command_relationships(
        &self,
        node: Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        if let Some(command_name_node) = self.find_command_name_node(node) {
            let command_name = self.base.get_node_text(&command_name_node);
            if let Some(command_symbol) = symbols
                .iter()
                .find(|s| s.name == command_name && s.kind == SymbolKind::Function)
            {
                // Find the parent function that calls this command
                let mut current = Some(node);
                while let Some(n) = current {
                    if n.kind() == "function_definition" {
                        if let Some(func_name_node) = self.find_function_name_node(n) {
                            let func_name = self.base.get_node_text(&func_name_node);
                            if let Some(func_symbol) = symbols
                                .iter()
                                .find(|s| s.name == func_name && s.kind == SymbolKind::Function)
                            {
                                if func_symbol.id != command_symbol.id {
                                    relationships.push(self.base.create_relationship(
                                        func_symbol.id.clone(),
                                        command_symbol.id.clone(),
                                        RelationshipKind::Calls,
                                        &node,
                                        None,
                                        None,
                                    ));
                                }
                            }
                        }
                        break;
                    }
                    current = n.parent();
                }
            }
        }
    }

    fn extract_inheritance_relationships(
        &self,
        node: Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        if let Some(inheritance) = self.extract_inheritance(node) {
            if let Some(class_name_node) = self.find_class_name_node(node) {
                let class_name = self.base.get_node_text(&class_name_node);
                let child_class = symbols
                    .iter()
                    .find(|s| s.name == class_name && s.kind == SymbolKind::Class);
                let parent_class = symbols
                    .iter()
                    .find(|s| s.name == inheritance && s.kind == SymbolKind::Class);

                if let (Some(child), Some(parent)) = (child_class, parent_class) {
                    relationships.push(self.base.create_relationship(
                        child.id.clone(),
                        parent.id.clone(),
                        RelationshipKind::Extends,
                        &node,
                        None,
                        None,
                    ));
                }
            }
        }
    }

    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        let mut types = HashMap::new();

        for symbol in symbols {
            if symbol.kind == SymbolKind::Variable || symbol.kind == SymbolKind::Property {
                let signature = symbol.signature.as_ref().map_or("", |s| s.as_str());
                let mut type_name = "object".to_string();

                // Extract type from PowerShell type annotations
                if let Some(captures) = TYPE_ANNOTATION_RE.captures(signature) {
                    type_name = captures.get(1).unwrap().as_str().to_lowercase();
                } else if signature.contains("=") {
                    // Infer from value
                    let value = signature.split('=').nth(1).map_or("", |v| v.trim());
                    if INTEGER_RE.is_match(value) {
                        type_name = "int".to_string();
                    } else if FLOAT_RE.is_match(value) {
                        type_name = "double".to_string();
                    } else if BOOL_VAR_RE.is_match(value) {
                        type_name = "bool".to_string();
                    } else if value.starts_with('"') || value.starts_with("'") {
                        type_name = "string".to_string();
                    } else if value.starts_with("@(") {
                        type_name = "array".to_string();
                    } else if value.starts_with("@{") {
                        type_name = "hashtable".to_string();
                    }
                }

                types.insert(symbol.name.clone(), type_name);
            }
        }

        types
    }

    // Helper methods for node finding
    fn find_function_name_node<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();
        children
            .into_iter()
            .find(|child| matches!(child.kind(), "function_name" | "identifier" | "cmdlet_name"))
    }

    fn find_variable_name_node<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();
        children.into_iter().find(|child| {
            matches!(
                child.kind(),
                "left_assignment_expression" | "variable" | "identifier"
            )
        })
    }

    fn find_parameter_name_node<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();
        children
            .into_iter()
            .find(|child| matches!(child.kind(), "variable" | "parameter_name"))
    }

    fn find_class_name_node<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();
        children
            .into_iter()
            .find(|child| matches!(child.kind(), "simple_name" | "identifier" | "type_name"))
    }

    fn find_method_name_node<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();
        children
            .into_iter()
            .find(|child| matches!(child.kind(), "simple_name" | "identifier" | "method_name"))
    }

    fn find_property_name_node<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();
        children
            .into_iter()
            .find(|child| matches!(child.kind(), "variable" | "property_name" | "identifier"))
    }

    fn find_enum_name_node<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();
        children
            .into_iter()
            .find(|child| matches!(child.kind(), "simple_name" | "identifier" | "type_name"))
    }

    fn find_enum_member_name_node<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();
        children
            .into_iter()
            .find(|child| matches!(child.kind(), "simple_name" | "identifier"))
    }

    fn extract_enum_member_value(&self, node: Node) -> Option<String> {
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();

        // Look for assignment pattern: name = value
        for (i, child) in children.iter().enumerate() {
            if child.kind() == "=" && i + 1 < children.len() {
                return Some(self.base.get_node_text(&children[i + 1]));
            }
        }
        None
    }

    fn find_module_name_node<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();
        children
            .into_iter()
            .find(|child| matches!(child.kind(), "string" | "identifier" | "module_name"))
    }

    fn find_command_name_node<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();
        children
            .into_iter()
            .find(|child| matches!(child.kind(), "command_name" | "identifier" | "cmdlet_name"))
    }

    // Signature extraction methods
    fn extract_function_signature(&self, node: Node) -> String {
        let name = self
            .find_function_name_node(node)
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "unknown".to_string());

        let has_attributes = self.has_attribute(node, "CmdletBinding");
        let prefix = if has_attributes {
            "[CmdletBinding()] "
        } else {
            ""
        };

        format!("{}function {}()", prefix, name)
    }

    fn extract_parameter_signature(&self, node: Node) -> String {
        let name = self
            .find_parameter_name_node(node)
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "unknown".to_string());

        let attributes = self.extract_parameter_attributes(node);
        if !attributes.is_empty() {
            format!("{} {}", attributes, name)
        } else {
            name
        }
    }

    fn extract_script_parameter_signature(&self, node: Node) -> String {
        // Extract variable name
        let name = self
            .find_nodes_by_type(node, "variable")
            .first()
            .map(|n| self.base.get_node_text(n))
            .unwrap_or_else(|| "$unknown".to_string());

        // Extract type and attributes from attribute_list
        let attribute_list = self.find_nodes_by_type(node, "attribute_list");
        if attribute_list.is_empty() {
            return name;
        }

        let mut attributes = Vec::new();
        let attribute_nodes = self.find_nodes_by_type(attribute_list[0], "attribute");

        for attr in attribute_nodes {
            let attr_text = self.base.get_node_text(&attr);

            // Collect Parameter attributes and type brackets (like [string], [switch])
            if attr_text.contains("Parameter") || TYPE_BRACKET_RE.is_match(&attr_text) {
                attributes.push(attr_text);
            }
        }

        if !attributes.is_empty() {
            format!("{} {}", attributes.join(" "), name)
        } else {
            name
        }
    }

    fn extract_variable_signature(&self, node: Node) -> String {
        let full_text = self.base.get_node_text(&node);
        let equal_index = full_text.find('=');

        if let Some(pos) = equal_index {
            if pos < full_text.len() - 1 {
                return full_text.trim().to_string();
            }
        }

        self.find_variable_name_node(node)
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "unknown".to_string())
    }

    fn extract_class_signature(&self, node: Node) -> String {
        let name = self
            .find_class_name_node(node)
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "unknown".to_string());

        // Check for inheritance
        if let Some(inheritance) = self.extract_inheritance(node) {
            format!("class {} : {}", name, inheritance)
        } else {
            format!("class {}", name)
        }
    }

    fn extract_method_signature(&self, node: Node) -> String {
        let name = self
            .find_method_name_node(node)
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "unknown".to_string());

        let return_type = self.extract_return_type(node);
        let is_static = self.has_modifier(node, "static");

        let prefix = if is_static { "static " } else { "" };
        let suffix = return_type.map_or(String::new(), |t| format!(" {}", t));

        format!("{}{} {}()", prefix, suffix, name)
    }

    fn extract_property_signature(&self, node: Node) -> String {
        let name = self
            .find_property_name_node(node)
            .map(|n| self.base.get_node_text(&n).replace("$", ""))
            .unwrap_or_else(|| "unknown".to_string());

        let property_type = self.extract_property_type(node);
        let is_hidden = self.has_modifier(node, "hidden");

        let prefix = if is_hidden { "hidden " } else { "" };
        if let Some(ptype) = property_type {
            format!("{}{}${}", prefix, ptype, name)
        } else {
            format!("{}${}", prefix, name)
        }
    }

    fn extract_command_signature(&self, node: Node) -> String {
        let command_text = self.base.get_node_text(&node);
        if command_text.len() > 100 {
            format!("{}...", &command_text[..97])
        } else {
            command_text
        }
    }

    // Helper methods for attributes and modifiers
    fn has_attribute(&self, node: Node, attribute_name: &str) -> bool {
        let node_text = self.base.get_node_text(&node);
        node_text.contains(&format!("[{}", attribute_name))
    }

    fn has_parameter_attribute(&self, node: Node, attribute_name: &str) -> bool {
        let node_text = self.base.get_node_text(&node);
        node_text.contains(&format!("{}=$true", attribute_name))
            || node_text.contains(&format!("{}=true", attribute_name))
    }

    fn has_modifier(&self, node: Node, modifier: &str) -> bool {
        let node_text = self.base.get_node_text(&node);
        node_text.contains(modifier)
    }

    fn extract_parameter_attributes(&self, node: Node) -> String {
        let node_text = self.base.get_node_text(&node);
        if let Some(captures) = regex::Regex::new(r"\[Parameter[^\]]*\]")
            .unwrap()
            .captures(&node_text)
        {
            captures.get(0).unwrap().as_str().to_string()
        } else {
            String::new()
        }
    }

    fn extract_inheritance(&self, node: Node) -> Option<String> {
        let node_text = self.base.get_node_text(&node);
        regex::Regex::new(r":\s*(\w+)")
            .unwrap()
            .captures(&node_text)
            .map(|captures| captures.get(1).unwrap().as_str().to_string())
    }

    fn extract_return_type(&self, node: Node) -> Option<String> {
        let node_text = self.base.get_node_text(&node);
        regex::Regex::new(r"\[(\w+)\]")
            .unwrap()
            .captures(&node_text)
            .map(|captures| format!("[{}]", captures.get(1).unwrap().as_str()))
    }

    fn extract_property_type(&self, node: Node) -> Option<String> {
        let node_text = self.base.get_node_text(&node);
        regex::Regex::new(r"\[(\w+)\]")
            .unwrap()
            .captures(&node_text)
            .map(|captures| format!("[{}]", captures.get(1).unwrap().as_str()))
    }

    // Variable classification methods
    fn is_environment_variable(&self, name: &str) -> bool {
        let env_vars = [
            "PATH",
            "COMPUTERNAME",
            "USERNAME",
            "TEMP",
            "TMP",
            "USERPROFILE",
            "AZURE_CLIENT_ID",
            "AZURE_CLIENT_SECRET",
            "AZURE_TENANT_ID",
            "POWERSHELL_TELEMETRY_OPTOUT",
        ];
        env_vars.contains(&name)
            || regex::Regex::new(r"^[A-Z_][A-Z0-9_]*$")
                .unwrap()
                .is_match(name)
    }

    fn is_automatic_variable(&self, name: &str) -> bool {
        let auto_vars = [
            "PSVersionTable",
            "PWD",
            "LASTEXITCODE",
            "Error",
            "Host",
            "Profile",
            "PSScriptRoot",
            "PSCommandPath",
            "MyInvocation",
            "Args",
            "Input",
        ];
        auto_vars.contains(&name)
    }

    fn get_variable_documentation(
        &self,
        is_environment: bool,
        is_automatic: bool,
        is_global: bool,
        is_script: bool,
    ) -> String {
        let mut annotations = Vec::new();

        if is_environment {
            annotations.push("Environment Variable");
        }
        if is_automatic {
            annotations.push("Automatic Variable");
        }
        if is_global {
            annotations.push("Global Scope");
        }
        if is_script {
            annotations.push("Script Scope");
        }

        if !annotations.is_empty() {
            format!("[{}]", annotations.join(", "))
        } else {
            String::new()
        }
    }

    fn get_command_documentation(&self, command_name: &str) -> String {
        let command_docs = [
            ("Connect-AzAccount", "[Azure CLI Call]"),
            ("Set-AzContext", "[Azure Context Management]"),
            ("New-AzResourceGroup", "[Azure Resource Management]"),
            ("New-AzResourceGroupDeployment", "[Azure Deployment]"),
            ("docker", "[Docker Container Call]"),
            ("kubectl", "[Kubernetes CLI Call]"),
            ("az", "[Azure CLI Call]"),
            ("Import-Module", "[PowerShell Module Import]"),
            ("Export-ModuleMember", "[PowerShell Module Export]"),
            ("Invoke-Command", "[PowerShell Remoting]"),
        ];

        // Check direct match first
        for (cmd, doc) in &command_docs {
            if command_name == *cmd {
                return doc.to_string();
            }
        }

        // Pattern matching for commands
        if command_name.starts_with("Connect-Az") {
            return "[Azure CLI Call]".to_string();
        }
        if command_name.starts_with("New-Az") {
            return "[Azure Resource Creation]".to_string();
        }
        if command_name.starts_with("Set-Az") {
            return "[Azure Configuration]".to_string();
        }
        if command_name.starts_with("Get-Az") {
            return "[Azure Information Retrieval]".to_string();
        }
        if command_name.contains("WindowsFeature") {
            return "[Windows Feature Management]".to_string();
        }
        if command_name.contains("Service") {
            return "[Windows Service Management]".to_string();
        }

        "[PowerShell Command]".to_string()
    }

    fn extract_function_name_from_param_block(&self, node: Node) -> Option<String> {
        // For param_block nodes inside advanced functions, we need to look up the tree
        // to find the ERROR node that contains the function declaration

        // First, try to find ERROR node at program level (parent's parent's parent typically)
        let mut current = Some(node);
        while let Some(n) = current {
            if n.kind() == "program" {
                break;
            }
            current = n.parent();
        }

        if let Some(program_node) = current {
            // Look for ERROR node in program children
            let mut cursor = program_node.walk();
            for child in program_node.children(&mut cursor) {
                if child.kind() == "ERROR" {
                    let text = self.base.get_node_text(&child);
                    // Extract function name from text like "\nfunction Set-CustomProperty {"
                    if let Some(captures) = FUNCTION_NAME_RE.captures(&text) {
                        return Some(captures.get(1).unwrap().as_str().to_string());
                    }
                }
            }
        }

        // Fallback: look in parent nodes for any ERROR containing function
        let mut current = node.parent();
        while let Some(n) = current {
            if n.kind() == "ERROR" {
                let text = self.base.get_node_text(&n);
                if let Some(captures) = FUNCTION_NAME_RE.captures(&text) {
                    return Some(captures.get(1).unwrap().as_str().to_string());
                }
            }
            current = n.parent();
        }

        None
    }

    fn extract_advanced_function_signature(&self, node: Node, function_name: &str) -> String {
        let has_cmdlet_binding = self.has_attribute(node, "CmdletBinding");
        let has_output_type = self.has_attribute(node, "OutputType");

        let mut signature = String::new();
        if has_cmdlet_binding {
            signature.push_str("[CmdletBinding()] ");
        }
        if has_output_type {
            signature.push_str("[OutputType([void])] ");
        }
        signature.push_str(&format!("function {}()", function_name));

        signature
    }

    #[allow(clippy::only_used_in_recursion)] // &self used in recursive calls
    fn find_nodes_by_type<'a>(&self, node: Node<'a>, node_type: &str) -> Vec<Node<'a>> {
        let mut result = Vec::new();
        let mut cursor = node.walk();

        // Check direct children first
        for child in node.children(&mut cursor) {
            if child.kind() == node_type {
                result.push(child);
            }
            // Recursively search in children
            result.extend(self.find_nodes_by_type(child, node_type));
        }

        result
    }

    // ========================================================================
    // Identifier Extraction (for LSP-quality find_references)
    // ========================================================================

    /// Extract all identifier usages (function calls, member access, etc.)
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
        node: Node,
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
        node: Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) {
        match node.kind() {
            // PowerShell commands and cmdlet calls: Get-Process, Write-Host, etc.
            "command" | "command_expression" => {
                // Extract command name
                if let Some(name_node) = self.find_command_name_node(node) {
                    let name = self.base.get_node_text(&name_node);
                    let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                    self.base.create_identifier(
                        &name_node,
                        name,
                        IdentifierKind::Call,
                        containing_symbol_id,
                    );
                }
            }

            // PowerShell invocation expressions: function calls
            "invocation_expression" => {
                // Extract function name from invocation
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "command_name" || child.kind() == "identifier" {
                        let name = self.base.get_node_text(&child);
                        let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                        self.base.create_identifier(
                            &child,
                            name,
                            IdentifierKind::Call,
                            containing_symbol_id,
                        );
                        break;
                    } else if child.kind() == "member_access_expression" {
                        // For member access in invocation (e.g., $obj.Method())
                        // Extract the rightmost identifier (the method name)
                        let text = self.base.get_node_text(&child);
                        if let Some(last_dot_pos) = text.rfind('.') {
                            if last_dot_pos + 1 < text.len() {
                                let method_name = &text[last_dot_pos + 1..];
                                let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                                self.base.create_identifier(
                                    &child,
                                    method_name.to_string(),
                                    IdentifierKind::Call,
                                    containing_symbol_id,
                                );
                            }
                        }
                        break;
                    }
                }
            }

            // PowerShell member access: $object.Property, $this.Name
            // PowerShell tree-sitter uses "member_access" (not "member_access_expression")
            "member_access" => {
                // Only extract if it's NOT part of an invocation_expression or command
                // (we handle method calls separately)
                if let Some(parent) = node.parent() {
                    if parent.kind() == "invocation_expression" || parent.kind() == "command" {
                        return; // Skip - handled by invocation/command
                    }
                }

                // Extract member name from member_access node
                // Structure: member_access -> member_name -> simple_name
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "member_name" {
                        // Get the simple_name child
                        let mut name_cursor = child.walk();
                        for name_child in child.children(&mut name_cursor) {
                            if name_child.kind() == "simple_name" {
                                let member_name = self.base.get_node_text(&name_child);
                                let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                                self.base.create_identifier(
                                    &name_child,
                                    member_name,
                                    IdentifierKind::MemberAccess,
                                    containing_symbol_id,
                                );
                                return;
                            }
                        }
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
    /// POWERSHELL-SPECIFIC: Skip command symbols to avoid matching command calls with themselves
    fn find_containing_symbol_id(
        &self,
        node: Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) -> Option<String> {
        // CRITICAL FIX: Only search symbols from THIS FILE, not all files
        // Bug was: searching all symbols in DB caused wrong file symbols to match
        let file_symbols: Vec<Symbol> = symbol_map
            .values()
            .filter(|s| {
                s.file_path == self.base.file_path
                    // PowerShell-specific: Skip command symbols (they're calls, not containers)
                    // Only consider functions, methods, and classes as potential containers
                    && matches!(s.kind, SymbolKind::Function | SymbolKind::Method | SymbolKind::Class)
                    // PowerShell-specific: Skip single-line symbols (they're likely command calls)
                    // A true containing symbol must have a range (start_line < end_line)
                    && s.start_line < s.end_line
            })
            .map(|&s| s.clone())
            .collect();

        self.base
            .find_containing_symbol(&node, &file_symbols)
            .map(|s| s.id.clone())
    }
}
