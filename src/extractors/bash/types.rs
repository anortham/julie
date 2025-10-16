//! Type inference for Bash
//!
//! Infers variable types (string, integer, float, boolean, path) from their
//! assignments and signatures.

use crate::extractors::base::{Symbol, SymbolKind};
use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;

static INTEGER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d+$").unwrap());
static FLOAT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d+\.\d+$").unwrap());
static BOOLEAN_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(true|false)$").unwrap());

// Trait to enable delegation from main impl block
pub(super) trait BashExtractor {
    fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String>;
}

impl BashExtractor for super::BashExtractor {
    /// Infer types of variables based on their assignments
    fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        let mut types = HashMap::new();

        for symbol in symbols {
            if matches!(symbol.kind, SymbolKind::Variable | SymbolKind::Constant) {
                // Infer type from signature
                let signature = symbol.signature.as_deref().unwrap_or("");
                let mut var_type = "string".to_string();

                if let Some(value_part) = signature.split('=').nth(1) {
                    let value = value_part.trim().trim_matches(|c| c == '"' || c == '\'');

                    if INTEGER_RE.is_match(value) {
                        var_type = "integer".to_string();
                    } else if FLOAT_RE.is_match(value) {
                        var_type = "float".to_string();
                    } else if BOOLEAN_RE.is_match(&value.to_lowercase()) {
                        var_type = "boolean".to_string();
                    } else if value.starts_with('/') || value.contains('/') {
                        var_type = "path".to_string();
                    }
                }

                types.insert(symbol.name.clone(), var_type);
            }
        }

        types
    }
}
