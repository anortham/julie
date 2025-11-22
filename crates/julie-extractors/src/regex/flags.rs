/// Get the type/position of an anchor
pub(crate) fn get_anchor_type(anchor_text: &str) -> String {
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
pub(crate) fn get_lookaround_direction(lookaround_text: &str) -> String {
    if lookaround_text.contains("(?<=") || lookaround_text.contains("(?<!") {
        "lookbehind".to_string()
    } else {
        "lookahead".to_string()
    }
}

/// Check if a lookaround is positive (vs negative)
pub(crate) fn is_positive_lookaround(lookaround_text: &str) -> bool {
    lookaround_text.contains("(?=") || lookaround_text.contains("(?<=")
}

/// Extract alternation options separated by |
pub(crate) fn extract_alternation_options(alternation_text: &str) -> Vec<String> {
    alternation_text
        .split('|')
        .map(|s| s.trim().to_string())
        .collect()
}

/// Get the category of a predefined character class
pub(crate) fn get_predefined_class_category(class_text: &str) -> String {
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
pub(crate) fn extract_unicode_property_name(property_text: &str) -> String {
    if let Some(start) = property_text
        .find(r"\p{")
        .or_else(|| property_text.find(r"\P{"))
    {
        if let Some(end) = property_text[start..].find('}') {
            let inner_start = start + 3;
            let inner_end = start + end;
            // SAFETY: Check char boundaries before slicing to prevent UTF-8 panic
            if property_text.is_char_boundary(inner_start)
                && property_text.is_char_boundary(inner_end)
            {
                let inner = &property_text[inner_start..inner_end];
                return inner.to_string();
            }
        }
    }
    "unknown".to_string()
}

/// Extract group number from a numeric backreference like \1 or \2
pub(crate) fn extract_group_number(backref_text: &str) -> Option<String> {
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
pub(crate) fn extract_backref_group_name(backref_text: &str) -> Option<String> {
    if let Some(start) = backref_text.find(r"\k<") {
        if let Some(end) = backref_text[start + 3..].find('>') {
            let end_idx = start + 3 + end;
            // SAFETY: Check char boundary before slicing to prevent UTF-8 panic
            if backref_text.is_char_boundary(start + 3) && backref_text.is_char_boundary(end_idx) {
                return Some(backref_text[start + 3..end_idx].to_string());
            }
        }
    }
    if let Some(start) = backref_text.find("(?P=") {
        if let Some(end) = backref_text[start + 4..].find(')') {
            let end_idx = start + 4 + end;
            // SAFETY: Check char boundary before slicing to prevent UTF-8 panic
            if backref_text.is_char_boundary(start + 4) && backref_text.is_char_boundary(end_idx) {
                return Some(backref_text[start + 4..end_idx].to_string());
            }
        }
    }
    None
}

/// Extract the condition from a conditional pattern like (?(1)...)
pub(crate) fn extract_condition(conditional_text: &str) -> String {
    if let Some(start) = conditional_text.find("(?(") {
        if let Some(end) = conditional_text[start + 3..].find(')') {
            let cond_start = start + 3;
            let cond_end = start + 3 + end;
            // SAFETY: Check char boundary before slicing to prevent UTF-8 panic
            if conditional_text.is_char_boundary(cond_start)
                && conditional_text.is_char_boundary(cond_end)
            {
                return conditional_text[cond_start..cond_end].to_string();
            }
        }
    }
    "unknown".to_string()
}
