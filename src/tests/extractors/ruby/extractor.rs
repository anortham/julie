/// Inline tests extracted from extractors/ruby/mod.rs
///
/// These tests verify the Ruby extractor creation and basic functionality.
/// Ported from the inline test module for centralized test organization.

#[cfg(test)]
mod ruby_extractor_tests {
    use crate::extractors::ruby::RubyExtractor;

    #[test]
    fn test_ruby_extractor_creation() {
        // Verify that RubyExtractor can be created successfully
        // The constructor should not panic and should accept valid file paths and content
        let _extractor =
            RubyExtractor::new("test.rb".to_string(), "class MyClass\nend".to_string());
        // If we reach here without panicking, the test passes
        assert!(true);
    }
}
