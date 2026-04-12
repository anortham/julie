use crate::base::Symbol;
use std::collections::HashMap;

pub fn infer_types(symbols: &[Symbol]) -> HashMap<String, String> {
    let mut type_map = HashMap::new();

    for symbol in symbols {
        let inferred_type = match symbol.kind {
            crate::base::SymbolKind::Function | crate::base::SymbolKind::Method => {
                infer_function_return_type(symbol)
            }
            crate::base::SymbolKind::Property => infer_property_type(symbol),
            crate::base::SymbolKind::Field | crate::base::SymbolKind::Constant => {
                infer_field_type(symbol)
            }
            _ => None,
        };

        if let Some(inferred_type) = inferred_type {
            type_map.insert(symbol.id.clone(), inferred_type);
        }
    }

    type_map
}

fn infer_function_return_type(symbol: &Symbol) -> Option<String> {
    let signature = symbol.signature.as_ref()?;
    if signature.contains("Sub ") && !signature.contains("Function ") {
        return None;
    }

    let paren_end = signature.rfind(')')?;
    let after_paren = &signature[paren_end + 1..];
    let as_pos = after_paren.rfind(" As ")?;
    let type_str = after_paren[as_pos + 4..].trim();
    if type_str.is_empty() {
        None
    } else {
        Some(type_str.to_string())
    }
}

fn infer_property_type(symbol: &Symbol) -> Option<String> {
    let signature = symbol.signature.as_ref()?;
    let as_pos = signature.rfind(" As ")?;
    let type_str = signature[as_pos + 4..].trim();
    if type_str.is_empty() {
        None
    } else {
        Some(type_str.to_string())
    }
}

fn infer_field_type(symbol: &Symbol) -> Option<String> {
    let signature = symbol.signature.as_ref()?;
    let as_pos = signature.rfind(" As ")?;
    let type_str = signature[as_pos + 4..].trim();
    if type_str.is_empty() {
        None
    } else {
        Some(type_str.to_string())
    }
}
