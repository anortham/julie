use crate::extractors::base::Symbol;
use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;

// Static regex compiled once for performance
static ZIG_TYPE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r":\s*([\w\[\]!?*]+)").unwrap());

/// Infer types from symbols using Zig-specific rules
pub(super) fn infer_types(symbols: &[Symbol]) -> HashMap<String, String> {
    let mut types = HashMap::new();

    // Zig type inference based on symbol metadata and signatures
    for symbol in symbols {
        if let Some(signature) = &symbol.signature {
            // Extract Zig types from signatures
            if let Some(type_match) = ZIG_TYPE_RE.captures(signature) {
                types.insert(symbol.name.clone(), type_match[1].to_string());
            }
        }

        // Use metadata for Zig-specific types
        if let Some(is_error) = symbol
            .metadata
            .as_ref()
            .and_then(|m| m.get("isErrorType"))
            .and_then(|v| v.as_bool())
        {
            if is_error {
                types.insert(symbol.name.clone(), "error".to_string());
            }
        }
        if let Some(is_type_alias) = symbol
            .metadata
            .as_ref()
            .and_then(|m| m.get("isTypeAlias"))
            .and_then(|v| v.as_bool())
        {
            if is_type_alias {
                types.insert(symbol.name.clone(), "type".to_string());
            }
        }

        use crate::extractors::base::SymbolKind;
        match symbol.kind {
            SymbolKind::Class => {
                if symbol
                    .metadata
                    .as_ref()
                    .and_then(|m| m.get("isErrorType"))
                    .and_then(|v| v.as_bool())
                    != Some(true)
                {
                    types.insert(symbol.name.clone(), "struct".to_string());
                }
            }
            SymbolKind::Enum => {
                types.insert(symbol.name.clone(), "enum".to_string());
            }
            _ => {}
        }
    }

    types
}
