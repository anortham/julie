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
        _ => panic!("Unsupported language: {}", language),
    }

    parser.parse(code, None).expect("Failed to parse code")
}