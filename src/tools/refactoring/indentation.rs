//! Indentation handling utilities for code refactoring
//!
//! Provides functions to normalize and reapply indentation for multi-line code snippets.

/// Detect the minimum indentation level in a multi-line string (ignoring empty lines)
pub fn detect_min_indentation(text: &str) -> usize {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0)
}

/// Normalize indentation by removing the minimum common indentation from all lines
/// This brings multi-line content to "column 0" while preserving relative indentation
pub fn normalize_indentation(text: &str) -> String {
    let min_indent = detect_min_indentation(text);

    text.lines()
        .map(|line| {
            if line.trim().is_empty() {
                String::new()  // Empty lines stay empty
            } else if line.len() >= min_indent {
                line[min_indent..].to_string()  // Strip min_indent spaces
            } else {
                line.to_string()  // Line is shorter than min_indent (shouldn't happen)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Apply target indentation to normalized content
/// Adds `target_indent` spaces to each non-empty line
pub fn apply_indentation(normalized_text: &str, target_indent: usize) -> String {
    let indent_str = " ".repeat(target_indent);

    normalized_text
        .lines()
        .map(|line| {
            if line.trim().is_empty() {
                String::new()  // Empty lines stay empty
            } else {
                format!("{}{}", indent_str, line)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Complete indentation transformation pipeline:
/// 1. Detect minimum indentation in source
/// 2. Normalize to column 0
/// 3. Reapply at target indentation level
pub fn reindent(text: &str, target_indent: usize) -> String {
    let normalized = normalize_indentation(text);
    apply_indentation(&normalized, target_indent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_min_indentation() {
        let text = "    line1\n        line2\n    line3";
        assert_eq!(detect_min_indentation(text), 4);

        let text = "line1\n    line2\n        line3";
        assert_eq!(detect_min_indentation(text), 0);

        let text = "        line1\n        line2";
        assert_eq!(detect_min_indentation(text), 8);
    }

    #[test]
    fn test_normalize_indentation() {
        let text = "    line1\n        line2\n    line3";
        let expected = "line1\n    line2\nline3";
        assert_eq!(normalize_indentation(text), expected);

        let text = "        def foo():\n            return 1";
        let expected = "def foo():\n    return 1";
        assert_eq!(normalize_indentation(text), expected);
    }

    #[test]
    fn test_apply_indentation() {
        let text = "line1\n    line2\nline3";
        let expected = "    line1\n        line2\n    line3";
        assert_eq!(apply_indentation(text, 4), expected);
    }

    #[test]
    fn test_reindent_complete_pipeline() {
        // Source has 8 spaces, target needs 4 spaces
        let source = "        def foo():\n            return 1";
        let expected = "    def foo():\n        return 1";
        assert_eq!(reindent(source, 4), expected);

        // Source has no indent, target needs 8 spaces
        let source = "def bar():\n    pass";
        let expected = "        def bar():\n            pass";
        assert_eq!(reindent(source, 8), expected);
    }

    #[test]
    fn test_empty_lines_preserved() {
        let text = "    line1\n\n    line2";
        let normalized = normalize_indentation(text);
        assert_eq!(normalized, "line1\n\nline2");

        let reindented = apply_indentation(&normalized, 4);
        assert_eq!(reindented, "    line1\n\n    line2");
    }
}
