use tree_sitter::{Parser, Tree};

/// Initialize parser for the specified language
pub fn init_parser(code: &str, language: &str) -> Tree {
    let mut parser = Parser::new();

    match language {
        "go" => {
            parser.set_language(&tree_sitter_go::LANGUAGE.into())
                .expect("Error loading Go grammar");
        }
        "typescript" | "javascript" => {
            parser.set_language(&tree_sitter_javascript::LANGUAGE.into())
                .expect("Error loading JavaScript grammar");
        }
        "python" => {
            parser.set_language(&tree_sitter_python::LANGUAGE.into())
                .expect("Error loading Python grammar");
        }
        "rust" => {
            parser.set_language(&tree_sitter_rust::LANGUAGE.into())
                .expect("Error loading Rust grammar");
        }
        "swift" => {
            parser.set_language(&tree_sitter_swift::LANGUAGE.into())
                .expect("Error loading Swift grammar");
        }
        "css" => {
            parser.set_language(&tree_sitter_css::LANGUAGE.into())
                .expect("Error loading CSS grammar");
        }
        "razor" => {
            parser.set_language(&tree_sitter_razor::LANGUAGE.into())
                .expect("Error loading Razor grammar");
        }
        "powershell" => {
            parser.set_language(&tree_sitter_powershell::LANGUAGE.into())
                .expect("Error loading PowerShell grammar");
        }
        "regex" => {
            parser.set_language(&tree_sitter_regex::LANGUAGE.into())
                .expect("Error loading Regex grammar");
        }
        "html" => {
            parser.set_language(&tree_sitter_html::LANGUAGE.into())
                .expect("Error loading HTML grammar");
        }
        "zig" => {
            parser.set_language(&tree_sitter_zig::LANGUAGE.into())
                .expect("Error loading Zig grammar");
        }
        "c" => {
            parser.set_language(&tree_sitter_c::LANGUAGE.into())
                .expect("Error loading C grammar");
        }
        "cpp" => {
            parser.set_language(&tree_sitter_cpp::LANGUAGE.into())
                .expect("Error loading C++ grammar");
        }
        "csharp" => {
            parser.set_language(&tree_sitter_c_sharp::LANGUAGE.into())
                .expect("Error loading C# grammar");
        }
        "kotlin" => {
            parser.set_language(&tree_sitter_kotlin_ng::LANGUAGE.into())
                .expect("Error loading Kotlin grammar");
        }
        "dart" => {
            parser.set_language(&harper_tree_sitter_dart::LANGUAGE.into())
                .expect("Error loading Dart grammar");
        }
        "java" => {
            parser.set_language(&tree_sitter_java::LANGUAGE.into())
                .expect("Error loading Java grammar");
        }
        "ruby" => {
            parser.set_language(&tree_sitter_ruby::LANGUAGE.into())
                .expect("Error loading Ruby grammar");
        }
        "php" => {
            // TODO: Fix tree-sitter-php integration - crate doesn't expose expected API
            panic!("PHP parser not yet integrated - need to investigate tree-sitter-php crate API");
        }
        "bash" => {
            parser.set_language(&tree_sitter_bash::LANGUAGE.into())
                .expect("Error loading Bash grammar");
        }
        "lua" => {
            parser.set_language(&tree_sitter_lua::LANGUAGE.into())
                .expect("Error loading Lua grammar");
        }
        "gdscript" => {
            parser.set_language(&tree_sitter_gdscript::LANGUAGE.into())
                .expect("Error loading GDScript grammar");
        }
        "sql" => {
            parser.set_language(&tree_sitter_sql::LANGUAGE.into())
                .expect("Error loading SQL grammar");
        }
        "vue" => {
            // Vue SFCs are parsed as JavaScript for now
            parser.set_language(&tree_sitter_javascript::LANGUAGE.into())
                .expect("Error loading JavaScript grammar for Vue");
        }
        _ => panic!("Unsupported language: {}", language),
    }

    parser.parse(code, None).expect("Failed to parse code")
}