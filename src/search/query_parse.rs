//! Query parsing for the search reranker.
//!
//! Plan task C.1: classify a raw query into [`QueryIntent::Free`],
//! [`QueryIntent::Symbol`], or [`QueryIntent::Test`] and split out the
//! `target_terms` that downstream scoring should match against.
//!
//! Pure function, no I/O. See C.2 of
//! `docs/plans/2026-05-15-daemon-split-and-search-reranker-design.md`.

use crate::extractors::SymbolKind;

/// What the user is asking for, derived from a leading keyword token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryIntent {
    /// No leading-keyword intent detected; full-text relevance only.
    Free,
    /// Leading symbol-kind keyword (`function`, `class`, ...).
    Symbol(SymbolKind),
    /// Leading `test` keyword with ≥2 trailing terms.
    Test,
}

/// Parsed form of a raw query string, ready for the reranker.
#[derive(Debug, Clone)]
pub struct ParsedQuery {
    /// The original query, unchanged.
    pub raw: String,
    /// All tokens, lowercased and whitespace-split.
    pub terms: Vec<String>,
    /// Tokens after any leading intent keyword is stripped. For
    /// `QueryIntent::Free` this equals `terms`.
    pub target_terms: Vec<String>,
    /// Detected intent.
    pub intent: QueryIntent,
}

/// Symbol-kind keywords accepted as a leading intent token.
///
/// Matches the design doc subset; not every [`SymbolKind`] variant has a
/// natural one-word query form.
const SYMBOL_KIND_KEYWORDS: &[&str] = &[
    "function",
    "method",
    "class",
    "struct",
    "trait",
    "interface",
    "type",
    "enum",
];

/// Parse a raw search query into a [`ParsedQuery`].
///
/// Tokenization is whitespace-split + lowercase. Intent detection requires
/// **at least 3 total tokens** so that a leading `test` or `function`
/// keyword is unambiguously a modifier and not a search term in its own
/// right (e.g. searching for a symbol literally named `function` should
/// still work as a free-text query).
pub fn parse_query(raw: &str) -> ParsedQuery {
    let terms: Vec<String> = raw
        .split_whitespace()
        .map(|s| s.to_lowercase())
        .collect();

    if terms.len() >= 3 {
        let first = terms[0].as_str();
        if first == "test" {
            let target_terms = terms[1..].to_vec();
            return ParsedQuery {
                raw: raw.to_string(),
                terms,
                target_terms,
                intent: QueryIntent::Test,
            };
        }
        if SYMBOL_KIND_KEYWORDS.contains(&first) {
            if let Some(kind) = SymbolKind::try_from_string(first) {
                let target_terms = terms[1..].to_vec();
                return ParsedQuery {
                    raw: raw.to_string(),
                    terms,
                    target_terms,
                    intent: QueryIntent::Symbol(kind),
                };
            }
        }
    }

    let target_terms = terms.clone();
    ParsedQuery {
        raw: raw.to_string(),
        terms,
        target_terms,
        intent: QueryIntent::Free,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ----- Free intent (no keyword / too short to be intent) -----

    #[test]
    fn test_parse_query_empty_is_free_with_no_terms() {
        let q = parse_query("");
        assert_eq!(q.intent, QueryIntent::Free);
        assert!(q.terms.is_empty());
        assert!(q.target_terms.is_empty());
    }

    #[test]
    fn test_parse_query_single_word_is_free() {
        let q = parse_query("hello");
        assert_eq!(q.intent, QueryIntent::Free);
        assert_eq!(q.terms, vec!["hello".to_string()]);
        assert_eq!(q.target_terms, vec!["hello".to_string()]);
    }

    #[test]
    fn test_parse_query_two_words_is_free() {
        // <3 terms → intent detection does not fire even for keywords.
        let q = parse_query("hello world");
        assert_eq!(q.intent, QueryIntent::Free);
        assert_eq!(q.target_terms, vec!["hello", "world"]);
    }

    #[test]
    fn test_parse_query_three_words_no_keyword_is_free() {
        let q = parse_query("hello world foo");
        assert_eq!(q.intent, QueryIntent::Free);
        assert_eq!(q.target_terms, vec!["hello", "world", "foo"]);
    }

    // ----- Test intent guardrails -----

    #[test]
    fn test_parse_query_test_alone_is_free() {
        // Term count <3 → "test" is just a search term.
        let q = parse_query("test");
        assert_eq!(q.intent, QueryIntent::Free);
        assert_eq!(q.target_terms, vec!["test"]);
    }

    #[test]
    fn test_parse_query_test_one_arg_is_free() {
        // Still <3 terms; falls back to Free per plan edge case.
        let q = parse_query("test foo");
        assert_eq!(q.intent, QueryIntent::Free);
        assert_eq!(q.target_terms, vec!["test", "foo"]);
    }

    #[test]
    fn test_parse_query_test_two_args_is_test_intent() {
        let q = parse_query("test foo bar");
        assert_eq!(q.intent, QueryIntent::Test);
        assert_eq!(q.target_terms, vec!["foo", "bar"]);
        // Original `terms` keeps the leading keyword.
        assert_eq!(q.terms, vec!["test", "foo", "bar"]);
    }

    #[test]
    fn test_parse_query_test_is_case_insensitive() {
        let q = parse_query("TEST Foo BAR");
        assert_eq!(q.intent, QueryIntent::Test);
        assert_eq!(q.target_terms, vec!["foo", "bar"]);
    }

    // ----- Symbol intent for each accepted keyword -----

    #[test]
    fn test_parse_query_function_keyword() {
        let q = parse_query("function fooBar baz");
        assert_eq!(q.intent, QueryIntent::Symbol(SymbolKind::Function));
        assert_eq!(q.target_terms, vec!["foobar", "baz"]);
    }

    #[test]
    fn test_parse_query_method_keyword() {
        let q = parse_query("method foo bar");
        assert_eq!(q.intent, QueryIntent::Symbol(SymbolKind::Method));
        assert_eq!(q.target_terms, vec!["foo", "bar"]);
    }

    #[test]
    fn test_parse_query_class_keyword() {
        let q = parse_query("class FooBar baz");
        assert_eq!(q.intent, QueryIntent::Symbol(SymbolKind::Class));
        assert_eq!(q.target_terms, vec!["foobar", "baz"]);
    }

    #[test]
    fn test_parse_query_struct_keyword() {
        let q = parse_query("struct foo bar");
        assert_eq!(q.intent, QueryIntent::Symbol(SymbolKind::Struct));
    }

    #[test]
    fn test_parse_query_trait_keyword() {
        let q = parse_query("trait foo bar");
        assert_eq!(q.intent, QueryIntent::Symbol(SymbolKind::Trait));
    }

    #[test]
    fn test_parse_query_interface_keyword() {
        let q = parse_query("interface foo bar");
        assert_eq!(q.intent, QueryIntent::Symbol(SymbolKind::Interface));
    }

    #[test]
    fn test_parse_query_type_keyword() {
        let q = parse_query("type foo bar");
        assert_eq!(q.intent, QueryIntent::Symbol(SymbolKind::Type));
    }

    #[test]
    fn test_parse_query_enum_keyword() {
        let q = parse_query("enum foo bar");
        assert_eq!(q.intent, QueryIntent::Symbol(SymbolKind::Enum));
    }

    #[test]
    fn test_parse_query_symbol_keyword_is_case_insensitive() {
        let q = parse_query("FUNCTION Foo Bar");
        assert_eq!(q.intent, QueryIntent::Symbol(SymbolKind::Function));
        assert_eq!(q.target_terms, vec!["foo", "bar"]);
    }

    // ----- Edge cases -----

    #[test]
    fn test_parse_query_leading_and_trailing_whitespace() {
        let q = parse_query("   function foo bar   ");
        assert_eq!(q.intent, QueryIntent::Symbol(SymbolKind::Function));
        assert_eq!(q.target_terms, vec!["foo", "bar"]);
    }

    #[test]
    fn test_parse_query_internal_whitespace_collapses() {
        // Tabs and multiple spaces collapse via split_whitespace.
        let q = parse_query("function\tfoo  bar");
        assert_eq!(q.intent, QueryIntent::Symbol(SymbolKind::Function));
        assert_eq!(q.target_terms, vec!["foo", "bar"]);
    }

    #[test]
    fn test_parse_query_short_keyword_below_threshold_is_free() {
        // "function foo" is 2 tokens; intent threshold is 3.
        let q = parse_query("function foo");
        assert_eq!(q.intent, QueryIntent::Free);
        assert_eq!(q.target_terms, vec!["function", "foo"]);
    }

    #[test]
    fn test_parse_query_non_keyword_first_token_is_free() {
        // "fn" is a Rust shorthand but not in our keyword set.
        let q = parse_query("fn foo bar");
        assert_eq!(q.intent, QueryIntent::Free);
        assert_eq!(q.target_terms, vec!["fn", "foo", "bar"]);
    }

    #[test]
    fn test_parse_query_hyphenated_tokens_preserved() {
        // split_whitespace doesn't split on hyphens; the reranker decides
        // how to handle compound tokens downstream.
        let q = parse_query("hello-world test code");
        assert_eq!(q.intent, QueryIntent::Free);
        assert_eq!(q.target_terms, vec!["hello-world", "test", "code"]);
    }

    #[test]
    fn test_parse_query_test_three_test_tokens() {
        // Pathological but well-defined: "test test test" → Test intent,
        // target_terms = ["test", "test"]. The reranker treats the
        // remaining literal "test" tokens as search terms.
        let q = parse_query("test test test");
        assert_eq!(q.intent, QueryIntent::Test);
        assert_eq!(q.target_terms, vec!["test", "test"]);
    }

    #[test]
    fn test_parse_query_raw_field_preserves_original() {
        let raw = "  FUNCTION Foo Bar  ";
        let q = parse_query(raw);
        assert_eq!(q.raw, raw);
        // Lowercased + trimmed for terms but raw is untouched.
        assert_eq!(q.terms, vec!["function", "foo", "bar"]);
    }
}
