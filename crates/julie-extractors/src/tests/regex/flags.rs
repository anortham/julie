/// Inline tests extracted from extractors/regex/flags.rs
///
/// This module contains all tests for regex flag utility functions including:
/// - Anchor type detection (start, end, word-boundary, etc.)
/// - Lookaround direction and polarity (lookahead/lookbehind, positive/negative)
/// - Alternation option extraction
/// - Predefined character class categorization
/// - Unicode property extraction
/// - Backreference extraction
/// - Conditional pattern extraction

#[cfg(test)]
mod tests {
    use crate::regex::flags::*;

    #[test]
    fn test_get_anchor_type() {
        assert_eq!(get_anchor_type("^"), "start");
        assert_eq!(get_anchor_type("$"), "end");
        assert_eq!(get_anchor_type(r"\b"), "word-boundary");
    }

    #[test]
    fn test_get_lookaround_direction() {
        assert_eq!(get_lookaround_direction("(?=...)"), "lookahead");
        assert_eq!(get_lookaround_direction("(?<=...)"), "lookbehind");
    }

    #[test]
    fn test_is_positive_lookaround() {
        assert!(is_positive_lookaround("(?=...)"));
        assert!(is_positive_lookaround("(?<=...)"));
        assert!(!is_positive_lookaround("(?!...)"));
    }

    #[test]
    fn test_extract_alternation_options() {
        let options = extract_alternation_options("cat|dog|bird");
        assert_eq!(options.len(), 3);
        assert_eq!(options[0], "cat");
    }

    #[test]
    fn test_get_predefined_class_category() {
        assert_eq!(get_predefined_class_category(r"\d"), "digit");
        assert_eq!(get_predefined_class_category(r"\w"), "word");
    }

    #[test]
    fn test_extract_unicode_property_name() {
        assert_eq!(extract_unicode_property_name(r"\p{Letter}"), "Letter");
    }

    #[test]
    fn test_extract_group_number() {
        assert_eq!(extract_group_number(r"\1"), Some("1".to_string()));
        assert_eq!(extract_group_number(r"\42"), Some("42".to_string()));
    }

    #[test]
    fn test_extract_backref_group_name() {
        assert_eq!(
            extract_backref_group_name(r"\k<name>"),
            Some("name".to_string())
        );
        assert_eq!(
            extract_backref_group_name("(?P=email)"),
            Some("email".to_string())
        );
    }

    #[test]
    fn test_extract_condition() {
        assert_eq!(extract_condition("(?(1)yes|no)"), "1");
    }
}
