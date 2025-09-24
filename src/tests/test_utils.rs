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
        _ => panic!("Unsupported language: {}", language),
    }

    parser.parse(code, None).expect("Failed to parse code")
}