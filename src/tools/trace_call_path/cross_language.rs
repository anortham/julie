//! Cross-language call path matching using naming variants

use crate::database::SymbolDatabase;
use crate::extractors::Symbol;
use crate::utils::cross_language_intelligence::generate_naming_variants;
use anyhow::Result;
use std::sync::{Arc, Mutex};
use tracing::debug;

/// Find cross-language callers using naming variants
pub async fn find_cross_language_callers(
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
    let db_lock = match db.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            tracing::warn!(
                "Database mutex poisoned in find_cross_language_callers, recovering: {}",
                poisoned
            );
            poisoned.into_inner()
        }
    };

    for variant in variants {
        if variant == symbol.name {
            continue; // Skip original
        }

        // Find symbols with this variant name
        if let Ok(variant_symbols) = db_lock.get_symbols_by_name(&variant) {
            for variant_symbol in variant_symbols {
                // Only include if different language - naming variant match is sufficient
                if variant_symbol.language != symbol.language {
                    cross_lang_symbols.push(variant_symbol);
                }
            }
        }
    }

    drop(db_lock);

    debug!(
        "Found {} cross-language callers for {}",
        cross_lang_symbols.len(),
        symbol.name
    );

    Ok(cross_lang_symbols)
}

/// Find cross-language callees using naming variants
pub async fn find_cross_language_callees(
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
    let db_lock = match db.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            tracing::warn!(
                "Database mutex poisoned in find_cross_language_callees, recovering: {}",
                poisoned
            );
            poisoned.into_inner()
        }
    };

    for variant in variants {
        if variant == symbol.name {
            continue;
        }

        // Find symbols with this variant name in different languages
        if let Ok(variant_symbols) = db_lock.get_symbols_by_name(&variant) {
            for variant_symbol in variant_symbols {
                // Only include if different language - naming variant match is sufficient
                if variant_symbol.language != symbol.language {
                    cross_lang_symbols.push(variant_symbol);
                }
            }
        }
    }

    drop(db_lock);

    debug!(
        "Found {} cross-language callees for {}",
        cross_lang_symbols.len(),
        symbol.name
    );

    Ok(cross_lang_symbols)
}
