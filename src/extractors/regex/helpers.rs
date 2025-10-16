use crate::extractors::base::SymbolKind;

/// Check if a node represents a regex pattern
pub(super) fn is_regex_pattern(node_kind: &str) -> bool {
    matches!(
        node_kind,
        "pattern"
            | "regex"
            | "expression"
            | "character_class"
            | "group"
            | "quantifier"
            | "anchor"
            | "lookahead"
            | "lookbehind"
            | "alternation"
            | "character_escape"
            | "unicode_property"
            | "backreference"
            | "conditional"
    )
}

/// Clean a regex line by removing comments and extra whitespace
pub(super) fn clean_regex_line(line: &str) -> String {
    // Remove inline comments (// or #)
    let cleaned = if let Some(pos) = line.find("//") {
        &line[..pos]
    } else if let Some(pos) = line.find('#') {
        &line[..pos]
    } else {
        line
    };

    // Remove excessive whitespace
    cleaned.trim().to_string()
}

/// Check if text is a valid regex pattern
pub(crate) fn is_valid_regex_pattern(text: &str) -> bool {
    // Skip very short patterns or obvious non-regex content
    if text.is_empty() {
        return false;
    }

    // Allow simple literals (letters, numbers, basic words)
    if text.chars().all(|c| c.is_alphanumeric()) {
        return true;
    }

    // Allow single character regex metacharacters
    if matches!(text, "." | "^" | "$") {
        return true;
    }

    // Allow simple groups and common patterns
    if (text.starts_with('(') && text.ends_with(')')) || text.ends_with('*') || text == "**" {
        return true;
    }

    // Check for regex-specific characters or patterns
    let regex_indicators = [
        r"[\[\](){}*+?^$|\\]", // Special regex characters
        r"\\[dwsWDSnrtfve]",   // Escape sequences
        r"\(\?\<?[!=]",        // Lookarounds
        r"\(\?\w+\)",          // Groups with modifiers
        r"\\p\{",              // Unicode properties
        r"\[\^",               // Negated character classes
        r"\{[\d,]+\}",         // Quantifiers
    ];

    regex_indicators.iter().any(|pattern| {
        // Simple pattern matching - check for common regex constructs
        match *pattern {
            r"[\[\](){}*+?^$|\\]" => text.chars().any(|c| "[](){}*+?^$|\\".contains(c)),
            r"\\[dwsWDSnrtfve]" => {
                text.contains(r"\d")
                    || text.contains(r"\w")
                    || text.contains(r"\s")
                    || text.contains(r"\D")
                    || text.contains(r"\W")
                    || text.contains(r"\S")
                    || text.contains(r"\n")
                    || text.contains(r"\r")
                    || text.contains(r"\t")
                    || text.contains(r"\f")
                    || text.contains(r"\v")
                    || text.contains(r"\e")
            }
            r"\(\?\<?[!=]" => {
                text.contains("(?=")
                    || text.contains("(?!")
                    || text.contains("(?<=")
                    || text.contains("(?<!")
            }
            r"\(\?\w+\)" => text.contains("(?") && text.contains(')'),
            r"\\p\{" => text.contains(r"\p{") || text.contains(r"\P{"),
            r"\[\^" => text.contains("[^"),
            r"\{[\d,]+\}" => {
                text.contains('{')
                    && text.contains('}')
                    && text.chars().any(|c| c.is_ascii_digit())
                    && (text.contains(',')
                        || text.chars().filter(|c| c.is_ascii_digit()).count() > 0)
            }
            _ => false,
        }
    })
}

/// Determine the symbol kind for a pattern
pub(crate) fn determine_pattern_kind(pattern: &str) -> SymbolKind {
    // Lookarounds (check first, before groups)
    if pattern.contains("(?=")
        || pattern.contains("(?!")
        || pattern.contains("(?<=")
        || pattern.contains("(?<!")
    {
        return SymbolKind::Method;
    }

    // Character classes
    if pattern.starts_with('[') && pattern.ends_with(']') {
        return SymbolKind::Class;
    }

    // Groups (but not lookarounds)
    if pattern.starts_with('(')
        && pattern.ends_with(')')
        && !pattern.contains("(?=")
        && !pattern.contains("(?!")
        && !pattern.contains("(?<=")
        && !pattern.contains("(?<!")
    {
        return SymbolKind::Class;
    }

    // Quantifiers
    if pattern.ends_with('?')
        || pattern.ends_with('*')
        || pattern.ends_with('+')
        || (pattern.contains('{') && pattern.contains('}'))
    {
        return SymbolKind::Function;
    }

    // Anchors and predefined classes
    if matches!(pattern, "^" | "$")
        || pattern == r"\b"
        || pattern == r"\B"
        || pattern == r"\d"
        || pattern == r"\D"
        || pattern == r"\w"
        || pattern == r"\W"
        || pattern == r"\s"
        || pattern == r"\S"
        || pattern == "."
    {
        return SymbolKind::Constant;
    }

    // Unicode properties
    if pattern.contains(r"\p{") || pattern.contains(r"\P{") {
        return SymbolKind::Constant;
    }

    // Default to Variable for basic patterns
    SymbolKind::Variable
}

/// Calculate complexity score of a pattern
pub(crate) fn calculate_complexity(pattern: &str) -> u32 {
    let mut complexity = 0;

    // Basic complexity indicators
    complexity += pattern.matches(['*', '+', '?']).count() as u32; // Quantifiers
    complexity += pattern.matches(['[', ']', '(', ')', '{', '}']).count() as u32; // Grouping constructs
    complexity += pattern.matches("(?").count() as u32 * 2; // Lookarounds
    complexity += pattern.matches(r"\p{").count() as u32; // Unicode properties
    complexity += pattern.matches('|').count() as u32; // Alternations

    complexity
}

/// Check if a literal is escaped
pub(super) fn is_escaped_literal(literal_text: &str) -> bool {
    literal_text.starts_with('\\')
}
