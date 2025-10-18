// C# Type Inference

use crate::extractors::base::Symbol;
use std::collections::HashMap;

/// Infer types for all symbols
pub fn infer_types(symbols: &[Symbol]) -> HashMap<String, String> {
    let mut type_map = HashMap::new();

    for symbol in symbols {
        let inferred_type = match symbol.kind {
            crate::extractors::base::SymbolKind::Method
            | crate::extractors::base::SymbolKind::Function => infer_method_return_type(symbol),
            crate::extractors::base::SymbolKind::Property => infer_property_type(symbol),
            crate::extractors::base::SymbolKind::Field
            | crate::extractors::base::SymbolKind::Constant => infer_field_type(symbol),
            crate::extractors::base::SymbolKind::Variable => infer_variable_type(symbol),
            _ => None,
        };

        if let Some(inferred_type) = inferred_type {
            type_map.insert(symbol.id.clone(), inferred_type);
        }
    }

    type_map
}

fn infer_method_return_type(symbol: &Symbol) -> Option<String> {
    let signature = symbol.signature.as_ref()?;
    let parts: Vec<&str> = signature.split_whitespace().collect();
    let modifiers = [
        "public",
        "private",
        "protected",
        "internal",
        "static",
        "virtual",
        "override",
        "abstract",
        "async",
        "sealed",
    ];
    let method_name_index = parts.iter().position(|part| part.contains(&symbol.name))?;

    if method_name_index > 0 {
        for i in (0..method_name_index).rev() {
            let part = parts[i];
            if !modifiers.contains(&part) && !part.is_empty() {
                return Some(part.to_string());
            }
        }
    }

    None
}

fn infer_property_type(symbol: &Symbol) -> Option<String> {
    let signature = symbol.signature.as_ref()?;
    let parts: Vec<&str> = signature.split_whitespace().collect();
    let modifiers = [
        "public",
        "private",
        "protected",
        "internal",
        "static",
        "virtual",
        "override",
        "abstract",
    ];

    for part in &parts {
        if !modifiers.contains(part) && !part.is_empty() {
            return Some(part.to_string());
        }
    }

    None
}

fn infer_field_type(symbol: &Symbol) -> Option<String> {
    let signature = symbol.signature.as_ref()?;
    let parts: Vec<&str> = signature.split_whitespace().collect();
    let modifiers = [
        "public",
        "private",
        "protected",
        "internal",
        "static",
        "readonly",
        "const",
        "volatile",
    ];

    for part in &parts {
        if !modifiers.contains(part) && !part.is_empty() {
            return Some(part.to_string());
        }
    }

    None
}

fn infer_variable_type(_symbol: &Symbol) -> Option<String> {
    None
}
