// Inline tests extracted from src/extractors/regex/mod.rs
//
// This module contains tests for the RegexExtractor implementation,
// originally embedded in the source module and extracted for better code organization.

use crate::extractors::regex::RegexExtractor;

#[test]
fn test_regex_extractor_creation() {
    let extractor = RegexExtractor::new(
        "regex".to_string(),
        "/test/file.regex".to_string(),
        "[a-z]+".to_string(),
    );
    assert_eq!(extractor.base.language, "regex");
}
