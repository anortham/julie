/// Lua class pattern detection
///
/// Post-processes symbols to detect Lua class patterns:
/// - Tables with metatable setup (local Class = {})
/// - Variables created with setmetatable (local Dog = setmetatable({}, Animal))
/// - Tables with __index pattern (Class.__index = Class)
/// - Tables with new and colon methods (Class.new, Class:method)
use crate::extractors::base::{Symbol, SymbolKind};
use regex::Regex;
use std::sync::LazyLock;

// Static regex compiled once for performance
static SETMETATABLE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"setmetatable\(\s*\{\s*\}\s*,\s*(\w+)\s*\)").unwrap());

/// Detect and upgrade Lua class patterns
///
/// This post-processes all symbols to identify Lua class patterns and upgrades
/// Variable symbols to Class symbols when they match class creation patterns.
pub(crate) fn detect_lua_classes(symbols: &mut Vec<Symbol>) {
    // Collect upgrades to apply (can't mutate while iterating)
    let mut class_upgrades = Vec::new();

    for (index, symbol) in symbols.iter().enumerate() {
        if symbol.kind == SymbolKind::Variable {
            let class_name = &symbol.name;

            // Pattern 1: Tables with metatable setup (local Class = {})
            let is_table = symbol
                .metadata
                .as_ref()
                .and_then(|m| m.get("dataType"))
                .map(|dt| dt.as_str() == Some("table"))
                .unwrap_or(false);

            // Pattern 2: Variables created with setmetatable (local Dog = setmetatable({}, Animal))
            let is_setmetatable = symbol
                .signature
                .as_ref()
                .map(|s| s.contains("setmetatable("))
                .unwrap_or(false);

            // Only check class patterns for tables or setmetatable creations
            if is_table || is_setmetatable {
                // Look for metatable patterns that indicate this is a class
                let has_index_pattern = symbols.iter().any(|s| {
                    s.signature
                        .as_ref()
                        .map(|sig| {
                            sig.contains(&format!("{}.__index = {}", class_name, class_name))
                        })
                        .unwrap_or(false)
                });

                let has_new_method = symbols.iter().any(|s| {
                    s.name == "new"
                        && s.signature
                            .as_ref()
                            .map(|sig| sig.contains(&format!("{}.new", class_name)))
                            .unwrap_or(false)
                });

                let has_colon_methods = symbols.iter().any(|s| {
                    s.kind == SymbolKind::Method
                        && s.signature
                            .as_ref()
                            .map(|sig| sig.contains(&format!("{}:", class_name)))
                            .unwrap_or(false)
                });

                // If it has metatable patterns, upgrade to Class
                if has_index_pattern || (has_new_method && has_colon_methods) || is_setmetatable {
                    class_upgrades.push((index, is_setmetatable, symbol.signature.clone()));
                }
            }
        }
    }

    // Apply class upgrades
    for (index, is_setmetatable, signature) in class_upgrades {
        symbols[index].kind = SymbolKind::Class;

        // Extract inheritance information from setmetatable pattern
        if is_setmetatable {
            if let Some(captures) = signature.as_ref().and_then(|s| SETMETATABLE_RE.captures(s)) {
                if let Some(parent_class_name) = captures.get(1) {
                    let parent_class_name = parent_class_name.as_str();
                    // Verify the parent class exists in our symbols
                    let parent_exists = symbols.iter().any(|s| {
                        s.name == parent_class_name
                            && (s.kind == SymbolKind::Class
                                || s.metadata
                                    .as_ref()
                                    .and_then(|m| m.get("dataType"))
                                    .map(|dt| dt.as_str() == Some("table"))
                                    .unwrap_or(false))
                    });

                    if parent_exists {
                        let metadata = symbols[index]
                            .metadata
                            .get_or_insert_with(std::collections::HashMap::new);
                        metadata.insert("baseClass".to_string(), parent_class_name.into());
                    }
                }
            }
        }
    }
}
