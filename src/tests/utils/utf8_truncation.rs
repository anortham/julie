#[cfg(test)]
mod tests {
    use crate::utils::context_truncation::truncate_string;

    #[test]
    fn test_truncate_string_with_utf8() {
        // Test string from the error message with Icelandic characters
        let test_str =
            r#"[ "Jan","Feb","Mar","Apr","Maí","Jún","Júl","Ágú","Sep","Okt","Nóv","Des" ]"#;

        // Test truncation at 30 characters (where the original error occurred)
        let truncated = truncate_string(test_str, 30);
        assert!(truncated.chars().count() <= 33); // 30 + "..." = 33
        assert!(truncated.is_char_boundary(truncated.len())); // Verify valid UTF-8

        // Test truncation at 50 characters
        let truncated = truncate_string(test_str, 50);
        assert!(truncated.chars().count() <= 53); // 50 + "..." = 53
        assert!(truncated.is_char_boundary(truncated.len())); // Verify valid UTF-8

        // Test with already short string - should not add "..."
        let short = "short";
        let truncated = truncate_string(short, 30);
        assert_eq!(truncated, "short");

        // Test exact boundary
        let exact = "exactly_30_characters_here";
        let truncated = truncate_string(exact, 30);
        assert_eq!(truncated, exact);

        // Test Unicode characters at the boundary
        let unicode = "Test 日本語 characters";
        let truncated = truncate_string(unicode, 10);
        assert!(truncated.is_char_boundary(truncated.len())); // Should not panic
    }

    #[test]
    fn test_truncate_string_preserves_multibyte_chars() {
        // String with various multibyte UTF-8 characters
        let multibyte = "Ágú Maí Jún Júl Nóv Des"; // Icelandic months
        let truncated = truncate_string(multibyte, 10);

        // Should not panic and should be valid UTF-8
        assert!(truncated.is_char_boundary(truncated.len()));

        // The result should contain at most 10 characters + "..."
        let char_count = truncated.chars().count();
        assert!(
            char_count <= 13, // 10 + "..." = 13
            "Expected at most 13 characters, got {}",
            char_count
        );
    }

    #[test]
    fn test_truncate_string_with_emoji() {
        // Test with emoji which can be multiple bytes
        let emoji_str = "Hello 👋 World 🌍 Test 🚀";
        let truncated = truncate_string(emoji_str, 10);

        // Should not panic and should be valid UTF-8
        assert!(truncated.is_char_boundary(truncated.len()));
    }
}
