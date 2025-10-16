/// Check if a group is a capturing group
pub(super) fn is_capturing_group(group_text: &str) -> bool {
    !group_text.starts_with("(?:")
        && !group_text.starts_with("(?<")
        && !group_text.starts_with("(?P<")
}

/// Extract the name from a named group
pub(super) fn extract_group_name(group_text: &str) -> Option<String> {
    if let Some(start) = group_text.find("(?<") {
        if let Some(end) = group_text[start + 3..].find('>') {
            return Some(group_text[start + 3..start + 3 + end].to_string());
        }
    }
    if let Some(start) = group_text.find("(?P<") {
        if let Some(end) = group_text[start + 4..].find('>') {
            return Some(group_text[start + 4..start + 4 + end].to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_capturing_group() {
        assert!(is_capturing_group("(abc)"));
        assert!(!is_capturing_group("(?:abc)"));
        assert!(!is_capturing_group("(?<name>abc)"));
    }

    #[test]
    fn test_extract_group_name() {
        assert_eq!(extract_group_name("(?<name>...)"), Some("name".to_string()));
        assert_eq!(extract_group_name("(?P<name>...)"), Some("name".to_string()));
        assert_eq!(extract_group_name("(abc)"), None);
    }
}
