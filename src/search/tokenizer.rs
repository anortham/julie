//! Code-aware tokenization for search indexing.
//!
//! Provides a Tantivy-compatible tokenizer that understands code conventions:
//! - Preserves special character sequences (::, ->, ?., etc.)
//! - Splits CamelCase into component words
//! - Splits snake_case into component words
//! - Keeps original tokens for exact matching

use tantivy::tokenizer::{Token, TokenStream, Tokenizer};

use crate::search::language_config::LanguageConfigs;

/// A tokenizer designed for code that:
/// - Preserves special character sequences (::, ->, ?., etc.)
/// - Splits CamelCase into component words
/// - Splits snake_case into component words
/// - Keeps original tokens for exact matching
#[derive(Clone)]
pub struct CodeTokenizer {
    /// Patterns to preserve as single tokens (e.g., "::", "->")
    preserve_patterns: Vec<String>,
}

impl CodeTokenizer {
    pub fn new(preserve_patterns: Vec<String>) -> Self {
        let mut patterns = preserve_patterns;
        patterns.sort_by_key(|b| std::cmp::Reverse(b.len()));
        Self {
            preserve_patterns: patterns,
        }
    }

    /// Create a tokenizer with default patterns for common languages.
    pub fn with_default_patterns() -> Self {
        Self::new(vec![
            "::".to_string(),
            "->".to_string(),
            "=>".to_string(),
            "'".to_string(),
            "#[".to_string(),
            "#![".to_string(),
            "?.".to_string(),
            "??".to_string(),
            "===".to_string(),
            "!==".to_string(),
            "__".to_string(),
            "@".to_string(),
            ":=".to_string(),
            "<-".to_string(),
            "<<".to_string(),
            ">>".to_string(),
            "&&".to_string(),
            "||".to_string(),
            "<>".to_string(),
            "++".to_string(),
            "--".to_string(),
        ])
    }

    /// Create a tokenizer from Julie's language configurations.
    ///
    /// Collects all preserve_patterns from all configured languages
    /// into a single union set, sorted by length descending.
    pub fn from_language_configs(configs: &LanguageConfigs) -> Self {
        let patterns = configs.all_preserve_patterns();
        Self::new(patterns)
    }
}

impl Tokenizer for CodeTokenizer {
    type TokenStream<'a> = CodeTokenStream<'a>;

    fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
        CodeTokenStream::new(text, &self.preserve_patterns)
    }
}

pub struct CodeTokenStream<'a> {
    #[allow(dead_code)]
    text: &'a str,
    tokens: Vec<Token>,
    current: usize,
}

impl<'a> CodeTokenStream<'a> {
    fn new(text: &'a str, preserve_patterns: &[String]) -> Self {
        let tokens = tokenize_code(text, preserve_patterns);
        Self {
            text,
            tokens,
            current: 0,
        }
    }
}

impl<'a> TokenStream for CodeTokenStream<'a> {
    fn advance(&mut self) -> bool {
        if self.current < self.tokens.len() {
            self.current += 1;
            true
        } else {
            false
        }
    }

    fn token(&self) -> &Token {
        &self.tokens[self.current - 1]
    }

    fn token_mut(&mut self) -> &mut Token {
        &mut self.tokens[self.current - 1]
    }
}

fn tokenize_code(text: &str, preserve_patterns: &[String]) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut position = 0;
    let segments = extract_segments(text, preserve_patterns);

    for (segment, offset) in segments {
        if preserve_patterns.iter().any(|p| p == &segment) {
            tokens.push(Token {
                offset_from: offset,
                offset_to: offset + segment.len(),
                position,
                text: segment.to_lowercase(),
                position_length: 1,
            });
            position += 1;
            continue;
        }

        if segment.chars().all(|c| c.is_alphanumeric() || c == '_') {
            tokens.push(Token {
                offset_from: offset,
                offset_to: offset + segment.len(),
                position,
                text: segment.to_lowercase(),
                position_length: 1,
            });
            position += 1;

            // Split CamelCase/PascalCase identifiers
            if segment.chars().any(|c| c.is_uppercase())
                && segment.chars().any(|c| c.is_lowercase())
            {
                for part in split_camel_case(&segment) {
                    if part.to_lowercase() != segment.to_lowercase() {
                        tokens.push(Token {
                            offset_from: offset,
                            offset_to: offset + segment.len(),
                            position,
                            text: part.to_lowercase(),
                            position_length: 1,
                        });
                        position += 1;
                    }
                }
            }

            // Split snake_case identifiers
            if segment.contains('_') {
                for part in split_snake_case(&segment) {
                    if part.to_lowercase() != segment.to_lowercase() {
                        tokens.push(Token {
                            offset_from: offset,
                            offset_to: offset + segment.len(),
                            position,
                            text: part.to_lowercase(),
                            position_length: 1,
                        });
                        position += 1;
                    }
                }
            }
        }
    }

    tokens
}

fn extract_segments(text: &str, preserve_patterns: &[String]) -> Vec<(String, usize)> {
    let mut segments = Vec::new();
    let mut remaining = text;
    let mut offset = 0;

    while !remaining.is_empty() {
        // Check for preserved patterns first
        let mut found_pattern = false;
        for pattern in preserve_patterns {
            if remaining.starts_with(pattern.as_str()) {
                segments.push((pattern.clone(), offset));
                remaining = &remaining[pattern.len()..];
                offset += pattern.len();
                found_pattern = true;
                break;
            }
        }
        if found_pattern {
            continue;
        }

        // Skip whitespace and delimiters
        if let Some(c) = remaining.chars().next() {
            if c.is_whitespace() || "(){}[]<>,;\"'".contains(c) {
                remaining = &remaining[c.len_utf8()..];
                offset += c.len_utf8();
                continue;
            }
        }

        // Extract a word segment (until we hit whitespace, delimiter, or pattern)
        let end = remaining
            .char_indices()
            .find(|(i, c)| {
                c.is_whitespace()
                    || "(){}[]<>,;\"'".contains(*c)
                    || preserve_patterns
                        .iter()
                        .any(|p| remaining[*i..].starts_with(p.as_str()))
            })
            .map(|(i, _)| i)
            .unwrap_or(remaining.len());

        if end > 0 {
            segments.push((remaining[..end].to_string(), offset));
            remaining = &remaining[end..];
            offset += end;
        } else if !remaining.is_empty() {
            // Skip a single character we can't categorize
            let c = remaining.chars().next().unwrap();
            remaining = &remaining[c.len_utf8()..];
            offset += c.len_utf8();
        }
    }

    segments
}

/// Split a CamelCase or PascalCase identifier into words.
///
/// Handles transitions like:
/// - `UserService` -> `["User", "Service"]`
/// - `XMLParser` -> `["XML", "Parser"]`
/// - `getHTTPResponse` -> `["get", "HTTP", "Response"]`
pub fn split_camel_case(s: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = 0;
    let chars: Vec<char> = s.chars().collect();

    for i in 1..chars.len() {
        let prev = chars[i - 1];
        let curr = chars[i];
        let split_before_upper = prev.is_lowercase() && curr.is_uppercase();
        let split_acronym =
            i >= 2 && chars[i - 2].is_uppercase() && prev.is_uppercase() && curr.is_lowercase();

        if split_before_upper || split_acronym {
            let split_pos = if split_acronym { i - 1 } else { i };
            if split_pos > start {
                let byte_start: usize = chars[..start].iter().map(|c| c.len_utf8()).sum();
                let byte_end: usize = chars[..split_pos].iter().map(|c| c.len_utf8()).sum();
                result.push(&s[byte_start..byte_end]);
                start = split_pos;
            }
        }
    }

    if start < chars.len() {
        let byte_start: usize = chars[..start].iter().map(|c| c.len_utf8()).sum();
        result.push(&s[byte_start..]);
    }

    result
}

/// Split a snake_case or SCREAMING_SNAKE_CASE identifier into words.
///
/// Simply splits on `_` and filters out empty parts.
pub fn split_snake_case(s: &str) -> Vec<&str> {
    s.split('_').filter(|part| !part.is_empty()).collect()
}
