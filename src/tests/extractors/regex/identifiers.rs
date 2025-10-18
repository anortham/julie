//! Tests extracted from extractors/regex/identifiers.rs
//!
//! This module contains all inline tests that were previously in the regex identifiers module.
//! Tests verify the extraction of identifier usages (backreferences and named groups) in regex patterns.

#[cfg(test)]
mod tests {
    use crate::extractors::regex::identifiers::extract_group_name;

    #[test]
    fn test_extract_group_name() {
        assert_eq!(extract_group_name("(?<name>...)"), Some("name".to_string()));
        assert_eq!(
            extract_group_name("(?P<name>...)"),
            Some("name".to_string())
        );
        assert_eq!(extract_group_name("(abc)"), None);
    }
}
