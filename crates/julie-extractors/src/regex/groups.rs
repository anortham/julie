/// Check if a group is a capturing group
pub(crate) fn is_capturing_group(group_text: &str) -> bool {
    !group_text.starts_with("(?:")
        && !group_text.starts_with("(?<")
        && !group_text.starts_with("(?P<")
}

/// Extract the name from a named group
pub(crate) fn extract_group_name(group_text: &str) -> Option<String> {
    if let Some(start) = group_text.find("(?<") {
        if let Some(end) = group_text[start + 3..].find('>') {
            let end_idx = start + 3 + end;
            // SAFETY: Check char boundary before slicing to prevent UTF-8 panic
            if group_text.is_char_boundary(start + 3) && group_text.is_char_boundary(end_idx) {
                return Some(group_text[start + 3..end_idx].to_string());
            }
        }
    }
    if let Some(start) = group_text.find("(?P<") {
        if let Some(end) = group_text[start + 4..].find('>') {
            let end_idx = start + 4 + end;
            // SAFETY: Check char boundary before slicing to prevent UTF-8 panic
            if group_text.is_char_boundary(start + 4) && group_text.is_char_boundary(end_idx) {
                return Some(group_text[start + 4..end_idx].to_string());
            }
        }
    }
    None
}
