/// Type inference for Razor symbols
use crate::extractors::base::{Symbol, SymbolKind};
use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;

// Static regexes compiled once for performance
static RAZOR_TYPE_PATTERN1: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:\[\w+.*?\]\s+)?(?:public|private|protected|internal|static)\s+(\w+(?:<[^>]+>)?(?:\?|\[\])?)\s+\w+").unwrap()
});
static RAZOR_TYPE_PATTERN2: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:public|private|protected|internal|static|async)\s+(\w+(?:<[^>]+>)?)\s+\w+\s*\(")
        .unwrap()
});
static RAZOR_TYPE_PATTERN3: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\w+(?:<[^>]+>)?(?:\?|\[\])?)\s+\w+\s*=").unwrap());

impl super::RazorExtractor {
    /// Infer types for all symbols
    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        let mut types = HashMap::new();
        for symbol in symbols {
            let mut inferred_type = "unknown".to_string();

            // Use actual type information from metadata
            let metadata = &symbol.metadata;
            if let Some(property_type) = metadata
                .as_ref()
                .and_then(|m| m.get("propertyType"))
                .and_then(|v| v.as_str())
            {
                inferred_type = property_type.to_string();
            } else if let Some(field_type) = metadata
                .as_ref()
                .and_then(|m| m.get("fieldType"))
                .and_then(|v| v.as_str())
            {
                inferred_type = field_type.to_string();
            } else if let Some(variable_type) = metadata
                .as_ref()
                .and_then(|m| m.get("variableType"))
                .and_then(|v| v.as_str())
            {
                inferred_type = variable_type.to_string();
            } else if let Some(return_type) = metadata
                .as_ref()
                .and_then(|m| m.get("returnType"))
                .and_then(|v| v.as_str())
            {
                inferred_type = return_type.to_string();
            } else if let Some(signature) = &symbol.signature {
                // Try to extract type from signature
                let type_patterns: Vec<&Regex> = vec![
                    &*RAZOR_TYPE_PATTERN1,
                    &*RAZOR_TYPE_PATTERN2,
                    &*RAZOR_TYPE_PATTERN3,
                ];

                for pattern in &type_patterns {
                    if let Some(captures) = pattern.captures(signature) {
                        if let Some(type_match) = captures.get(1) {
                            let matched_type = type_match.as_str();
                            if matched_type != symbol.name {
                                inferred_type = matched_type.to_string();
                                break;
                            }
                        }
                    }
                }
            }

            // Handle special cases
            if metadata
                .as_ref()
                .and_then(|m| m.get("isDataBinding"))
                .and_then(|v| v.as_str())
                == Some("true")
            {
                inferred_type = "bool".to_string();
            }

            if symbol.kind == SymbolKind::Method {
                if let Some(signature) = &symbol.signature {
                    if signature.contains("async Task") {
                        inferred_type = "Task".to_string();
                    } else if signature.contains("void") {
                        inferred_type = "void".to_string();
                    }
                }
            }

            types.insert(symbol.id.clone(), inferred_type);
        }
        types
    }
}
