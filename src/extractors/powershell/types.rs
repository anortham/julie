//! PowerShell type inference and type annotation handling
//! Infers types from type annotations, assignments, and values

use crate::extractors::base::{Symbol, SymbolKind};
use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;

// Static regexes for type inference
static TYPE_ANNOTATION_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[(\w+)\]").unwrap());
static INTEGER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d+$").unwrap());
static FLOAT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d+\.\d+$").unwrap());
static BOOL_VAR_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\$(true|false)$").unwrap());
static TYPE_BRACKET_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\[.*\]$").unwrap());

/// Infer types from symbols (variables, properties) based on annotations and assignments
pub(super) fn infer_types(symbols: &[Symbol]) -> HashMap<String, String> {
    let mut types = HashMap::new();

    for symbol in symbols {
        if symbol.kind == SymbolKind::Variable || symbol.kind == SymbolKind::Property {
            let signature = symbol.signature.as_ref().map_or("", |s| s.as_str());
            let mut type_name = "object".to_string();

            // Extract type from PowerShell type annotations
            if let Some(captures) = TYPE_ANNOTATION_RE.captures(signature) {
                let captured_type = captures.get(1).map_or("", |m| m.as_str());
                if !captured_type.is_empty() {
                    type_name = captured_type.to_lowercase();
                }
            } else if signature.contains("=") {
                // Infer from value
                let value = signature.split('=').nth(1).map_or("", |v| v.trim());
                if INTEGER_RE.is_match(value) {
                    type_name = "int".to_string();
                } else if FLOAT_RE.is_match(value) {
                    type_name = "double".to_string();
                } else if BOOL_VAR_RE.is_match(value) {
                    type_name = "bool".to_string();
                } else if value.starts_with('"') || value.starts_with("'") {
                    type_name = "string".to_string();
                } else if value.starts_with("@(") {
                    type_name = "array".to_string();
                } else if value.starts_with("@{") {
                    type_name = "hashtable".to_string();
                }
            }

            types.insert(symbol.name.clone(), type_name);
        }
    }

    types
}

/// Check if a string looks like a PowerShell type bracket
pub(super) fn is_type_bracket(text: &str) -> bool {
    TYPE_BRACKET_RE.is_match(text)
}
