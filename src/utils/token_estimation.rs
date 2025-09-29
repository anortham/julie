// RED: Write failing tests first, then implement minimal code to pass

pub struct TokenEstimator {
    // Placeholder - will implement after tests
}

impl Default for TokenEstimator {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenEstimator {
    /// Average characters per token for English text (verified from COA framework)
    const CHARS_PER_TOKEN: f64 = 4.0;

    /// Average characters per token for CJK languages (verified from COA framework)
    const CJK_CHARS_PER_TOKEN: f64 = 2.0;

    /// Average words per token multiplier (verified from COA framework)
    const WORDS_PER_TOKEN_MULTIPLIER: f64 = 1.3;

    /// Hybrid formula weights (verified from COA framework)
    const CHAR_WEIGHT: f64 = 0.6;
    const WORD_WEIGHT: f64 = 0.4;

    pub fn new() -> Self {
        Self {}
    }

    pub fn estimate_string(&self, text: &str) -> usize {
        if text.is_empty() {
            0
        } else {
            // Detect if text contains CJK characters
            let use_cjk_rate = self.contains_cjk(text);
            let chars_per_token = if use_cjk_rate {
                Self::CJK_CHARS_PER_TOKEN
            } else {
                Self::CHARS_PER_TOKEN
            };

            // Character-based estimation using language-appropriate ratio
            // Use chars().count() for actual character count, not byte count
            (text.chars().count() as f64 / chars_per_token).ceil() as usize
        }
    }

    /// Estimate tokens using word-based counting
    /// Uses verified multiplier from COA framework
    pub fn estimate_words(&self, text: &str) -> usize {
        if text.is_empty() {
            0
        } else {
            let word_count = text.split_whitespace().count();
            if word_count == 0 {
                0
            } else {
                // Apply word-based multiplier
                (word_count as f64 * Self::WORDS_PER_TOKEN_MULTIPLIER).ceil() as usize
            }
        }
    }

    /// Estimate tokens using hybrid formula (0.6 char + 0.4 word)
    /// Verified from COA framework TokenEstimator.cs:86
    pub fn estimate_string_hybrid(&self, text: &str) -> usize {
        if text.is_empty() {
            0
        } else {
            let char_based = self.estimate_string(text) as f64;
            let word_based = self.estimate_words(text) as f64;

            // Apply hybrid formula: 0.6 * char_based + 0.4 * word_based
            let hybrid_result = (char_based * Self::CHAR_WEIGHT) + (word_based * Self::WORD_WEIGHT);
            hybrid_result.ceil() as usize
        }
    }

    /// Detect if text contains CJK (Chinese, Japanese, Korean) characters
    /// Uses verified Unicode ranges from TokenEstimator.cs
    pub fn contains_cjk(&self, text: &str) -> bool {
        for ch in text.chars() {
            let code = ch as u32;
            if (0x4E00..=0x9FFF).contains(&code) ||  // CJK Unified Ideographs
               (0x3400..=0x4DBF).contains(&code) ||  // CJK Extension A
               (0x3040..=0x30FF).contains(&code) ||  // Hiragana and Katakana
               (0xAC00..=0xD7AF).contains(&code)
            {
                // Hangul Syllables
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_string_returns_zero_tokens() {
        let estimator = TokenEstimator::new();
        assert_eq!(estimator.estimate_string(""), 0);
    }

    #[test]
    fn test_english_text_uses_four_chars_per_token() {
        let estimator = TokenEstimator::new();
        // "Hello world" = 11 chars, should be roughly 11/4 = 2.75 -> 3 tokens
        let tokens = estimator.estimate_string("Hello world");
        assert!(
            tokens >= 2 && tokens <= 4,
            "Expected 2-4 tokens for 'Hello world', got {}",
            tokens
        );
    }

    #[test]
    fn test_cjk_detection_uses_different_ratios() {
        let estimator = TokenEstimator::new();

        // Test specific case: 9-char Japanese vs 8-char English
        // Japanese "こんにちは世界です" = 9 chars -> should be 9/2 = 4.5 -> 5 tokens with CJK detection
        // English "hellowld" = 8 chars -> should be 8/4 = 2 tokens
        let japanese_text = "こんにちは世界です"; // 9 Japanese characters
        let english_text = "hellowld"; // 8 English characters

        let japanese_tokens = estimator.estimate_string(japanese_text);
        let english_tokens = estimator.estimate_string(english_text);

        // Verify correct calculation:
        // Japanese: 9 chars with CJK ratio (2 chars/token) -> 9/2 = 4.5 -> 5 tokens
        // English: 8 chars with English ratio (4 chars/token) -> 8/4 = 2 tokens

        // Without CJK detection, they should be the same (both use 4 chars/token)
        // With CJK detection, Japanese should be double (using 2 chars/token)
        // Let's explicitly test for CJK giving more tokens
        assert_eq!(
            japanese_tokens, 5,
            "Japanese should be 5 tokens with 2 chars/token ratio (9 chars / 2)"
        );
        assert_eq!(
            english_tokens, 2,
            "English should be 2 tokens with 4 chars/token ratio"
        );
    }

    #[test]
    fn test_word_based_estimation() {
        let estimator = TokenEstimator::new();

        // Test word-based counting
        // "hello world test" = 3 words
        // Word-based: 3 * 1.3 = 3.9 -> 4 tokens
        let tokens = estimator.estimate_words("hello world test");
        assert_eq!(
            tokens, 4,
            "3 words should estimate to 4 tokens using 1.3 multiplier"
        );

        // Test single word
        let tokens = estimator.estimate_words("hello");
        assert_eq!(
            tokens, 2,
            "1 word should estimate to 2 tokens using 1.3 multiplier (1.3 -> 2)"
        );

        // Test empty string
        let tokens = estimator.estimate_words("");
        assert_eq!(tokens, 0, "Empty string should be 0 tokens");
    }

    #[test]
    fn test_hybrid_formula_implementation() {
        let estimator = TokenEstimator::new();

        // Test where character-based and word-based give different results
        // "a" = 1 char, 1 word
        // Character-based: 1/4 = 0.25 -> 1 token
        // Word-based: 1 * 1.3 = 1.3 -> 2 tokens
        // Hybrid (0.6 char + 0.4 word): (1 * 0.6) + (2 * 0.4) = 0.6 + 0.8 = 1.4 -> 2 tokens
        let tokens = estimator.estimate_string_hybrid("a");
        assert_eq!(
            tokens, 2,
            "Single character 'a' should be 2 tokens with hybrid formula"
        );

        // Test longer text where hybrid makes a difference
        // "hello world" = 11 chars, 2 words
        // Character-based: 11/4 = 2.75 -> 3 tokens
        // Word-based: 2 * 1.3 = 2.6 -> 3 tokens
        // Hybrid: (3 * 0.6) + (3 * 0.4) = 1.8 + 1.2 = 3.0 -> 3 tokens
        let tokens = estimator.estimate_string_hybrid("hello world");
        assert_eq!(
            tokens, 3,
            "Text 'hello world' should be 3 tokens with hybrid formula"
        );

        // Test where hybrid differs from pure character-based
        // "x y z" = 5 chars, 3 words
        // Character-based: 5/4 = 1.25 -> 2 tokens
        // Word-based: 3 * 1.3 = 3.9 -> 4 tokens
        // Hybrid: (2 * 0.6) + (4 * 0.4) = 1.2 + 1.6 = 2.8 -> 3 tokens
        let tokens = estimator.estimate_string_hybrid("x y z");
        assert_eq!(
            tokens, 3,
            "Text 'x y z' should be 3 tokens with hybrid formula"
        );
    }

    #[test]
    fn test_current_estimate_string_uses_character_only() {
        let estimator = TokenEstimator::new();

        // Current estimate_string should still use character-based only for backward compatibility
        // Until we explicitly switch it to hybrid
        let tokens = estimator.estimate_string("a");
        assert_eq!(
            tokens, 1,
            "Single character should be 1 token with current char-only approach"
        );
    }
}
