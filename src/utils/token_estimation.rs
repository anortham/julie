// RED: Write failing tests first, then implement minimal code to pass

pub struct TokenEstimator {
    // Placeholder - will implement after tests
}

impl TokenEstimator {
    /// Average characters per token for English text (verified from COA framework)
    const CHARS_PER_TOKEN: f64 = 4.0;

    /// Average characters per token for CJK languages (verified from COA framework)
    const CJK_CHARS_PER_TOKEN: f64 = 2.0;

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

    /// Detect if text contains CJK (Chinese, Japanese, Korean) characters
    /// Uses verified Unicode ranges from TokenEstimator.cs
    pub fn contains_cjk(&self, text: &str) -> bool {
        for ch in text.chars() {
            let code = ch as u32;
            if (code >= 0x4E00 && code <= 0x9FFF) ||  // CJK Unified Ideographs
               (code >= 0x3400 && code <= 0x4DBF) ||  // CJK Extension A
               (code >= 0x3040 && code <= 0x30FF) ||  // Hiragana and Katakana
               (code >= 0xAC00 && code <= 0xD7AF) {   // Hangul Syllables
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
        assert!(tokens >= 2 && tokens <= 4, "Expected 2-4 tokens for 'Hello world', got {}", tokens);
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
        assert_eq!(japanese_tokens, 5, "Japanese should be 5 tokens with 2 chars/token ratio (9 chars / 2)");
        assert_eq!(english_tokens, 2, "English should be 2 tokens with 4 chars/token ratio");
    }

    #[test]
    fn test_hybrid_formula_not_implemented_yet() {
        let estimator = TokenEstimator::new();

        // Test text where character-based and word-based would give different results
        // "hello world test" = 16 chars, 3 words
        // Character-based: 16/4 = 4 tokens
        // Word-based: 3 * 1.3 = 3.9 -> 4 tokens
        // Hybrid (0.6 char + 0.4 word): (4 * 0.6) + (4 * 0.4) = 2.4 + 1.6 = 4 tokens

        // But let's use text where they differ more:
        // "a" = 1 char, 1 word
        // Character-based: 1/4 = 0.25 -> 1 token
        // Word-based: 1 * 1.3 = 1.3 -> 2 tokens
        // Hybrid: (1 * 0.6) + (2 * 0.4) = 0.6 + 0.8 = 1.4 -> 2 tokens (should be more than pure char-based)

        let tokens = estimator.estimate_string("a");

        // Currently it just uses character-based (1 char / 4 = 0.25 -> 1 token)
        // With hybrid it should be higher (closer to 2)
        // For now this will pass with current implementation, but will fail when we add word counting
        assert_eq!(tokens, 1, "Single character should be 1 token with current char-only approach");
    }
}