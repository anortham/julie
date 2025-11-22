// Inline tests extracted from extractors/regex/classes.rs
//
// This module contains tests for regex character class utilities, specifically
// testing the is_negated_class function which determines if a pattern represents
// a negated character class (e.g., [^a-z]).

use crate::regex::classes::is_negated_class;

#[test]
fn test_is_negated_class() {
    assert!(is_negated_class("[^a-z]"));
    assert!(!is_negated_class("[a-z]"));
}
