//! Language Support - Shared tree-sitter language configuration
//!
//! This module provides the public language metadata API.

pub use crate::language_spec::{
    DATA_ONLY_CAPABILITIES, DocCommentStyle, FULL_CAPABILITIES, LanguageCapabilities, LanguageSpec,
    NO_PENDING_CAPABILITIES, NO_RELATIONSHIP_CAPABILITIES, PENDING_NO_TYPES_CAPABILITIES,
    detect_language_for_source, detect_language_from_extension, get_tree_sitter_language,
    language_spec, language_specs, supported_extensions, supported_languages,
};

/// Get AST node types that represent function definitions for a given language
///
/// Used by refactoring tools to identify functions in AST for operations like
/// extract function, find insertion points, etc.
pub fn get_function_node_kinds(language: &str) -> Vec<&'static str> {
    match language {
        "rust" => vec!["function_item", "impl_item"],
        "typescript" | "tsx" | "javascript" | "jsx" => {
            vec![
                "function_declaration",
                "method_definition",
                "arrow_function",
            ]
        }
        "python" => vec!["function_definition"],
        "java" => vec!["method_declaration"],
        "cpp" | "c" => vec!["function_definition"],
        "go" => vec!["function_declaration", "method_declaration"],
        "csharp" => vec!["method_declaration"],
        "vbnet" => vec!["method_declaration", "abstract_method_declaration"],
        "php" => vec!["function_definition", "method_declaration"],
        "ruby" => vec!["method", "singleton_method"],
        "swift" => vec!["function_declaration"],
        "kotlin" => vec!["function_declaration"],
        "scala" => vec!["function_definition", "function_declaration"],
        "elixir" => vec!["call"],
        "dart" => vec!["function_signature", "method_signature"],
        "lua" => vec!["function_declaration"],
        "bash" => vec!["function_definition"],
        "powershell" => vec!["function_statement"],
        _ => vec!["function"], // Generic fallback
    }
}

/// Get AST node types that represent import/use statements for a given language
///
/// Used by refactoring tools to find where to insert new code after imports.
pub fn get_import_node_kinds(language: &str) -> Vec<&'static str> {
    match language {
        "rust" => vec!["use_declaration"],
        "typescript" | "tsx" | "javascript" | "jsx" => vec!["import_statement"],
        "python" => vec!["import_statement", "import_from_statement"],
        "java" => vec!["import_declaration"],
        "go" => vec!["import_declaration"],
        "csharp" => vec!["using_directive"],
        "vbnet" => vec!["imports_statement"],
        "php" => vec!["namespace_use_declaration"],
        "ruby" => vec!["call"], // require/require_relative are function calls
        "swift" => vec!["import_declaration"],
        "kotlin" => vec!["import_header"],
        "scala" => vec!["import_declaration"],
        "elixir" => vec!["call"],
        "dart" => vec!["import_or_export"],
        "cpp" | "c" => vec!["preproc_include"],
        _ => vec!["import"], // Generic fallback
    }
}

/// Get AST node types that represent symbol definitions (functions, classes, structs, etc.)
///
/// Used by refactoring tools to locate and manipulate symbol definitions for operations
/// like rename symbol, find symbol boundaries, etc.
pub fn get_symbol_node_kinds(language: &str) -> Vec<&'static str> {
    match language {
        "rust" => vec![
            "function_item",
            "struct_item",
            "enum_item",
            "impl_item",
            "trait_item",
            "type_item",
        ],
        "typescript" | "tsx" | "javascript" | "jsx" => vec![
            "function_declaration",
            "class_declaration",
            "method_definition",
            "interface_declaration",
            "type_alias_declaration",
        ],
        "python" => vec!["function_definition", "class_definition"],
        "java" => vec![
            "method_declaration",
            "class_declaration",
            "interface_declaration",
            "enum_declaration",
        ],
        "cpp" | "c" => vec![
            "function_definition",
            "class_specifier",
            "struct_specifier",
            "enum_specifier",
        ],
        "go" => vec![
            "function_declaration",
            "method_declaration",
            "type_declaration",
        ],
        "csharp" => vec![
            "method_declaration",
            "class_declaration",
            "interface_declaration",
            "struct_declaration",
            "enum_declaration",
        ],
        "vbnet" => vec![
            "method_declaration",
            "abstract_method_declaration",
            "class_block",
            "module_block",
            "structure_block",
            "interface_block",
            "enum_block",
        ],
        "php" => vec![
            "function_definition",
            "method_declaration",
            "class_declaration",
            "interface_declaration",
            "trait_declaration",
        ],
        "ruby" => vec!["method", "singleton_method", "class", "module"],
        "swift" => vec![
            "function_declaration",
            "class_declaration",
            "struct_declaration",
            "protocol_declaration",
            "enum_declaration",
        ],
        "kotlin" => vec![
            "function_declaration",
            "class_declaration",
            "object_declaration",
            "interface_declaration",
        ],
        "scala" => vec![
            "function_definition",
            "class_definition",
            "trait_definition",
            "object_definition",
            "enum_definition",
            "type_definition",
        ],
        "elixir" => vec!["call"],
        "dart" => vec!["function_signature", "method_signature", "class_definition"],
        "lua" => vec!["function_declaration", "local_function"],
        _ => vec!["function", "class", "method"], // Generic fallback
    }
}

/// Get the field name used to extract symbol names from AST nodes
///
/// Different languages use different field names in their AST to store the symbol name.
/// Most use "name", but some (like C/C++) use more complex nested structures.
pub fn get_symbol_name_field(language: &str) -> &'static str {
    match language {
        "rust" | "typescript" | "tsx" | "javascript" | "jsx" | "python" | "java" | "go"
        | "csharp" | "vbnet" | "php" | "ruby" | "swift" | "kotlin" | "scala" | "elixir"
        | "dart" | "lua" | "bash" | "powershell" => "name",
        "cpp" | "c" => "declarator", // C/C++ use nested declarator nodes
        _ => "name",                 // Generic fallback
    }
}
