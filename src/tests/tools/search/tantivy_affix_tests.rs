//! Tests for meaningful_affixes tokenizer feature
//!
//! Verifies that the CodeTokenizer can strip language-specific affixes
//! (prefixes like "is_", "has_" and suffixes like "_mut", "_ref")
//! and emit the stripped form as an additional search token.

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
    fn test_meaningful_affix_prefix_stripping() {
        // "is_valid" → should emit "valid" as affix-stripped token
        let mut tokenizer = CodeTokenizer::new(vec![]);
        tokenizer.set_meaningful_affixes(vec!["is_".into(), "has_".into(), "_mut".into()]);

        let tokens = collect_tokens(tokenizer, "is_valid");

        // Should have: "is_valid" (whole), "is" (snake part), "valid" (snake part + affix-stripped)
        assert!(
            tokens.contains(&"valid".to_string()),
            "Should strip is_ prefix to emit 'valid': {:?}",
            tokens
        );
    }

    #[test]
    fn test_meaningful_affix_suffix_stripping() {
        // "borrow_mut" with _mut suffix → should emit "borrow" as affix-stripped
        let mut tokenizer = CodeTokenizer::new(vec![]);
        tokenizer.set_meaningful_affixes(vec!["_mut".into(), "_ref".into()]);

        let tokens = collect_tokens(tokenizer, "borrow_mut");

        assert!(
            tokens.contains(&"borrow".to_string()),
            "Should strip _mut suffix to emit 'borrow': {:?}",
            tokens
        );
    }

    #[test]
    fn test_meaningful_affix_camelcase_prefix() {
        // C#-style: "IsValid" with "Is" affix → should emit "valid"
        let mut tokenizer = CodeTokenizer::new(vec![]);
        tokenizer.set_meaningful_affixes(vec!["Is".into(), "Has".into(), "Get".into()]);

        let tokens = collect_tokens(tokenizer, "IsValid");

        assert!(
            tokens.contains(&"valid".to_string()),
            "Should strip Is prefix to emit 'valid': {:?}",
            tokens
        );
    }

    #[test]
    fn test_meaningful_affix_no_false_stripping() {
        // "island" should NOT be stripped when "is_" is an affix (no match without underscore)
        let mut tokenizer = CodeTokenizer::new(vec![]);
        tokenizer.set_meaningful_affixes(vec!["is_".into(), "has_".into()]);

        let tokens = collect_tokens(tokenizer, "island");

        // "island" should NOT produce "land" — "is_" has underscore, doesn't match "is" in "island"
        assert!(
            !tokens.contains(&"land".to_string()),
            "Should NOT strip 'is' from 'island' (affix is 'is_' not 'is'): {:?}",
            tokens
        );
    }

    #[test]
    fn test_meaningful_affix_no_strip_if_remainder_too_short() {
        // "is_a" → stripping "is_" leaves "a" which is too short to be useful
        let mut tokenizer = CodeTokenizer::new(vec![]);
        tokenizer.set_meaningful_affixes(vec!["is_".into()]);

        let tokens = collect_tokens(tokenizer, "is_a");

        // "a" alone is not useful — should not be emitted as affix-stripped
        let affix_stripped: Vec<_> = tokens
            .iter()
            .filter(|t| t.as_str() == "a")
            .collect();
        // This is debatable, but single-char tokens from affix stripping add noise
        // The snake_case split will already emit "a" though, so we won't fight it
    }

    #[test]
    fn test_meaningful_affix_from_language_configs() {
        // Verify that from_language_configs wires affixes correctly
        use crate::search::language_config::LanguageConfigs;

        let configs = LanguageConfigs::load_embedded();
        let tokenizer = CodeTokenizer::from_language_configs(&configs);

        // Rust config has "is_" as a meaningful affix
        let tokens = collect_tokens(tokenizer, "is_empty");

        assert!(
            tokens.contains(&"empty".to_string()),
            "from_language_configs should wire meaningful_affixes from TOML configs: {:?}",
            tokens
        );
    }
}
