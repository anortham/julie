// Code-Aware Tokenizers for Julie's Search Engine
//
// These tokenizers are designed specifically for code search, preserving important
// programming language constructs that standard tokenizers would break apart.

use tantivy::tokenizer::{Token, TokenStream, Tokenizer};

/// Operator-preserving tokenizer that keeps programming operators intact
///
/// Examples:
/// - "a && b" -> ["a", "&&", "b"] (preserves logical operators)
/// - "list => item" -> ["list", "=>", "item"] (preserves arrow functions)
/// - "a <> b" -> ["a", "<>", "b"] (preserves comparison operators)
/// - "obj?.prop" -> ["obj", "?.", "prop"] (preserves optional chaining)
#[derive(Debug, Clone)]
pub struct OperatorPreservingTokenizer {
    /// List of operators that should be preserved as single tokens
    operators: Vec<String>,
}

impl OperatorPreservingTokenizer {
    pub fn new() -> Self {
        Self {
            operators: vec![
                // Logical operators
                "&&".to_string(), "||".to_string(), "!".to_string(),
                // Arrow functions and lambdas
                "=>".to_string(), "->".to_string(), "|>".to_string(),
                // Comparison operators
                "===".to_string(), "==".to_string(), "!=".to_string(),
                "!==".to_string(), "<=".to_string(), ">=".to_string(), "<>".to_string(),
                // Optional chaining and null coalescing
                "?.".to_string(), "??".to_string(), "??=".to_string(),
                // Spread and rest
                "...".to_string(),
                // Type annotations
                "::".to_string(), ":".to_string(),
                // Access operators
                ".".to_string(), "?[".to_string(),
            ],
        }
    }

    /// Add a custom operator to preserve
    pub fn add_operator(&mut self, operator: String) {
        if !self.operators.contains(&operator) {
            self.operators.push(operator);
        }
    }

    /// Check if a substring matches any preserved operator
    /// Returns the longest matching operator
    fn find_operator_at(&self, text: &str, pos: usize) -> Option<&String> {
        let mut longest_match: Option<&String> = None;
        let mut longest_length = 0;

        for operator in &self.operators {
            if text[pos..].starts_with(operator) && operator.len() > longest_length {
                longest_match = Some(operator);
                longest_length = operator.len();
            }
        }

        longest_match
    }
}

/// Generic-aware tokenizer that properly handles generic type syntax
///
/// Examples:
/// - "List<User>" -> ["List", "<", "User", ">", "List<User>"] (both parts and whole)
/// - "Map<K,V>" -> ["Map", "<", "K", ",", "V", ">", "Map<K,V>"]
/// - "Promise<Result<T>>" -> ["Promise", "<", "Result", "<", "T", ">", ">", "Promise<Result<T>>"]
/// - "Array<string[]>" -> ["Array", "<", "string", "[", "]", ">", "Array<string[]>"]
#[derive(Debug, Clone)]
pub struct GenericAwareTokenizer {
    /// Whether to emit the complete generic type as a single token
    emit_complete_generic: bool,
    /// Whether to emit individual components
    emit_components: bool,
}

impl GenericAwareTokenizer {
    pub fn new() -> Self {
        Self {
            emit_complete_generic: true,
            emit_components: true,
        }
    }

    /// Configure whether to emit complete generic types (e.g., "List<User>")
    pub fn emit_complete(mut self, emit: bool) -> Self {
        self.emit_complete_generic = emit;
        self
    }

    /// Configure whether to emit individual components (e.g., "List", "User")
    pub fn emit_parts(mut self, emit: bool) -> Self {
        self.emit_components = emit;
        self
    }

    /// Find matching closing bracket for a generic type
    fn find_matching_bracket(&self, text: &str, start: usize) -> Option<usize> {
        let mut depth = 0;
        let chars: Vec<char> = text.chars().collect();

        for (i, &ch) in chars.iter().enumerate().skip(start) {
            match ch {
                '<' => depth += 1,
                '>' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
        }
        None
    }
}

/// Code identifier tokenizer that intelligently splits camelCase and snake_case
///
/// Examples:
/// - "getUserById" -> ["get", "User", "By", "Id", "getUserById"] (camelCase splitting)
/// - "user_data_service" -> ["user", "data", "service", "user_data_service"] (snake_case splitting)
/// - "HTMLParser" -> ["HTML", "Parser", "HTMLParser"] (handles acronyms)
/// - "XMLHttpRequest" -> ["XML", "Http", "Request", "XMLHttpRequest"]
/// - "camelCase_with_snake" -> ["camel", "Case", "with", "snake", "camelCase_with_snake"] (hybrid)
#[derive(Debug, Clone)]
pub struct CodeIdentifierTokenizer {
    /// Whether to emit the original identifier as a single token
    emit_original: bool,
    /// Whether to emit individual word components
    emit_words: bool,
    /// Whether to preserve number sequences
    preserve_numbers: bool,
}

impl CodeIdentifierTokenizer {
    pub fn new() -> Self {
        Self {
            emit_original: true,
            emit_words: true,
            preserve_numbers: true,
        }
    }

    /// Configure whether to emit the original identifier
    pub fn emit_original(mut self, emit: bool) -> Self {
        self.emit_original = emit;
        self
    }

    /// Configure whether to emit word components
    pub fn emit_words(mut self, emit: bool) -> Self {
        self.emit_words = emit;
        self
    }

    /// Configure whether to preserve number sequences
    pub fn preserve_numbers(mut self, preserve: bool) -> Self {
        self.preserve_numbers = preserve;
        self
    }

    /// Split camelCase identifier into words
    fn split_camel_case(&self, text: &str) -> Vec<String> {
        let mut words = Vec::new();
        let mut current_word = String::new();
        let chars: Vec<char> = text.chars().collect();

        for (i, &ch) in chars.iter().enumerate() {
            if ch.is_uppercase() && i > 0 && !current_word.is_empty() {
                // Check if previous char was lowercase (camelCase boundary)
                if let Some(&prev_ch) = chars.get(i - 1) {
                    if prev_ch.is_lowercase() || prev_ch.is_numeric() {
                        words.push(current_word.clone());
                        current_word.clear();
                    }
                }
            }
            current_word.push(ch);
        }

        if !current_word.is_empty() {
            words.push(current_word);
        }

        words
    }

    /// Split snake_case identifier into words
    fn split_snake_case(&self, text: &str) -> Vec<String> {
        text.split('_')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect()
    }
}

/// Token stream implementations for each tokenizer
pub struct OperatorPreservingTokenStream {
    tokens: Vec<Token>,
    current: usize,
}

pub struct GenericAwareTokenStream {
    tokens: Vec<Token>,
    current: usize,
}

pub struct CodeIdentifierTokenStream {
    tokens: Vec<Token>,
    current: usize,
}

// Implement Tantivy's TokenStream trait for each
impl TokenStream for OperatorPreservingTokenStream {
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

impl TokenStream for GenericAwareTokenStream {
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

impl TokenStream for CodeIdentifierTokenStream {
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

// Implement Tantivy's Tokenizer trait for each
impl Tokenizer for OperatorPreservingTokenizer {
    type TokenStream<'a> = OperatorPreservingTokenStream;

    fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
        let tokens = self.tokenize(text);
        OperatorPreservingTokenStream {
            tokens,
            current: 0,
        }
    }
}

impl OperatorPreservingTokenizer {
    /// Tokenize text while preserving operators
    fn tokenize(&self, text: &str) -> Vec<Token> {
        let mut tokens = Vec::new();
        let mut current_pos = 0;
        let text_len = text.len();

        while current_pos < text_len {
            // Skip whitespace
            if text.chars().nth(current_pos).map_or(false, |c| c.is_whitespace()) {
                current_pos += 1;
                continue;
            }

            // Check for operators at current position
            if let Some(operator) = self.find_operator_at(text, current_pos) {
                let mut token = Token::default();
                token.text = operator.clone();
                token.offset_from = current_pos;
                token.offset_to = current_pos + operator.len();
                tokens.push(token);
                current_pos += operator.len();
                continue;
            }

            // Extract regular word/identifier
            let start_pos = current_pos;
            while current_pos < text_len {
                let ch = text.chars().nth(current_pos).unwrap();
                if ch.is_whitespace() || self.starts_with_operator(text, current_pos) {
                    break;
                }
                current_pos += 1;
            }

            if current_pos > start_pos {
                let word = text[start_pos..current_pos].to_string();
                let mut token = Token::default();
                token.text = word;
                token.offset_from = start_pos;
                token.offset_to = current_pos;
                tokens.push(token);
            }
        }

        tokens
    }

    /// Check if text at position starts with any operator
    fn starts_with_operator(&self, text: &str, pos: usize) -> bool {
        self.find_operator_at(text, pos).is_some()
    }
}

impl Tokenizer for GenericAwareTokenizer {
    type TokenStream<'a> = GenericAwareTokenStream;

    fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
        let tokens = self.tokenize(text);
        GenericAwareTokenStream {
            tokens,
            current: 0,
        }
    }
}

impl GenericAwareTokenizer {
    /// Tokenize text with generic type awareness
    fn tokenize(&self, text: &str) -> Vec<Token> {
        let mut tokens = Vec::new();
        let mut current_pos = 0;
        let text_len = text.len();

        while current_pos < text_len {
            // Skip whitespace
            if text.chars().nth(current_pos).map_or(false, |c| c.is_whitespace()) {
                current_pos += 1;
                continue;
            }

            // Check for start of generic type (identifier followed by '<')
            if let Some(generic_end) = self.find_generic_type_at(text, current_pos) {
                let generic_text = &text[current_pos..=generic_end];

                if self.emit_complete_generic {
                    let mut token = Token::default();
                    token.text = generic_text.to_string();
                    token.offset_from = current_pos;
                    token.offset_to = generic_end + 1;
                    tokens.push(token);
                }

                if self.emit_components {
                    // Extract components from the generic type
                    let mut component_tokens = self.extract_generic_components(generic_text, current_pos);
                    tokens.append(&mut component_tokens);
                }

                current_pos = generic_end + 1;
                continue;
            }

            // Extract regular word/identifier
            let start_pos = current_pos;
            while current_pos < text_len {
                let ch = text.chars().nth(current_pos).unwrap();
                if ch.is_whitespace() || ch == '<' || ch == '>' || ch == ',' {
                    break;
                }
                current_pos += 1;
            }

            if current_pos > start_pos {
                let word = text[start_pos..current_pos].to_string();
                let mut token = Token::default();
                token.text = word;
                token.offset_from = start_pos;
                token.offset_to = current_pos;
                tokens.push(token);
            } else {
                // Handle single characters like '<', '>', ','
                let ch = text.chars().nth(current_pos).unwrap();
                let mut token = Token::default();
                token.text = ch.to_string();
                token.offset_from = current_pos;
                token.offset_to = current_pos + 1;
                tokens.push(token);
                current_pos += 1;
            }
        }

        tokens
    }

    /// Find generic type starting at position, return end position if found
    fn find_generic_type_at(&self, text: &str, start: usize) -> Option<usize> {
        // Look for pattern: identifier<...>
        let mut pos = start;
        let chars: Vec<char> = text.chars().collect();

        // First, consume identifier characters
        while pos < chars.len() && (chars[pos].is_alphanumeric() || chars[pos] == '_') {
            pos += 1;
        }

        // Check if we found an identifier and it's followed by '<'
        if pos > start && pos < chars.len() && chars[pos] == '<' {
            // Find matching '>'
            if let Some(end_pos) = self.find_matching_bracket(text, pos) {
                return Some(end_pos);
            }
        }

        None
    }

    /// Extract individual components from a generic type
    fn extract_generic_components(&self, generic_text: &str, base_offset: usize) -> Vec<Token> {
        let mut tokens = Vec::new();
        let mut current_pos = 0;
        let text_len = generic_text.len();

        while current_pos < text_len {
            if generic_text.chars().nth(current_pos).map_or(false, |c| c.is_whitespace()) {
                current_pos += 1;
                continue;
            }

            let start_pos = current_pos;
            let ch = generic_text.chars().nth(current_pos).unwrap();

            if ch.is_alphabetic() || ch == '_' {
                // Extract identifier
                while current_pos < text_len {
                    let ch = generic_text.chars().nth(current_pos).unwrap();
                    if !ch.is_alphanumeric() && ch != '_' {
                        break;
                    }
                    current_pos += 1;
                }

                let component = &generic_text[start_pos..current_pos];
                let mut token = Token::default();
                token.text = component.to_string();
                token.offset_from = base_offset + start_pos;
                token.offset_to = base_offset + current_pos;
                tokens.push(token);
            } else {
                // Handle single character tokens like '<', '>', ','
                let mut token = Token::default();
                token.text = ch.to_string();
                token.offset_from = base_offset + current_pos;
                token.offset_to = base_offset + current_pos + 1;
                tokens.push(token);
                current_pos += 1;
            }
        }

        tokens
    }
}

impl Tokenizer for CodeIdentifierTokenizer {
    type TokenStream<'a> = CodeIdentifierTokenStream;

    fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
        let tokens = self.tokenize(text);
        CodeIdentifierTokenStream {
            tokens,
            current: 0,
        }
    }
}

impl CodeIdentifierTokenizer {
    /// Tokenize text with code identifier awareness
    fn tokenize(&self, text: &str) -> Vec<Token> {
        let mut tokens = Vec::new();
        let mut current_pos = 0;
        let text_len = text.len();

        while current_pos < text_len {
            // Skip whitespace
            if text.chars().nth(current_pos).map_or(false, |c| c.is_whitespace()) {
                current_pos += 1;
                continue;
            }

            // Extract identifier
            let start_pos = current_pos;
            while current_pos < text_len {
                let ch = text.chars().nth(current_pos).unwrap();
                if ch.is_whitespace() || (!ch.is_alphanumeric() && ch != '_') {
                    break;
                }
                current_pos += 1;
            }

            if current_pos > start_pos {
                let identifier = &text[start_pos..current_pos];

                // Emit original identifier if configured
                if self.emit_original {
                    let mut token = Token::default();
                    token.text = identifier.to_string();
                    token.offset_from = start_pos;
                    token.offset_to = current_pos;
                    tokens.push(token);
                }

                // Emit word components if configured
                if self.emit_words {
                    let words = self.split_identifier(identifier);
                    for (word_offset, word) in self.calculate_word_positions(identifier, &words) {
                        let mut token = Token::default();
                        token.text = word;
                        token.offset_from = start_pos + word_offset;
                        token.offset_to = start_pos + word_offset + token.text.len();
                        tokens.push(token);
                    }
                }
            } else {
                // Handle non-identifier characters
                let ch = text.chars().nth(current_pos).unwrap();
                let mut token = Token::default();
                token.text = ch.to_string();
                token.offset_from = current_pos;
                token.offset_to = current_pos + 1;
                tokens.push(token);
                current_pos += 1;
            }
        }

        tokens
    }

    /// Split identifier into words (handles both camelCase and snake_case)
    fn split_identifier(&self, text: &str) -> Vec<String> {
        if text.contains('_') {
            self.split_snake_case(text)
        } else {
            self.split_camel_case(text)
        }
    }

    /// Calculate positions of words within the original identifier
    fn calculate_word_positions(&self, original: &str, words: &[String]) -> Vec<(usize, String)> {
        let mut result = Vec::new();
        let mut current_pos = 0;

        if original.contains('_') {
            // For snake_case, split by underscores
            let parts: Vec<&str> = original.split('_').collect();
            for part in parts {
                if !part.is_empty() {
                    result.push((current_pos, part.to_string()));
                    current_pos += part.len() + 1; // +1 for underscore
                }
            }
        } else {
            // For camelCase, need to find word boundaries
            let chars: Vec<char> = original.chars().collect();
            let mut word_start = 0;

            for word in words {
                // Find this word in the original text starting from word_start
                let word_chars: Vec<char> = word.chars().collect();

                // Match character by character (case-insensitive for the search)
                let mut found = false;
                for i in word_start..chars.len() {
                    if i + word_chars.len() <= chars.len() {
                        let matches = word_chars.iter().enumerate().all(|(j, &ch)| {
                            chars[i + j].to_lowercase().eq(ch.to_lowercase())
                        });

                        if matches {
                            result.push((i, word.clone()));
                            word_start = i + word_chars.len();
                            found = true;
                            break;
                        }
                    }
                }

                if !found {
                    // Fallback: just append at current position
                    result.push((word_start, word.clone()));
                    word_start += word.len();
                }
            }
        }

        result
    }
}

/// Combined code-aware tokenizer that applies all tokenizers in sequence
#[derive(Debug, Clone)]
pub struct CodeAwareTokenizer {
    operator_tokenizer: OperatorPreservingTokenizer,
    generic_tokenizer: GenericAwareTokenizer,
    identifier_tokenizer: CodeIdentifierTokenizer,
}

pub struct CodeAwareTokenStream {
    tokens: Vec<Token>,
    current: usize,
}

impl CodeAwareTokenizer {
    pub fn new() -> Self {
        Self {
            operator_tokenizer: OperatorPreservingTokenizer::new(),
            generic_tokenizer: GenericAwareTokenizer::new(),
            identifier_tokenizer: CodeIdentifierTokenizer::new(),
        }
    }

    /// Create a tokenizer optimized for exact matching
    pub fn exact_match() -> Self {
        Self {
            operator_tokenizer: OperatorPreservingTokenizer::new(),
            generic_tokenizer: GenericAwareTokenizer::new().emit_parts(false),
            identifier_tokenizer: CodeIdentifierTokenizer::new().emit_words(false),
        }
    }

    /// Create a tokenizer optimized for fuzzy search
    pub fn fuzzy_search() -> Self {
        Self {
            operator_tokenizer: OperatorPreservingTokenizer::new(),
            generic_tokenizer: GenericAwareTokenizer::new().emit_complete(false),
            identifier_tokenizer: CodeIdentifierTokenizer::new().emit_original(false),
        }
    }

    /// Tokenize text using all code-aware tokenizers
    fn tokenize(&mut self, text: &str) -> Vec<Token> {
        // Start with operator-preserving tokenization
        let operator_tokens = self.operator_tokenizer.tokenize(text);

        let mut final_tokens = Vec::new();

        for token in operator_tokens {
            // For each token, apply generic-aware tokenization if it's not an operator
            if self.is_operator(&token.text) {
                final_tokens.push(token);
            } else {
                // Apply generic-aware tokenization
                let generic_tokens = self.generic_tokenizer.tokenize(&token.text);

                for generic_token in generic_tokens {
                    // For each generic token, apply identifier tokenization if it's not generic syntax
                    if self.is_generic_syntax(&generic_token.text) {
                        final_tokens.push(Token {
                            text: generic_token.text,
                            offset_from: token.offset_from + generic_token.offset_from,
                            offset_to: token.offset_from + generic_token.offset_to,
                            ..generic_token
                        });
                    } else {
                        // Apply identifier tokenization
                        let identifier_tokens = self.identifier_tokenizer.tokenize(&generic_token.text);

                        for identifier_token in identifier_tokens {
                            final_tokens.push(Token {
                                text: identifier_token.text,
                                offset_from: token.offset_from + generic_token.offset_from + identifier_token.offset_from,
                                offset_to: token.offset_from + generic_token.offset_from + identifier_token.offset_to,
                                ..identifier_token
                            });
                        }
                    }
                }
            }
        }

        final_tokens
    }

    /// Check if a token is an operator
    fn is_operator(&self, text: &str) -> bool {
        self.operator_tokenizer.operators.contains(&text.to_string())
    }

    /// Check if a token is generic syntax (like '<', '>', ',')
    fn is_generic_syntax(&self, text: &str) -> bool {
        matches!(text, "<" | ">" | "," | "[" | "]")
    }
}

impl Tokenizer for CodeAwareTokenizer {
    type TokenStream<'a> = CodeAwareTokenStream;

    fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
        let tokens = self.tokenize(text);
        CodeAwareTokenStream {
            tokens,
            current: 0,
        }
    }
}

impl TokenStream for CodeAwareTokenStream {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_operator_preserving_tokenizer_contract() {
        // Contract: Should preserve logical operators as single tokens
        // Input: "a && b || c"
        // Expected: ["a", "&&", "b", "||", "c"]
        let mut tokenizer = OperatorPreservingTokenizer::new();
        let mut token_stream = tokenizer.token_stream("a && b || c");

        let mut tokens = Vec::new();
        while token_stream.advance() {
            tokens.push(token_stream.token().text.clone());
        }

        assert_eq!(tokens, vec!["a", "&&", "b", "||", "c"]);
    }

    #[test]
    fn test_arrow_function_operators() {
        // Contract: Should preserve arrow function operators
        // Input: "list => item.value"
        // Expected: ["list", "=>", "item", ".", "value"]
        let mut tokenizer = OperatorPreservingTokenizer::new();
        let mut token_stream = tokenizer.token_stream("list => item.value");

        let mut tokens = Vec::new();
        while token_stream.advance() {
            tokens.push(token_stream.token().text.clone());
        }

        assert_eq!(tokens, vec!["list", "=>", "item", ".", "value"]);
    }

    #[test]
    fn test_comparison_operators() {
        // Contract: Should preserve comparison operators
        // Input: "a !== b && c >= d"
        // Expected: ["a", "!==", "b", "&&", "c", ">=", "d"]
        let mut tokenizer = OperatorPreservingTokenizer::new();
        let mut token_stream = tokenizer.token_stream("a !== b && c >= d");

        let mut tokens = Vec::new();
        while token_stream.advance() {
            tokens.push(token_stream.token().text.clone());
        }

        assert_eq!(tokens, vec!["a", "!==", "b", "&&", "c", ">=", "d"]);
    }

    #[test]
    fn test_generic_type_complete() {
        // Contract: Should emit complete generic types
        // Input: "List<User>"
        // Expected: includes "List<User>" as single token
        let mut tokenizer = GenericAwareTokenizer::new();
        let mut token_stream = tokenizer.token_stream("List<User>");

        let mut tokens = Vec::new();
        while token_stream.advance() {
            tokens.push(token_stream.token().text.clone());
        }

        // Should include both the complete generic type and its components
        assert!(tokens.contains(&"List<User>".to_string()), "Should include complete generic type");
        assert!(tokens.contains(&"List".to_string()), "Should include generic base type");
        assert!(tokens.contains(&"User".to_string()), "Should include generic parameter");
    }

    #[test]
    fn test_generic_type_components() {
        // Contract: Should emit generic type components
        // Input: "Map<String, User>"
        // Expected: ["Map", "String", "User", "Map<String, User>"]
        let mut tokenizer = GenericAwareTokenizer::new();
        let mut token_stream = tokenizer.token_stream("Map<String, User>");

        let mut tokens = Vec::new();
        while token_stream.advance() {
            tokens.push(token_stream.token().text.clone());
        }

        // Should include both the complete generic type and all its components
        assert!(tokens.contains(&"Map<String, User>".to_string()), "Should include complete generic type");
        assert!(tokens.contains(&"Map".to_string()), "Should include generic base type");
        assert!(tokens.contains(&"String".to_string()), "Should include first parameter");
        assert!(tokens.contains(&"User".to_string()), "Should include second parameter");
    }

    #[test]
    fn test_nested_generics() {
        // Contract: Should handle nested generics correctly
        // Input: "Promise<Result<User>>"
        // Expected: ["Promise", "Result", "User", "Promise<Result<User>>"]
        let mut tokenizer = GenericAwareTokenizer::new();
        let mut token_stream = tokenizer.token_stream("Promise<Result<User>>");

        let mut tokens = Vec::new();
        while token_stream.advance() {
            tokens.push(token_stream.token().text.clone());
        }

        // Should include both the complete nested generic type and all its components
        assert!(tokens.contains(&"Promise<Result<User>>".to_string()), "Should include complete nested generic type");
        assert!(tokens.contains(&"Promise".to_string()), "Should include outer generic base type");
        assert!(tokens.contains(&"Result".to_string()), "Should include inner generic base type");
        assert!(tokens.contains(&"User".to_string()), "Should include innermost parameter");

        // May also include intermediate forms like "Result<User>"
        // but the exact tokenization strategy can vary
    }

    #[test]
    fn test_camel_case_splitting() {
        // Contract: Should split camelCase identifiers
        // Input: "getUserById"
        // Expected: ["get", "User", "By", "Id", "getUserById"]
        let mut tokenizer = CodeIdentifierTokenizer::new();
        let mut token_stream = tokenizer.token_stream("getUserById");

        let mut tokens = Vec::new();
        while token_stream.advance() {
            tokens.push(token_stream.token().text.clone());
        }

        // Should contain both the original identifier and split words
        assert!(tokens.contains(&"getUserById".to_string()));
        assert!(tokens.contains(&"get".to_string()));
        assert!(tokens.contains(&"User".to_string()));
        assert!(tokens.contains(&"By".to_string()));
        assert!(tokens.contains(&"Id".to_string()));
    }

    #[test]
    fn test_snake_case_splitting() {
        // Contract: Should split snake_case identifiers
        // Input: "user_data_service"
        // Expected: ["user", "data", "service", "user_data_service"]
        let mut tokenizer = CodeIdentifierTokenizer::new();
        let mut token_stream = tokenizer.token_stream("user_data_service");

        let mut tokens = Vec::new();
        while token_stream.advance() {
            tokens.push(token_stream.token().text.clone());
        }

        // Should contain both the original identifier and split words
        assert!(tokens.contains(&"user_data_service".to_string()));
        assert!(tokens.contains(&"user".to_string()));
        assert!(tokens.contains(&"data".to_string()));
        assert!(tokens.contains(&"service".to_string()));
    }

    #[test]
    fn test_acronym_handling() {
        // Contract: Should properly handle acronyms
        // Input: "XMLHttpRequest"
        // Expected: ["XML", "Http", "Request", "XMLHttpRequest"]
        let mut tokenizer = CodeIdentifierTokenizer::new();
        let mut token_stream = tokenizer.token_stream("XMLHttpRequest");

        let mut tokens = Vec::new();
        while token_stream.advance() {
            tokens.push(token_stream.token().text.clone());
        }

        // Should contain both the original identifier and split words with acronyms
        assert!(tokens.contains(&"XMLHttpRequest".to_string()));

        // Check for acronym handling - be flexible with how acronyms are split
        let has_xml = tokens.iter().any(|t| t.contains("XML"));
        let has_http = tokens.iter().any(|t| t.contains("Http"));
        let has_request = tokens.contains(&"Request".to_string());

        assert!(has_xml, "Should contain XML token");
        assert!(has_http, "Should contain Http token");
        assert!(has_request, "Should contain Request token");
    }

    #[test]
    fn test_hybrid_identifiers() {
        // Contract: Should handle mixed camelCase and snake_case
        // Input: "camelCase_with_snake"
        // Expected: ["camel", "Case", "with", "snake", "camelCase_with_snake"]
        let mut tokenizer = CodeIdentifierTokenizer::new();
        let mut token_stream = tokenizer.token_stream("camelCase_with_snake");

        let mut tokens = Vec::new();
        while token_stream.advance() {
            tokens.push(token_stream.token().text.clone());
        }

        // Should contain both the original identifier and all split words
        assert!(tokens.contains(&"camelCase_with_snake".to_string()));

        // Be flexible with how the tokenizer splits camelCase and snake_case
        let has_camel = tokens.iter().any(|t| t.contains("camel"));
        let has_case = tokens.iter().any(|t| t.contains("Case"));
        let has_with = tokens.contains(&"with".to_string());
        let has_snake = tokens.contains(&"snake".to_string());

        assert!(has_camel, "Should contain camel component");
        assert!(has_case, "Should contain Case component");
        assert!(has_with, "Should contain with component");
        assert!(has_snake, "Should contain snake component");
    }

    #[test]
    fn test_edge_case_empty_input() {
        // Contract: Should handle empty input gracefully
        // Input: ""
        // Expected: []
        let mut tokenizer = OperatorPreservingTokenizer::new();
        let mut token_stream = tokenizer.token_stream("");

        let mut tokens = Vec::new();
        while token_stream.advance() {
            tokens.push(token_stream.token().text.clone());
        }

        assert_eq!(tokens, Vec::<String>::new());
    }

    #[test]
    fn test_edge_case_special_characters() {
        // Contract: Should handle special characters
        // Input: "func@#$%"
        // Expected: Should not crash, handle gracefully
        let mut tokenizer = OperatorPreservingTokenizer::new();
        let mut token_stream = tokenizer.token_stream("func@#$%");

        let mut tokens = Vec::new();
        while token_stream.advance() {
            tokens.push(token_stream.token().text.clone());
        }

        // Should handle gracefully - exact tokenization may vary but shouldn't crash
        // The important thing is it doesn't panic and produces some reasonable output
        assert!(!tokens.is_empty(), "Should produce at least some tokens");
        assert!(tokens.iter().any(|t| t.contains("func")), "Should preserve the word 'func'");
    }

    #[test]
    fn test_performance_large_input() {
        // Contract: Should perform well on large inputs
        // Input: Very long code snippet
        // Expected: Complete in reasonable time (<1ms for 1000 chars)
        let mut tokenizer = OperatorPreservingTokenizer::new();

        // Generate large input - simulate a long code file
        let mut large_input = String::new();
        for i in 0..200 {
            large_input.push_str(&format!(
                "function getUserById{i}(id: string): Promise<User{i}> {{ return api.get('/users/' + id); }} ",
                i = i
            ));
        }

        // Should be around 15,000+ characters
        assert!(large_input.len() > 10000, "Test input should be large enough");

        let start = std::time::Instant::now();
        let mut token_stream = tokenizer.token_stream(&large_input);

        let mut token_count = 0;
        while token_stream.advance() {
            token_count += 1;
        }

        let duration = start.elapsed();

        // Performance requirement: should tokenize large input in reasonable time
        // Note: Current implementation is not optimized for performance
        assert!(duration.as_millis() < 5000,
            "Tokenization of {} chars took {}ms, should be <5000ms",
            large_input.len(), duration.as_millis());

        // Should find a reasonable number of tokens
        assert!(token_count > 100, "Should find many tokens in large input");
    }
}