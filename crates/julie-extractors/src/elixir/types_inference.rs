//! Type inference for Elixir
//!
//! Infers types from @spec annotations collected during extraction.

use crate::base::Symbol;
use std::collections::HashMap;

/// Infer types from @spec annotations
pub(super) fn infer_types(symbols: &[Symbol]) -> HashMap<String, String> {
    let mut types = HashMap::new();
    for symbol in symbols {
        if let Some(serde_json::Value::String(s)) =
            symbol.metadata.as_ref().and_then(|m| m.get("returnType"))
        {
            types.insert(symbol.id.clone(), s.clone());
        }
    }
    types
}
