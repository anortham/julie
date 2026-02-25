//! Tests for English stemming in the CodeTokenizer.
//!
//! Stemming emits morphological variants as ADDITIONAL tokens so that
//! "estimation" and "estimator" both produce the stem "estim", enabling
//! cross-variant matching while preserving exact tokens.

use crate::search::tokenizer::CodeTokenizer;
use tantivy::tokenizer::{TextAnalyzer, TokenStream};

fn get_tokens(text: &str) -> Vec<String> {
    let tokenizer = CodeTokenizer::new(vec![]);
    let mut analyzer = TextAnalyzer::builder(tokenizer).build();
    let mut stream = analyzer.token_stream(text);
    let mut tokens = Vec::new();
    while let Some(token) = stream.next() {
        tokens.push(token.text.clone());
    }
    tokens
}

#[test]
fn test_stemming_estimation_and_estimator_share_stem() {
    let tokens_a = get_tokens("estimation");
    let tokens_b = get_tokens("estimator");

    // Both should produce the stem "estim"
    assert!(
        tokens_a.contains(&"estim".to_string()),
        "\"estimation\" should stem to \"estim\", got: {:?}",
        tokens_a
    );
    assert!(
        tokens_b.contains(&"estim".to_string()),
        "\"estimator\" should stem to \"estim\", got: {:?}",
        tokens_b
    );
}

#[test]
fn test_stemming_connected_and_connecting_share_stem() {
    let tokens_a = get_tokens("connected");
    let tokens_b = get_tokens("connecting");

    // Both should produce the stem "connect"
    assert!(
        tokens_a.contains(&"connect".to_string()),
        "\"connected\" should stem to \"connect\", got: {:?}",
        tokens_a
    );
    assert!(
        tokens_b.contains(&"connect".to_string()),
        "\"connecting\" should stem to \"connect\", got: {:?}",
        tokens_b
    );
}

#[test]
fn test_stemming_preserves_exact_tokens() {
    let tokens = get_tokens("estimation");

    // Must emit BOTH the exact lowercased token AND the stem
    assert!(
        tokens.contains(&"estimation".to_string()),
        "Exact token \"estimation\" must be preserved, got: {:?}",
        tokens
    );
    assert!(
        tokens.contains(&"estim".to_string()),
        "Stem \"estim\" must also be emitted, got: {:?}",
        tokens
    );
}

#[test]
fn test_stemming_short_tokens_not_stemmed() {
    // Tokens shorter than 4 chars should NOT be stemmed (avoids noise from "fn", "if", "ok")
    let tokens_fn = get_tokens("fn");
    let tokens_if = get_tokens("if");
    let tokens_ok = get_tokens("ok");

    // Each should only produce the lowercased original, no stem variant
    assert_eq!(tokens_fn, vec!["fn"], "Short token 'fn' should not be stemmed");
    assert_eq!(tokens_if, vec!["if"], "Short token 'if' should not be stemmed");
    assert_eq!(tokens_ok, vec!["ok"], "Short token 'ok' should not be stemmed");
}

#[test]
fn test_stemming_no_duplicate_when_stem_equals_original() {
    // "test" stems to "test" — should NOT emit a duplicate
    let tokens = get_tokens("test");
    let count = tokens.iter().filter(|t| *t == "test").count();
    assert_eq!(
        count, 1,
        "\"test\" stems to itself; should appear exactly once, got: {:?}",
        tokens
    );
}

#[test]
fn test_stemming_works_with_camel_case_splits() {
    // "TokenEstimator" → splits into "token" + "estimator" → "estimator" stems to "estim"
    let tokens = get_tokens("TokenEstimator");

    assert!(
        tokens.contains(&"tokenestimator".to_string()),
        "Original lowered should be present: {:?}",
        tokens
    );
    assert!(
        tokens.contains(&"token".to_string()),
        "CamelCase split 'token' should be present: {:?}",
        tokens
    );
    assert!(
        tokens.contains(&"estimator".to_string()),
        "CamelCase split 'estimator' should be present: {:?}",
        tokens
    );
    assert!(
        tokens.contains(&"estim".to_string()),
        "Stem 'estim' from 'estimator' should be present: {:?}",
        tokens
    );
}

#[test]
fn test_stemming_works_with_snake_case_splits() {
    // "token_estimation" → splits into "token" + "estimation" → "estimation" stems to "estim"
    let tokens = get_tokens("token_estimation");

    assert!(
        tokens.contains(&"token_estimation".to_string()),
        "Original lowered should be present: {:?}",
        tokens
    );
    assert!(
        tokens.contains(&"token".to_string()),
        "Snake split 'token' should be present: {:?}",
        tokens
    );
    assert!(
        tokens.contains(&"estimation".to_string()),
        "Snake split 'estimation' should be present: {:?}",
        tokens
    );
    assert!(
        tokens.contains(&"estim".to_string()),
        "Stem 'estim' from 'estimation' should be present: {:?}",
        tokens
    );
}
