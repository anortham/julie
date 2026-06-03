//! Tests for strip_prefixes/suffixes variant generation in CodeTokenizer
//!
//! Verifies that the CodeTokenizer strips type-level prefixes (e.g., "I" for interfaces)
//! and suffixes (e.g., "Service", "Controller") to generate additional search tokens.

#[cfg(test)]
mod tests {
    use crate::search::tokenizer::CodeTokenizer;
    use tantivy::tokenizer::{TextAnalyzer, TokenStream};

    /// Helper: collect all token texts from a tokenizer run
    fn collect_tokens(tokenizer: CodeTokenizer, text: &str) -> Vec<String> {
        let mut analyzer = TextAnalyzer::builder(tokenizer).build();
        let mut stream = analyzer.token_stream(text);
        let mut tokens = Vec::new();
        while let Some(token) = stream.next() {
            tokens.push(token.text.clone());
        }
        tokens
    }

    #[test]
    fn test_strip_prefix_interface() {
        // "IUserService" with prefix "I" → should emit "userservice" (without I prefix)
        let mut tokenizer = CodeTokenizer::new(vec![]);
        tokenizer.set_strip_rules(vec!["I".into()], vec!["Service".into()]);

        let tokens = collect_tokens(tokenizer, "IUserService");

        assert!(
            tokens.contains(&"userservice".to_string()),
            "Should strip I prefix to emit 'userservice': {:?}",
            tokens
        );
    }

    #[test]
    fn test_strip_suffix_service() {
        // "IUserService" with suffix "Service" → should emit "iuser" (without Service suffix)
        let mut tokenizer = CodeTokenizer::new(vec![]);
        tokenizer.set_strip_rules(vec!["I".into()], vec!["Service".into()]);

        let tokens = collect_tokens(tokenizer, "IUserService");

        assert!(
            tokens.contains(&"iuser".to_string()),
            "Should strip Service suffix to emit 'iuser': {:?}",
            tokens
        );
    }

    #[test]
    fn test_strip_suffix_controller() {
        // "AuthController" with suffix "Controller" → should emit "auth"
        let mut tokenizer = CodeTokenizer::new(vec![]);
        tokenizer.set_strip_rules(vec![], vec!["Controller".into()]);

        let tokens = collect_tokens(tokenizer, "AuthController");

        assert!(
            tokens.contains(&"auth".to_string()),
            "Should strip Controller suffix to emit 'auth': {:?}",
            tokens
        );
    }

    #[test]
    fn test_strip_prefix_underscore() {
        // Python-style: "_internal_method" with prefix "_" → should emit "internal_method"
        let mut tokenizer = CodeTokenizer::new(vec![]);
        tokenizer.set_strip_rules(vec!["_".into()], vec![]);

        let tokens = collect_tokens(tokenizer, "_internal_method");

        assert!(
            tokens.contains(&"internal_method".to_string()),
            "Should strip _ prefix to emit 'internal_method': {:?}",
            tokens
        );
    }

    #[test]
    fn test_strip_no_false_positive_short_remainder() {
        // "IA" with prefix "I" → remainder "A" is too short, should not emit
        let mut tokenizer = CodeTokenizer::new(vec![]);
        tokenizer.set_strip_rules(vec!["I".into()], vec![]);

        let tokens = collect_tokens(tokenizer, "IA");

        // "a" is only 1 char — too short to be a useful stripped variant
        let has_just_a = tokens.iter().any(|t| t == "a");
        assert!(
            !has_just_a,
            "Should NOT emit single-char 'a' from stripping I prefix of 'IA': {:?}",
            tokens
        );
    }

    #[test]
    fn test_strip_from_language_configs() {
        // Verify that from_language_configs wires strip rules correctly
        use crate::search::language_config::LanguageConfigs;

        let configs = LanguageConfigs::load_embedded();
        let tokenizer = CodeTokenizer::from_language_configs(&configs);

        // C# config has strip_prefixes = ["I", "_"] and strip_suffixes include "Service"
        let tokens = collect_tokens(tokenizer, "IPaymentService");

        // Should strip "I" prefix → "paymentservice"
        assert!(
            tokens.contains(&"paymentservice".to_string()),
            "from_language_configs should wire strip_prefixes from TOML configs: {:?}",
            tokens
        );
    }

    #[test]
    fn test_strip_no_duplicates() {
        // If CamelCase splitting already produces "auth", stripping "Controller"
        // from "AuthController" should not produce a duplicate "auth"
        let mut tokenizer = CodeTokenizer::new(vec![]);
        tokenizer.set_strip_rules(vec![], vec!["Controller".into()]);

        let tokens = collect_tokens(tokenizer, "AuthController");

        // Count how many times "auth" appears — should be exactly 1
        let auth_count = tokens.iter().filter(|t| t.as_str() == "auth").count();
        assert_eq!(
            auth_count, 1,
            "Should not emit duplicate 'auth' tokens: {:?}",
            tokens
        );
    }
}
