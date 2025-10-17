//! General helper utilities for refactoring operations

/// Check if a comment line looks like a doc comment
pub fn looks_like_doc_comment(comment: &str) -> bool {
    let trimmed = comment.trim_start();

    trimmed.starts_with("///")
        || trimmed.starts_with("//!")
        || trimmed.starts_with("///<")
        || trimmed.starts_with("//!<")
        || trimmed.starts_with("/**")
        || trimmed.starts_with("/*!")
}

/// Replace identifier while respecting word boundaries
///
/// This function replaces text only when the replacement forms a complete identifier,
/// not part of a larger identifier. This is essential for correct symbol renaming.
pub fn replace_identifier_with_boundaries<F>(
    text: &str,
    old: &str,
    new: &str,
    is_identifier_char: &F,
) -> (String, bool)
where
    F: Fn(char) -> bool,
{
    if old.is_empty() {
        return (text.to_string(), false);
    }

    let mut result = String::with_capacity(text.len());
    let mut last_index = 0;
    let mut changed = false;

    for (idx, _) in text.match_indices(old) {
        let mut valid = true;

        if let Some(prev_char) = text[..idx].chars().rev().next() {
            if is_identifier_char(prev_char) {
                valid = false;
            }
        }

        let end = idx + old.len();
        if valid {
            if let Some(next_char) = text[end..].chars().next() {
                if is_identifier_char(next_char) {
                    valid = false;
                }
            }
        }

        if !valid {
            continue;
        }

        result.push_str(&text[last_index..idx]);
        result.push_str(new);
        last_index = end;
        changed = true;
    }

    result.push_str(&text[last_index..]);
    if changed {
        (result, true)
    } else {
        (text.to_string(), false)
    }
}
