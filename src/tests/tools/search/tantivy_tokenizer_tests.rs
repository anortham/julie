//! Tests for the code-aware tokenizer.

use crate::search::tokenizer::{split_camel_case, split_snake_case, CodeTokenizer};
use tantivy::tokenizer::{TextAnalyzer, TokenStream};

#[test]
fn test_camel_case_split() {
    assert_eq!(split_camel_case("UserService"), vec!["User", "Service"]);
}

#[test]
fn test_camel_case_acronym() {
    assert_eq!(split_camel_case("XMLParser"), vec!["XML", "Parser"]);
}

#[test]
fn test_camel_case_mixed() {
    let result = split_camel_case("getHTTPResponse");
    assert_eq!(result, vec!["get", "HTTP", "Response"]);
}

#[test]
fn test_snake_case_split() {
    assert_eq!(split_snake_case("user_service"), vec!["user", "service"]);
}

#[test]
fn test_snake_case_screaming() {
    assert_eq!(
        split_snake_case("MAX_BUFFER_SIZE"),
        vec!["MAX", "BUFFER", "SIZE"]
    );
}

#[test]
fn test_tokenizer_camel_case_produces_all_variants() {
    let tokenizer = CodeTokenizer::new(vec![]);
    let mut analyzer = TextAnalyzer::builder(tokenizer).build();
    let mut stream = analyzer.token_stream("UserService");
    let mut tokens = Vec::new();
    while let Some(token) = stream.next() {
        tokens.push(token.text.clone());
    }
    assert!(
        tokens.contains(&"userservice".to_string()),
        "Missing original: {:?}",
        tokens
    );
    assert!(
        tokens.contains(&"user".to_string()),
        "Missing 'user': {:?}",
        tokens
    );
    assert!(
        tokens.contains(&"service".to_string()),
        "Missing 'service': {:?}",
        tokens
    );
}

#[test]
fn test_tokenizer_preserves_rust_patterns() {
    let tokenizer = CodeTokenizer::new(vec!["::".to_string(), "->".to_string()]);
    let mut analyzer = TextAnalyzer::builder(tokenizer).build();
    let mut stream = analyzer.token_stream("std::io::Read");
    let mut tokens = Vec::new();
    while let Some(token) = stream.next() {
        tokens.push(token.text.clone());
    }
    assert!(tokens.contains(&"std".to_string()));
    assert!(tokens.contains(&"::".to_string()));
    assert!(tokens.contains(&"io".to_string()));
    assert!(tokens.contains(&"read".to_string()));
}

#[test]
fn test_tokenizer_preserves_typescript_patterns() {
    let tokenizer = CodeTokenizer::new(vec!["?.".to_string(), "??".to_string()]);
    let mut analyzer = TextAnalyzer::builder(tokenizer).build();
    let mut stream = analyzer.token_stream("user?.profile ?? default");
    let mut tokens = Vec::new();
    while let Some(token) = stream.next() {
        tokens.push(token.text.clone());
    }
    assert!(tokens.contains(&"user".to_string()));
    assert!(tokens.contains(&"?.".to_string()));
    assert!(tokens.contains(&"profile".to_string()));
    assert!(tokens.contains(&"??".to_string()));
}

#[test]
fn test_tokenizer_snake_case_produces_parts() {
    let tokenizer = CodeTokenizer::new(vec![]);
    let mut analyzer = TextAnalyzer::builder(tokenizer).build();
    let mut stream = analyzer.token_stream("get_user_data");
    let mut tokens = Vec::new();
    while let Some(token) = stream.next() {
        tokens.push(token.text.clone());
    }
    assert!(
        tokens.contains(&"get_user_data".to_string()),
        "Missing original: {:?}",
        tokens
    );
    assert!(
        tokens.contains(&"get".to_string()),
        "Missing 'get': {:?}",
        tokens
    );
    assert!(
        tokens.contains(&"user".to_string()),
        "Missing 'user': {:?}",
        tokens
    );
    assert!(
        tokens.contains(&"data".to_string()),
        "Missing 'data': {:?}",
        tokens
    );
}

#[test]
fn test_tokenizer_from_language_configs() {
    use crate::search::language_config::LanguageConfigs;
    let configs = LanguageConfigs::load_embedded();
    let tokenizer = CodeTokenizer::from_language_configs(&configs);
    let mut analyzer = TextAnalyzer::builder(tokenizer).build();
    let mut stream = analyzer.token_stream("std::io::Result");
    let mut tokens = Vec::new();
    while let Some(token) = stream.next() {
        tokens.push(token.text.clone());
    }
    assert!(
        tokens.contains(&"::".to_string()),
        "Should preserve :: from configs: {:?}",
        tokens
    );
}

#[test]
fn test_tokenizer_splits_hyphenated_terms() {
    // "tree-sitter" should be split on hyphen, producing "tree" and "sitter"
    let tokenizer = CodeTokenizer::new(vec![]);
    let mut analyzer = TextAnalyzer::builder(tokenizer).build();
    let mut stream = analyzer.token_stream("tree-sitter parse");
    let mut tokens = Vec::new();
    while let Some(token) = stream.next() {
        tokens.push(token.text.clone());
    }
    assert!(
        tokens.contains(&"tree".to_string()),
        "Hyphenated 'tree-sitter' should produce 'tree': {:?}",
        tokens
    );
    assert!(
        tokens.contains(&"sitter".to_string()),
        "Hyphenated 'tree-sitter' should produce 'sitter': {:?}",
        tokens
    );
    assert!(
        tokens.contains(&"parse".to_string()),
        "Should also produce 'parse': {:?}",
        tokens
    );
}

#[test]
fn test_tokenizer_handles_dotted_identifiers() {
    // "std.io.Read" should split on dots, producing "std", "io", "read"
    let tokenizer = CodeTokenizer::new(vec![]);
    let mut analyzer = TextAnalyzer::builder(tokenizer).build();
    let mut stream = analyzer.token_stream("CASCADE.architecture");
    let mut tokens = Vec::new();
    while let Some(token) = stream.next() {
        tokens.push(token.text.clone());
    }
    assert!(
        tokens.contains(&"cascade".to_string()),
        "Dotted 'CASCADE.architecture' should produce 'cascade': {:?}",
        tokens
    );
    assert!(
        tokens.contains(&"architecture".to_string()),
        "Dotted 'CASCADE.architecture' should produce 'architecture': {:?}",
        tokens
    );
}
