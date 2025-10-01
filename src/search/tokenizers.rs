use std::collections::HashSet;
use tantivy::tokenizer::{Token, TokenStream, Tokenizer};

/// Tokenizer optimized for code identifiers and file paths.
///
/// Produces the original token plus camelCase and snake_case splits so that
/// multi-word queries like "user service" can match identifiers such as
/// `UserService`.
#[derive(Debug, Clone)]
pub struct CodeTokenizer {
    preserve_original: bool,
}

impl Default for CodeTokenizer {
    fn default() -> Self {
        Self {
            preserve_original: true,
        }
    }
}

impl CodeTokenizer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn preserve_original(mut self, preserve: bool) -> Self {
        self.preserve_original = preserve;
        self
    }

    fn build_tokens(&self, text: &str) -> Vec<Token> {
        let mut tokens = Vec::new();
        let mut position: usize = 0;

        for (start, end) in collect_word_bounds(text) {
            if start >= end {
                continue;
            }

            let original = &text[start..end];
            if original.is_empty() {
                continue;
            }

            let mut seen = HashSet::new();

            if self.preserve_original {
                tokens.push(new_token(original.to_string(), start, end, position));
                seen.insert(original.to_ascii_lowercase());
                position += 1;
            }

            for variant in split_identifier(original) {
                if variant.is_empty() {
                    continue;
                }

                let key = variant.to_ascii_lowercase();
                if !seen.insert(key) {
                    continue;
                }

                tokens.push(new_token(variant, start, end, position));
                position += 1;
            }
        }

        tokens
    }
}

fn new_token(text: String, offset_from: usize, offset_to: usize, position: usize) -> Token {
    Token {
        text,
        offset_from,
        offset_to,
        position,
        ..Default::default()
    }
}

fn collect_word_bounds(text: &str) -> Vec<(usize, usize)> {
    let mut bounds = Vec::new();
    let mut start: Option<usize> = None;

    for (idx, ch) in text.char_indices() {
        if is_word_char(ch) {
            if start.is_none() {
                start = Some(idx);
            }
        } else if let Some(s) = start.take() {
            bounds.push((s, idx));
        }
    }

    if let Some(s) = start {
        bounds.push((s, text.len()));
    }

    bounds
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn split_identifier(identifier: &str) -> Vec<String> {
    let mut parts = Vec::new();

    for segment in identifier.split('_') {
        if segment.is_empty() {
            continue;
        }

        let camel_parts = split_camel_case(segment);
        if camel_parts.is_empty() {
            parts.push(segment.to_string());
        } else {
            parts.extend(camel_parts);
        }
    }

    if parts.is_empty() && !identifier.is_empty() {
        parts.push(identifier.to_string());
    }

    parts
}

fn split_camel_case(segment: &str) -> Vec<String> {
    let mut bounds = Vec::new();
    let chars: Vec<(usize, char)> = segment.char_indices().collect();

    if chars.is_empty() {
        return Vec::new();
    }

    bounds.push(0);

    for i in 1..chars.len() {
        let (_, prev) = chars[i - 1];
        let (idx, curr) = chars[i];
        let next = chars.get(i + 1).map(|(_, c)| *c);

        let is_boundary = (prev.is_lowercase() && curr.is_uppercase())
            || (prev.is_numeric() && curr.is_alphabetic())
            || (prev.is_alphabetic() && curr.is_numeric())
            || (prev.is_uppercase()
                && curr.is_uppercase()
                && next.map(|n| n.is_lowercase()).unwrap_or(false));

        if is_boundary {
            bounds.push(idx);
        }
    }

    bounds.push(segment.len());

    let mut parts = Vec::new();
    for window in bounds.windows(2) {
        let start = window[0];
        let end = window[1];
        if start < end {
            parts.push(segment[start..end].to_string());
        }
    }

    if parts.is_empty() {
        parts.push(segment.to_string());
    }

    parts
}

#[derive(Debug, Clone)]
pub struct CodeTokenStream {
    tokens: Vec<Token>,
    index: usize,
}

impl TokenStream for CodeTokenStream {
    fn advance(&mut self) -> bool {
        if self.index < self.tokens.len() {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn token(&self) -> &Token {
        &self.tokens[self.index - 1]
    }

    fn token_mut(&mut self) -> &mut Token {
        &mut self.tokens[self.index - 1]
    }
}

impl Tokenizer for CodeTokenizer {
    type TokenStream<'a>
        = CodeTokenStream
    where
        Self: 'a;

    fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
        let tokens = self.build_tokens(text);
        CodeTokenStream { tokens, index: 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect_tokens(tokenizer: &mut CodeTokenizer, text: &str) -> Vec<String> {
        let mut stream = tokenizer.token_stream(text);
        let mut tokens = Vec::new();
        while stream.advance() {
            tokens.push(stream.token().text.clone());
        }
        tokens
    }

    #[test]
    fn splits_camel_case_identifiers() {
        let mut tokenizer = CodeTokenizer::new();
        let tokens = collect_tokens(&mut tokenizer, "UserService");
        assert_eq!(tokens, vec!["UserService", "User", "Service"]);
    }

    #[test]
    fn splits_snake_case_identifiers() {
        let mut tokenizer = CodeTokenizer::new();
        let tokens = collect_tokens(&mut tokenizer, "extract_symbols");
        assert_eq!(tokens, vec!["extract_symbols", "extract", "symbols"]);
    }

    #[test]
    fn splits_mixed_uppercase_runs() {
        let mut tokenizer = CodeTokenizer::new();
        let tokens = collect_tokens(&mut tokenizer, "getHTTPResponse2");
        assert_eq!(
            tokens,
            vec!["getHTTPResponse2", "get", "HTTP", "Response", "2"]
        );
    }

    #[test]
    fn respects_hyphen_and_path_boundaries() {
        let mut tokenizer = CodeTokenizer::new();
        let tokens = collect_tokens(&mut tokenizer, "src/services/UserService.rs");
        assert_eq!(
            tokens,
            vec!["src", "services", "UserService", "User", "Service", "rs"]
        );
    }
}
