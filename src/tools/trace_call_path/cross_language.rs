//! Cross-language call path matching using naming variants

use crate::database::SymbolDatabase;
use crate::extractors::Symbol;
use crate::utils::cross_language_intelligence::generate_naming_variants;
use anyhow::Result;
use std::sync::{Arc, Mutex};
use tracing::debug;

/// Find cross-language symbol matches using naming variants.
///
/// Given a symbol, generates naming convention variants (camelCase, snake_case, etc.)
/// and finds symbols with those names in different languages.
/// Used for both upstream (callers) and downstream (callees) tracing —
/// the logic is identical since cross-language matching is directionless.
pub async fn find_cross_language_symbols(
    db: &Arc<Mutex<SymbolDatabase>>,
    symbol: &Symbol,
) -> Result<Vec<Symbol>> {
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
