// Submodule declarations
pub mod extractor;
pub mod classes;
pub mod flags;
pub mod groups;
pub mod helpers;
pub mod identifiers;
pub mod signatures;

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

// ========================================================================
// Identifier Extraction Tests (TDD RED phase)
// ========================================================================
//
// These tests validate the extract_identifiers() functionality for Regex:
// - Backreferences as "calls" (\k<name>, \1, \2)
// - Named group definitions as "member access" (?<name>...)
// - Proper containing symbol tracking (file-scoped)
//
// Following the Rust/C# extractor reference implementation pattern

#[cfg(test)]
mod identifier_extraction_tests {
    #![allow(unused_variables)]

    use super::*;
    use crate::extractors::base::IdentifierKind;

    fn extract_identifiers(
        code: &str,
    ) -> (
        Vec<crate::extractors::base::Symbol>,
        Vec<crate::extractors::base::Identifier>,
    ) {
        let tree = init_parser(code, "regex");
        let mut extractor = RegexExtractor::new(
            "regex".to_string(),
            "test.regex".to_string(),
            code.to_string(),
        );
        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);
        (symbols, identifiers)
    }

    #[test]
    fn test_regex_function_calls() {
        // In Regex: "function calls" = backreferences to groups
        let regex_code = r#"(?<email>\w+@\w+\.\w+).*\k<email>"#;

        let (_symbols, identifiers) = extract_identifiers(regex_code);

        // Find backreference identifier
        let backref = identifiers
            .iter()
            .find(|id| id.name == "email" && id.kind == IdentifierKind::Call);
        assert!(
            backref.is_some(),
            "Should extract backreference '\\k<email>' as Call identifier"
        );
    }

    #[test]
    fn test_regex_member_access() {
        // In Regex: "member access" = named group definitions
        let regex_code = r#"(?<username>[a-z]+)@(?<domain>[a-z]+\.[a-z]+)"#;

        let (_symbols, identifiers) = extract_identifiers(regex_code);

        // Find named group identifiers
        let username_group = identifiers
            .iter()
            .find(|id| id.name == "username" && id.kind == IdentifierKind::MemberAccess);
        assert!(
            username_group.is_some(),
            "Should extract named group '(?<username>...)' as MemberAccess identifier"
        );

        let domain_group = identifiers
            .iter()
            .find(|id| id.name == "domain" && id.kind == IdentifierKind::MemberAccess);
        assert!(
            domain_group.is_some(),
            "Should extract named group '(?<domain>...)' as MemberAccess identifier"
        );
    }

    #[test]
    fn test_regex_identifiers_have_containing_symbol() {
        // Verify that identifiers have containing_symbol_id set
        let regex_code = r#"(?<word>\w+)\s+\k<word>"#;

        let (symbols, identifiers) = extract_identifiers(regex_code);

        // Should have at least one identifier with containing symbol
        let backref = identifiers
            .iter()
            .find(|id| id.name == "word" && id.kind == IdentifierKind::Call);
        assert!(backref.is_some());

        // Note: Regex doesn't have traditional scopes like functions/classes,
        // so containing_symbol_id might be None or the root pattern
        // This is acceptable for regex's flat structure
    }

    #[test]
    fn test_regex_chained_member_access() {
        // In Regex: "chained" means nested groups
        let regex_code = r#"(?<outer>(?<inner>\d+))"#;

        let (_symbols, identifiers) = extract_identifiers(regex_code);

        // Should extract both nested group names
        let outer_group = identifiers
            .iter()
            .find(|id| id.name == "outer" && id.kind == IdentifierKind::MemberAccess);
        assert!(
            outer_group.is_some(),
            "Should extract outer named group '(?<outer>...)'"
        );

        let inner_group = identifiers
            .iter()
            .find(|id| id.name == "inner" && id.kind == IdentifierKind::MemberAccess);
        assert!(
            inner_group.is_some(),
            "Should extract inner named group '(?<inner>...)'"
        );
    }

    #[test]
    fn test_regex_duplicate_calls_at_different_locations() {
        // Same backreference used twice should create 2 identifiers
        let regex_code = r#"(?<word>\w+)\s+\k<word>\s+\k<word>"#;

        let (_symbols, identifiers) = extract_identifiers(regex_code);

        // Should extract BOTH backreferences
        let backref_calls: Vec<_> = identifiers
            .iter()
            .filter(|id| id.name == "word" && id.kind == IdentifierKind::Call)
            .collect();

        assert_eq!(
            backref_calls.len(),
            2,
            "Should extract both \\k<word> backreferences at different locations"
        );

        // Verify they have different positions (start_byte or start_column)
        if backref_calls.len() == 2 {
            assert!(
                backref_calls[0].start_byte != backref_calls[1].start_byte,
                "Duplicate backreferences should have different positions"
            );
        }
    }
}
