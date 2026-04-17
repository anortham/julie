//! Language Support - Shared tree-sitter language configuration
//!
//! This module provides centralized language support for Julie's 31 supported languages.
//! ALL language-specific tree-sitter configuration should go here to avoid duplication.

use anyhow::Result;

/// Get tree-sitter language parser for a given language name
///
/// This is the SINGLE SOURCE OF TRUTH for language support in Julie.
/// Used by both ExtractorManager (for symbol extraction) and SmartRefactorTool
/// (for AST-aware refactoring).
///
/// # Supported Languages (31 total)
///
/// **Systems**: Rust, C, C++, Go, Zig
/// **Web**: TypeScript, JavaScript, HTML, CSS, Vue, QML
/// **Backend**: Python, Java, C#, PHP, Ruby, Swift, Kotlin, Dart
/// **Scripting**: Lua, R, Bash, PowerShell
/// **Specialized**: GDScript, Razor, SQL, Regex
/// **Documentation**: Markdown, JSON, TOML, YAML
pub fn get_tree_sitter_language(language: &str) -> Result<tree_sitter::Language> {
    match language {
        // Systems languages
        "rust" => Ok(tree_sitter_rust::LANGUAGE.into()),
        "c" => Ok(tree_sitter_c::LANGUAGE.into()),
        "cpp" => Ok(tree_sitter_cpp::LANGUAGE.into()),
        "go" => Ok(tree_sitter_go::LANGUAGE.into()),
        "zig" => Ok(tree_sitter_zig::LANGUAGE.into()),

        // Web languages
        "typescript" => Ok(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        "tsx" => Ok(tree_sitter_typescript::LANGUAGE_TSX.into()),
        "javascript" | "jsx" => Ok(tree_sitter_javascript::LANGUAGE.into()),
        "html" => Ok(tree_sitter_html::LANGUAGE.into()),
        "css" => Ok(tree_sitter_css::LANGUAGE.into()),
        "vue" => Ok(tree_sitter_html::LANGUAGE.into()), // Vue SFCs use HTML structure

        // Backend languages
        "python" => Ok(tree_sitter_python::LANGUAGE.into()),
        "java" => Ok(tree_sitter_java::LANGUAGE.into()),
        "csharp" => Ok(tree_sitter_c_sharp::LANGUAGE.into()),
        "vbnet" => Ok(tree_sitter_vb_dotnet::LANGUAGE.into()),
        "php" => Ok(tree_sitter_php::LANGUAGE_PHP.into()),
        "ruby" => Ok(tree_sitter_ruby::LANGUAGE.into()),
        "swift" => Ok(tree_sitter_swift::LANGUAGE.into()),
        "kotlin" => Ok(tree_sitter_kotlin_ng::LANGUAGE.into()),
        "scala" => Ok(tree_sitter_scala::LANGUAGE.into()),
        "dart" => Ok(harper_tree_sitter_dart::LANGUAGE.into()),

        // Functional languages
        "elixir" => Ok(tree_sitter_elixir::LANGUAGE.into()),

        // Scripting languages
        "lua" => Ok(tree_sitter_lua::LANGUAGE.into()),
        "qml" => Ok(tree_sitter_qmljs::LANGUAGE.into()),
        "r" => Ok(tree_sitter_r::LANGUAGE.into()),
        "bash" => Ok(tree_sitter_bash::LANGUAGE.into()),
        "powershell" => Ok(tree_sitter_powershell::LANGUAGE.into()),

        // Specialized languages
        "gdscript" => Ok(tree_sitter_gdscript::LANGUAGE.into()),
        "razor" => Ok(tree_sitter_razor::LANGUAGE.into()),
        "sql" => Ok(tree_sitter_sequel::LANGUAGE.into()),
        "regex" => Ok(tree_sitter_regex::LANGUAGE.into()),

        // Documentation and configuration languages
        "markdown" => Ok(tree_sitter_md::LANGUAGE.into()),
        "json" => Ok(tree_sitter_json::LANGUAGE.into()),
        "toml" => Ok(tree_sitter_toml_ng::LANGUAGE.into()),
        "yaml" => Ok(tree_sitter_yaml::LANGUAGE.into()),

        _ => Err(anyhow::anyhow!(
            "Unsupported language: '{}'. Supported languages: rust, c, cpp, go, zig, typescript, javascript, html, css, vue, qml, r, python, java, csharp, php, ruby, swift, kotlin, scala, elixir, dart, lua, bash, powershell, gdscript, razor, sql, regex, markdown, json, toml, yaml",
            language
        )),
    }
}

/// Detect language from file extension.
///
/// This is the **SINGLE SOURCE OF TRUTH** for extension → language mapping.
/// All other language detection in the codebase MUST delegate to this function.
/// Returns the language name that can be passed to `get_tree_sitter_language()`.
pub fn detect_language_from_extension(extension: &str) -> Option<&'static str> {
    match extension {
        // Systems
        "rs" => Some("rust"),
        "c" | "h" => Some("c"),
        "cpp" | "cc" | "cxx" | "c++" | "hpp" | "hh" | "hxx" | "h++" => Some("cpp"),
        "go" => Some("go"),
        "zig" => Some("zig"),

        // Web
        "ts" | "mts" | "cts" => Some("typescript"),
        "tsx" => Some("tsx"),
        "js" | "jsx" | "mjs" | "cjs" => Some("javascript"),
        "html" | "htm" => Some("html"),
        "css" => Some("css"),
        "vue" => Some("vue"),

        // Backend
        "py" | "pyi" | "pyw" => Some("python"),
        "java" => Some("java"),
        "cs" => Some("csharp"),
        "vb" => Some("vbnet"),
        "php" | "phtml" => Some("php"),
        "rb" | "rbw" => Some("ruby"),
        "swift" => Some("swift"),
        "kt" | "kts" => Some("kotlin"),
        "scala" | "sc" => Some("scala"),
        "dart" => Some("dart"),

        // Functional
        "ex" | "exs" => Some("elixir"),

        // Scripting
        "lua" => Some("lua"),
        "qml" => Some("qml"),
        "r" | "R" => Some("r"),
        "sh" | "bash" => Some("bash"),
        "ps1" | "psm1" | "psd1" => Some("powershell"),

        // Specialized
        "gd" => Some("gdscript"),
        "razor" | "cshtml" => Some("razor"),
        "sql" => Some("sql"),
        "regex" => Some("regex"),

        // Documentation and config
        "md" | "markdown" => Some("markdown"),
        "json" | "jsonl" | "jsonc" => Some("json"),
        "toml" => Some("toml"),
        "yml" | "yaml" => Some("yaml"),

        _ => None,
    }
}

/// All file extensions that Julie can index.
///
/// **SINGLE SOURCE OF TRUTH** — `build_supported_extensions()` and `is_supported_file()`
/// must derive from this, not maintain their own lists.
pub fn supported_extensions() -> &'static [&'static str] {
    &[
        // Systems
        "rs", "c", "h", "cpp", "cc", "cxx", "c++", "hpp", "hh", "hxx", "h++", "go", "zig",
        // Web
        "ts", "mts", "cts", "tsx", "js", "jsx", "mjs", "cjs", "html", "htm", "css", "vue",
        // Backend
        "py", "pyi", "pyw", "java", "cs", "php", "phtml", "rb", "rbw", "swift", "kt", "kts",
        "scala", "sc", "dart", "vb", // Functional
        "ex", "exs", // Scripting
        "lua", "qml", "r", "R", "sh", "bash", "ps1", "psm1", "psd1", // Specialized
        "gd", "razor", "cshtml", "sql", "regex", // Documentation and config
        "md", "markdown", "json", "jsonl", "jsonc", "toml", "yml", "yaml",
    ]
}

/// All language names Julie supports (33 languages).
///
/// **SINGLE SOURCE OF TRUTH** — `ExtractorManager::supported_languages()` must
/// delegate to this, not maintain its own list.
pub fn supported_languages() -> &'static [&'static str] {
    &[
        // Systems
        "rust",
        "c",
        "cpp",
        "go",
        "zig",
        // Web
        "typescript",
        "tsx",
        "javascript",
        "html",
        "css",
        "vue",
        // Backend
        "python",
        "java",
        "csharp",
        "php",
        "ruby",
        "swift",
        "kotlin",
        "scala",
        "dart",
        "vbnet",
        // Functional
        "elixir",
        // Scripting
        "lua",
        "qml",
        "r",
        "bash",
        "powershell",
        // Specialized
        "gdscript",
        "razor",
        "sql",
        "regex",
        // Documentation and config
        "markdown",
        "json",
        "toml",
        "yaml",
    ]
}

/// Get AST node types that represent function definitions for a given language
///
/// Used by refactoring tools to identify functions in AST for operations like
/// extract function, find insertion points, etc.
pub fn get_function_node_kinds(language: &str) -> Vec<&'static str> {
    match language {
        "rust" => vec!["function_item", "impl_item"],
        "typescript" | "tsx" | "javascript" => {
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
        "typescript" | "tsx" | "javascript" => vec!["import_statement"],
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
        "typescript" | "tsx" | "javascript" => vec![
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
        "rust" | "typescript" | "tsx" | "javascript" | "python" | "java" | "go" | "csharp"
        | "vbnet" | "php" | "ruby" | "swift" | "kotlin" | "scala" | "elixir" | "dart" | "lua"
        | "bash" | "powershell" => "name",
        "cpp" | "c" => "declarator", // C/C++ use nested declarator nodes
        _ => "name",                 // Generic fallback
    }
}
