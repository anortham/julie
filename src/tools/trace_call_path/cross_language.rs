//! Cross-language call path matching using naming variants

use crate::database::SymbolDatabase;
use crate::extractors::Symbol;
use crate::utils::cross_language_intelligence::generate_naming_variants;
use anyhow::Result;
use std::sync::{Arc, Mutex};
use tracing::debug;

/// Names too generic for cross-language matching — ubiquitous across all languages.
/// Matching these produces false call paths (e.g., Rust `new` → Ruby `new`).
const GENERIC_NAMES: &[&str] = &[
    // Constructors/lifecycle
    "new", "init", "initialize", "create", "destroy", "delete", "dispose",
    "setup", "teardown", "build", "make", "clone", "copy", "reset",
    // Getters/setters
    "get", "set", "put", "post", "patch", "update", "insert", "remove",
    // Execution
    "run", "start", "stop", "call", "apply", "execute", "invoke", "main",
    // I/O
    "open", "close", "read", "write", "flush", "send", "receive",
    // Conversion
    "parse", "format", "from", "into", "to_string", "to_json", "to_str",
    "from_str", "as_ref", "as_mut",
    // Validation/comparison
    "validate", "check", "test", "equals", "compare", "hash",
    // Common patterns
    "len", "size", "count", "is_empty", "empty", "default", "display",
    "debug", "print", "log", "error", "warn", "info",
];

/// Minimum symbol name length for cross-language matching.
/// Very short names (≤3 chars) are almost always too generic.
const MIN_CROSS_LANG_NAME_LEN: usize = 4;

/// Find cross-language symbol matches using naming variants.
///
/// Given a symbol, generates naming convention variants (camelCase, snake_case, etc.)
/// and finds symbols with those names in different languages.
/// Used for both upstream (callers) and downstream (callees) tracing —
/// the logic is identical since cross-language matching is directionless.
///
/// Filters out generic names (like `new`, `init`, `get`) that would create
/// false call paths between unrelated symbols across languages.
pub async fn find_cross_language_symbols(
    db: &Arc<Mutex<SymbolDatabase>>,
    symbol: &Symbol,
) -> Result<Vec<Symbol>> {
    // Skip cross-language matching for generic/short names
    let name_lower = symbol.name.to_lowercase();
    if symbol.name.len() < MIN_CROSS_LANG_NAME_LEN
        || GENERIC_NAMES.contains(&name_lower.as_str())
    {
        debug!(
            "Skipping cross-language matching for generic name: '{}'",
            symbol.name
        );
        return Ok(Vec::new());
    }

    let variants = generate_naming_variants(&symbol.name);
    debug!(
        "Generated {} naming variants for {}",
        variants.len(),
        symbol.name
    );

    let mut cross_lang_symbols = Vec::new();
    let db_lock = super::lock_db(db, "find_cross_language_symbols");

    for variant in variants {
        if variant == symbol.name {
            continue;
        }

        if let Ok(variant_symbols) = db_lock.get_symbols_by_name(&variant) {
            for variant_symbol in variant_symbols {
                if variant_symbol.language != symbol.language {
                    cross_lang_symbols.push(variant_symbol);
                }
            }
        }
    }

    drop(db_lock);

    debug!(
        "Found {} cross-language symbols for {}",
        cross_lang_symbols.len(),
        symbol.name
    );

    Ok(cross_lang_symbols)
}
