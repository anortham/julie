// Advanced Regex Feature Tests
//
// Tests for advanced regex features that were previously untested:
// - Atomic groups (?>pattern) - prevents backtracking
// - Inline comments (?# comment)
// - Extended mode comments # comment
// - Literal characters (escaped and unescaped)

use crate::extractors::base::SymbolKind;
use crate::extractors::regex::RegexExtractor;
use crate::tests::test_utils::init_parser;
use std::path::PathBuf;

#[cfg(test)]
mod atomic_group_tests {
    use super::*;

    #[test]
    fn test_extract_atomic_group_basic() {
        // NOTE: Tree-sitter regex parser doesn't support atomic groups (?>...)
        // They're parsed as ERROR nodes, not as "atomic_group" nodes
        // This test validates the current behavior (pattern extraction despite ERROR)

        let workspace_root = PathBuf::from("/tmp/test");
        let pattern = r"(?>abc+)def";

        let tree = init_parser(pattern, "regex");
        let mut extractor = RegexExtractor::new(
            "regex".to_string(),
            "test.regex".to_string(),
            pattern.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        // Should still extract the pattern even though (?> is an ERROR
        assert!(!symbols.is_empty(), "Should extract symbols from pattern");

        // Verify the pattern is captured
        let has_atomic_syntax = symbols.iter().any(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains("(?>"))
        });

        assert!(
            has_atomic_syntax,
            "Should preserve (?> syntax in extracted symbols"
        );
    }

    #[test]
    fn test_extract_atomic_group_complex() {
        let workspace_root = PathBuf::from("/tmp/test");
        // Atomic group prevents backtracking - common use case
        let pattern = r"(?>foo|food)bar";

        let tree = init_parser(pattern, "regex");
        let mut extractor = RegexExtractor::new(
            "regex".to_string(),
            "test.regex".to_string(),
            pattern.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        let atomic_group = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains("(?>"))
        });

        assert!(
            atomic_group.is_some(),
            "Should extract atomic group with alternation"
        );
    }

    #[test]
    fn test_extract_nested_atomic_group() {
        let workspace_root = PathBuf::from("/tmp/test");
        // Nested atomic group
        let pattern = r"(?>a(?>bc)+)";

        let tree = init_parser(pattern, "regex");
        let mut extractor = RegexExtractor::new(
            "regex".to_string(),
            "test.regex".to_string(),
            pattern.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        // Should find at least one atomic group
        let atomic_groups: Vec<_> = symbols
            .iter()
            .filter(|s| {
                s.signature
                    .as_ref()
                    .map_or(false, |sig| sig.contains("(?>"))
            })
            .collect();

        assert!(
            !atomic_groups.is_empty(),
            "Should extract nested atomic groups"
        );
    }
}

#[cfg(test)]
mod comment_tests {
    use super::*;

    #[test]
    fn test_extract_inline_comment() {
        // NOTE: Tree-sitter regex parser doesn't support inline comments (?# ...)
        // They're parsed as ERROR nodes + individual pattern characters
        // This test validates the current behavior (pattern extraction despite ERROR)

        let workspace_root = PathBuf::from("/tmp/test");
        // Inline comment syntax: (?# comment text)
        let pattern = r"[a-z]+(?# matches lowercase letters)\d+";

        let tree = init_parser(pattern, "regex");
        let mut extractor = RegexExtractor::new(
            "regex".to_string(),
            "test.regex".to_string(),
            pattern.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        // Should still extract symbols from the pattern
        assert!(
            !symbols.is_empty(),
            "Should extract symbols from pattern with comment syntax"
        );

        // Character class should be extracted
        let char_class = symbols.iter().find(|s| s.kind == SymbolKind::Class);
        assert!(char_class.is_some(), "Should extract character class [a-z]");
    }

    #[test]
    fn test_extract_multiple_inline_comments() {
        // NOTE: Tree-sitter regex parser doesn't support inline comments
        // This test validates graceful handling of multiple (?# ...) patterns

        let workspace_root = PathBuf::from("/tmp/test");
        let pattern = r"(?# first comment)[a-z]+(?# second comment)\d+";

        let tree = init_parser(pattern, "regex");
        let mut extractor = RegexExtractor::new(
            "regex".to_string(),
            "test.regex".to_string(),
            pattern.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        // Should extract symbols despite comment syntax being ERROR nodes
        assert!(
            !symbols.is_empty(),
            "Should extract symbols from pattern with comment syntax"
        );
    }

    #[test]
    fn test_extract_extended_mode_comment() {
        let workspace_root = PathBuf::from("/tmp/test");
        // Extended mode uses # for comments (usually with (?x) flag)
        let pattern = "# This is a comment\n[a-z]+";

        let tree = init_parser(pattern, "regex");
        let mut extractor = RegexExtractor::new(
            "regex".to_string(),
            "test.regex".to_string(),
            pattern.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        // Extended mode comments might be parsed as comments if tree-sitter supports it
        // This test documents the behavior
        assert!(
            !symbols.is_empty(),
            "Should extract symbols from pattern with extended comment"
        );
    }
}

#[cfg(test)]
mod literal_tests {
    use super::*;

    #[test]
    fn test_extract_simple_literals() {
        let workspace_root = PathBuf::from("/tmp/test");
        let pattern = "hello";

        let tree = init_parser(pattern, "regex");
        let mut extractor = RegexExtractor::new(
            "regex".to_string(),
            "test.regex".to_string(),
            pattern.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        // Should extract literal characters
        assert!(
            !symbols.is_empty(),
            "Should extract symbols from literal pattern"
        );

        // Check for literal metadata if available
        let has_literal = symbols.iter().any(|s| {
            s.metadata
                .as_ref()
                .and_then(|m| m.get("type"))
                .and_then(|v| v.as_str())
                == Some("literal")
        });

        // Literals may or may not be extracted depending on tree-sitter parsing
        // This test documents the behavior
        if has_literal {
            let literal = symbols
                .iter()
                .find(|s| {
                    s.metadata
                        .as_ref()
                        .and_then(|m| m.get("type"))
                        .and_then(|v| v.as_str())
                        == Some("literal")
                })
                .unwrap();
            assert_eq!(literal.kind, SymbolKind::Variable);
        }
    }

    #[test]
    fn test_extract_escaped_literals() {
        let workspace_root = PathBuf::from("/tmp/test");
        // Escaped special characters
        let pattern = r"\.\*\+\?";

        let tree = init_parser(pattern, "regex");
        let mut extractor = RegexExtractor::new(
            "regex".to_string(),
            "test.regex".to_string(),
            pattern.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        assert!(
            !symbols.is_empty(),
            "Should extract symbols from escaped literals"
        );

        // Check for escaped literal detection
        let has_escaped = symbols.iter().any(|s| {
            s.metadata
                .as_ref()
                .and_then(|m| m.get("escaped"))
                .and_then(|v| v.as_str())
                == Some("true")
        });

        // If we detect escaped literals, verify metadata
        if has_escaped {
            let escaped_literal = symbols
                .iter()
                .find(|s| {
                    s.metadata
                        .as_ref()
                        .and_then(|m| m.get("escaped"))
                        .and_then(|v| v.as_str())
                        == Some("true")
                })
                .unwrap();

            assert!(
                escaped_literal.signature.is_some(),
                "Escaped literal should have signature"
            );
        }
    }

    #[test]
    fn test_extract_unicode_literals() {
        let workspace_root = PathBuf::from("/tmp/test");
        // Unicode escape sequences
        let pattern = r"\u{1F600}\x41\101";

        let tree = init_parser(pattern, "regex");
        let mut extractor = RegexExtractor::new(
            "regex".to_string(),
            "test.regex".to_string(),
            pattern.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        assert!(
            !symbols.is_empty(),
            "Should extract symbols from Unicode literals"
        );
    }

    #[test]
    fn test_extract_mixed_literals_and_metacharacters() {
        let workspace_root = PathBuf::from("/tmp/test");
        // Mix of literals and metacharacters
        let pattern = r"hello\s+world\.txt";

        let tree = init_parser(pattern, "regex");
        let mut extractor = RegexExtractor::new(
            "regex".to_string(),
            "test.regex".to_string(),
            pattern.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        assert!(
            !symbols.is_empty(),
            "Should extract symbols from mixed pattern"
        );

        // Pattern should be extracted as a whole
        let has_pattern = symbols.iter().any(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains("hello") && sig.contains("\\s"))
        });

        assert!(
            has_pattern,
            "Should extract pattern with literals and metacharacters"
        );
    }
}

#[cfg(test)]
mod comprehensive_advanced_tests {
    use super::*;

    #[test]
    fn test_extract_complex_pattern_with_all_features() {
        let workspace_root = PathBuf::from("/tmp/test");
        // Complex pattern combining atomic groups, comments, and various features
        let pattern = r"(?# Email validator)(?>[\w\.\-]+)@(?>[\w\-]+\.\w{2,})";

        let tree = init_parser(pattern, "regex");
        let mut extractor = RegexExtractor::new(
            "regex".to_string(),
            "test.regex".to_string(),
            pattern.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        assert!(
            !symbols.is_empty(),
            "Should extract symbols from complex pattern"
        );

        // Should have various symbol types
        let symbol_types: Vec<SymbolKind> = symbols.iter().map(|s| s.kind.clone()).collect();

        assert!(
            symbol_types.len() > 1,
            "Should extract multiple symbol types"
        );
    }
}
