use tree_sitter::{Parser, Tree};

/// Initialize parser for the specified language
pub fn init_parser(code: &str, language: &str) -> Tree {
    let mut parser = Parser::new();

    // Use the language module to get tree-sitter language (re-exported from julie-extractors)
    let lang = crate::language::get_tree_sitter_language(language)
        .unwrap_or_else(|_| panic!("Error loading {} grammar", language));

    parser
        .set_language(&lang)
        .unwrap_or_else(|_| panic!("Error setting {} language", language));

    parser.parse(code, None).expect("Failed to parse code")
}
