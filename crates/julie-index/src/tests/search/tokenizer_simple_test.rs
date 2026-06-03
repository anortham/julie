/// Tests for `SimpleCodeTokenizer` — lowercase + ascii-fold + length cap only.
///
/// No CamelCase splitting, no snake_case splitting, no stemming.
/// Registered under name `"simple_code"`.
use tantivy::tokenizer::{TextAnalyzer, TokenStream};

use crate::search::tokenizer::SimpleCodeTokenizer;

fn collect_tokens(mut analyzer: TextAnalyzer, text: &str) -> Vec<String> {
    let mut stream = analyzer.token_stream(text);
    let mut tokens = Vec::new();
    while stream.advance() {
        tokens.push(stream.token().text.clone());
    }
    tokens
}

fn make_analyzer() -> TextAnalyzer {
    TextAnalyzer::builder(SimpleCodeTokenizer::new()).build()
}

/// Core acceptance criterion: `"getUserData_v2"` → exactly one token, lowercased.
/// No camel splits, no snake splits, no stem variants.
#[test]
fn no_camel_or_stem() {
    let tokens = collect_tokens(make_analyzer(), "getUserData_v2");
    assert_eq!(
        tokens,
        vec!["getuserdata_v2"],
        "SimpleCodeTokenizer must emit exactly one lowercased token with no splits"
    );
}

/// Uppercase ASCII folds to lowercase.
#[test]
fn lowercases_and_folds() {
    let tokens = collect_tokens(make_analyzer(), "HTTPResponse");
    assert_eq!(tokens, vec!["httpresponse"]);

    let tokens = collect_tokens(make_analyzer(), "HELLO_WORLD");
    assert_eq!(tokens, vec!["hello_world"]);
}

/// Tokens longer than 80 characters are truncated to 80.
#[test]
fn respects_length_cap() {
    // 100-char word
    let long_word = "a".repeat(100);
    let tokens = collect_tokens(make_analyzer(), &long_word);
    assert_eq!(tokens.len(), 1, "should produce exactly one token");
    assert_eq!(
        tokens[0].len(),
        80,
        "token must be truncated to 80 chars (max_token_length)"
    );
}

/// Multiple whitespace-separated tokens are each emitted as one lowercased token.
#[test]
fn multiple_words_no_splitting() {
    let tokens = collect_tokens(make_analyzer(), "getUserData findByName");
    assert_eq!(tokens, vec!["getuserdata", "findbyname"]);
}

/// Empty string produces no tokens.
#[test]
fn empty_input() {
    let tokens = collect_tokens(make_analyzer(), "");
    assert!(tokens.is_empty());
}
