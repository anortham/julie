/// Type inference for Elixir from @spec annotations.
///
/// Associates @spec return types with their corresponding function symbols.
use crate::base::Symbol;
use std::collections::HashMap;

/// Infer types from collected @spec annotations.
///
/// `specs` maps function name → return type string (collected during attribute extraction).
/// This function matches specs to function symbols and returns symbol_id → type_string.
pub(super) fn infer_types(
    specs: &HashMap<String, String>,
    symbols: &[Symbol],
) -> HashMap<String, String> {
    let mut type_map = HashMap::new();

    for symbol in symbols {
        if let Some(return_type) = specs.get(&symbol.name) {
            type_map.insert(symbol.id.clone(), return_type.clone());
        }
    }

    type_map
}
