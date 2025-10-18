/// Type inference from method and field signatures
use crate::extractors::base::{Symbol, SymbolKind};
use crate::extractors::java::JavaExtractor;
use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;

// Static regexes compiled once for performance
static METHOD_RETURN_TYPE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\w+[\w<>\[\], ]*)\s+\w+\s*\(").unwrap());
static METHOD_MODIFIER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(public|private|protected|static|final|abstract|synchronized|native|strictfp)\s+")
        .unwrap()
});
static FIELD_TYPE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\w+[\w<>\[\], ]*)\s+\w+").unwrap());
static FIELD_MODIFIER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(public|private|protected|static|final|volatile|transient)\s+").unwrap()
});

/// Infer types from method and field signatures
pub(super) fn infer_types(
    _extractor: &JavaExtractor,
    symbols: &[Symbol],
) -> HashMap<String, String> {
    let mut types = HashMap::new();

    for symbol in symbols {
        // Extract return type from method signatures
        if symbol.kind == SymbolKind::Method {
            if let Some(signature) = &symbol.signature {
                // Regex pattern to match return type in method signature
                if let Some(captures) = METHOD_RETURN_TYPE_RE.captures(signature) {
                    if let Some(return_type_match) = captures.get(1) {
                        let return_type = return_type_match.as_str().trim();
                        // Clean up modifiers from return type
                        let clean_return_type =
                            METHOD_MODIFIER_RE.replace(return_type, "").to_string();
                        types.insert(symbol.id.clone(), clean_return_type);
                    }
                }
            }
        }

        // Extract field types from field signatures
        if symbol.kind == SymbolKind::Property {
            if let Some(signature) = &symbol.signature {
                // Regex pattern to match field type
                if let Some(captures) = FIELD_TYPE_RE.captures(signature) {
                    if let Some(field_type_match) = captures.get(1) {
                        let field_type = field_type_match.as_str().trim();
                        // Clean up modifiers from field type
                        let clean_field_type =
                            FIELD_MODIFIER_RE.replace(field_type, "").to_string();
                        types.insert(symbol.id.clone(), clean_field_type);
                    }
                }
            }
        }
    }

    types
}
