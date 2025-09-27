use crate::extractors::base::{SymbolKind, Visibility};
use crate::extractors::regex::RegexExtractor;
use crate::tests::test_utils::init_parser;

fn extract_symbols(code: &str) -> Vec<crate::extractors::base::Symbol> {
    let tree = init_parser(code, "regex");
    let mut extractor = RegexExtractor::new(
        "regex".to_string(),
        "test.regex".to_string(),
        code.to_string(),
    );
    extractor.extract_symbols(&tree)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_simple_patterns() {
        let regex_code = r#"
// Basic patterns
hello
world
test123

// Character classes
[abc]
[a-z]
[A-Z]
[0-9]

// Quantifiers
a?
a*
a+

// Anchors
^
$
"#;

        let symbols = extract_symbols(regex_code);

        // Should extract at least some symbols
        assert!(symbols.len() > 0);

        // Basic literals should be found
        let hello_pattern = symbols.iter().find(|s| s.name == "hello");
        assert!(hello_pattern.is_some());
        if let Some(symbol) = hello_pattern {
            assert_eq!(symbol.kind, SymbolKind::Variable);
        }

        // Character classes should be found
        let abc_class = symbols.iter().find(|s| s.name == "[abc]");
        assert!(abc_class.is_some());
        if let Some(symbol) = abc_class {
            assert_eq!(symbol.kind, SymbolKind::Class);
        }

        // Anchors should be found
        let start_anchor = symbols.iter().find(|s| s.name == "^");
        assert!(start_anchor.is_some());
        if let Some(symbol) = start_anchor {
            assert_eq!(symbol.kind, SymbolKind::Constant);
        }
    }

    #[test]
    fn test_extract_predefined_classes() {
        let regex_code = r#"
\d
\w
\s
.
"#;

        let symbols = extract_symbols(regex_code);

        // Should extract predefined character classes
        let digit_class = symbols.iter().find(|s| s.name == "\\d");
        assert!(digit_class.is_some());
        if let Some(symbol) = digit_class {
            assert_eq!(symbol.kind, SymbolKind::Constant);
        }

        let word_class = symbols.iter().find(|s| s.name == "\\w");
        assert!(word_class.is_some());

        let space_class = symbols.iter().find(|s| s.name == "\\s");
        assert!(space_class.is_some());

        let any_char = symbols.iter().find(|s| s.name == ".");
        assert!(any_char.is_some());
    }

    #[test]
    fn test_extract_quantifiers() {
        let regex_code = r#"
a?
a*
a+
a{3}
"#;

        let symbols = extract_symbols(regex_code);

        // Should extract quantified patterns
        let optional = symbols.iter().find(|s| s.name == "a?");
        assert!(optional.is_some());
        if let Some(symbol) = optional {
            assert_eq!(symbol.kind, SymbolKind::Function);
        }

        let zero_or_more = symbols.iter().find(|s| s.name == "a*");
        assert!(zero_or_more.is_some());

        let one_or_more = symbols.iter().find(|s| s.name == "a+");
        assert!(one_or_more.is_some());

        let exact_count = symbols.iter().find(|s| s.name == "a{3}");
        assert!(exact_count.is_some());
    }

    #[test]
    fn test_extract_groups() {
        let regex_code = r#"
(abc)
(?:def)
"#;

        let symbols = extract_symbols(regex_code);

        // Should extract groups
        let capturing_group = symbols.iter().find(|s| s.name == "(abc)");
        assert!(capturing_group.is_some());
        if let Some(symbol) = capturing_group {
            assert_eq!(symbol.kind, SymbolKind::Class);
        }

        let non_capturing_group = symbols.iter().find(|s| s.name == "(?:def)");
        assert!(non_capturing_group.is_some());
    }

    #[test]
    fn test_extract_alternation() {
        let regex_code = r#"
cat|dog|bird
red|blue|green
"#;

        let symbols = extract_symbols(regex_code);

        // Should extract alternation patterns
        let animal_alt = symbols.iter().find(|s| s.name == "cat|dog|bird");
        assert!(animal_alt.is_some());

        let color_alt = symbols.iter().find(|s| s.name == "red|blue|green");
        assert!(color_alt.is_some());
    }

    #[test]
    fn test_symbol_metadata() {
        let regex_code = r#"
hello
[abc]
a+
"#;

        let symbols = extract_symbols(regex_code);

        // Check that symbols have proper metadata
        let hello_symbol = symbols.iter().find(|s| s.name == "hello");
        assert!(hello_symbol.is_some());

        if let Some(symbol) = hello_symbol {
            assert!(symbol
                .metadata
                .as_ref()
                .map(|m| m.contains_key("type"))
                .unwrap_or(false));
            assert_eq!(symbol.visibility, Some(Visibility::Public));
            assert!(symbol.signature.is_some());
        }
    }
}
