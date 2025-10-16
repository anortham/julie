/// Get the type/position of an anchor
pub(super) fn get_anchor_type(anchor_text: &str) -> String {
    match anchor_text {
        "^" => "start".to_string(),
        "$" => "end".to_string(),
        r"\b" => "word-boundary".to_string(),
        r"\B" => "non-word-boundary".to_string(),
        r"\A" => "string-start".to_string(),
        r"\Z" => "string-end".to_string(),
        r"\z" => "absolute-end".to_string(),
        _ => "unknown".to_string(),
    }
}

/// Get the direction of a lookaround (lookahead vs lookbehind)
pub(super) fn get_lookaround_direction(lookaround_text: &str) -> String {
    if lookaround_text.contains("(?<=") || lookaround_text.contains("(?<!") {
        "lookbehind".to_string()
    } else {
        "lookahead".to_string()
    }
}

/// Check if a lookaround is positive (vs negative)
pub(super) fn is_positive_lookaround(lookaround_text: &str) -> bool {
    lookaround_text.contains("(?=") || lookaround_text.contains("(?<=")
}

/// Extract alternation options separated by |
pub(super) fn extract_alternation_options(alternation_text: &str) -> Vec<String> {
    alternation_text
        .split('|')
        .map(|s| s.trim().to_string())
        .collect()
}

/// Get the category of a predefined character class
pub(super) fn get_predefined_class_category(class_text: &str) -> String {
    match class_text {
        r"\d" => "digit".to_string(),
        r"\D" => "non-digit".to_string(),
        r"\w" => "word".to_string(),
        r"\W" => "non-word".to_string(),
        r"\s" => "whitespace".to_string(),
        r"\S" => "non-whitespace".to_string(),
        "." => "any-character".to_string(),
        r"\n" => "newline".to_string(),
        r"\r" => "carriage-return".to_string(),
        r"\t" => "tab".to_string(),
        r"\v" => "vertical-tab".to_string(),
        r"\f" => "form-feed".to_string(),
        r"\a" => "bell".to_string(),
        r"\e" => "escape".to_string(),
        _ => "other".to_string(),
    }
}

/// Extract unicode property name from pattern like \p{Letter}
pub(super) fn extract_unicode_property_name(property_text: &str) -> String {
    if let Some(start) = property_text
        .find(r"\p{")
        .or_else(|| property_text.find(r"\P{"))
    {
        if let Some(end) = property_text[start..].find('}') {
            let inner = &property_text[start + 3..start + end];
            return inner.to_string();
        }
    }
    "unknown".to_string()
}

/// Extract group number from a numeric backreference like \1 or \2
pub(super) fn extract_group_number(backref_text: &str) -> Option<String> {
    if let Some(start) = backref_text.find('\\') {
        let rest = &backref_text[start + 1..];
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !digits.is_empty() {
            return Some(digits);
        }
    }
    None
}

/// Extract group name from a named backreference like \k<name> or (?P=name)
pub(super) fn extract_backref_group_name(backref_text: &str) -> Option<String> {
    if let Some(start) = backref_text.find(r"\k<") {
        if let Some(end) = backref_text[start + 3..].find('>') {
            return Some(backref_text[start + 3..start + 3 + end].to_string());
        }
    }
    if let Some(start) = backref_text.find("(?P=") {
        if let Some(end) = backref_text[start + 4..].find(')') {
            return Some(backref_text[start + 4..start + 4 + end].to_string());
        }
    }
    None
}

/// Extract the condition from a conditional pattern like (?(1)...)
pub(super) fn extract_condition(conditional_text: &str) -> String {
    if let Some(start) = conditional_text.find("(?(") {
        if let Some(end) = conditional_text[start + 3..].find(')') {
            return conditional_text[start + 3..start + 3 + end].to_string();
        }
    }
    "unknown".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(
            extract_unicode_property_name(r"\p{Letter}"),
            "Letter"
        );
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
