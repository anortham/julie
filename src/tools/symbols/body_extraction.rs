//! Code body extraction for symbols
//!
//! Provides different levels of code context based on reading mode:
//! - "structure": No code bodies (just names and signatures)
//! - "minimal": Code bodies for top-level symbols only
//! - "full": Code bodies for all symbols

use anyhow::Result;
use tracing::{debug, warn};

use crate::extractors::base::Symbol;

/// Extract code bodies for symbols based on mode parameter
pub fn extract_code_bodies(
    mut symbols: Vec<Symbol>,
    file_path: &str,
    mode: &str,
) -> Result<Vec<Symbol>> {
    // In "structure" mode, strip all code context
    if mode == "structure" {
        for symbol in symbols.iter_mut() {
            symbol.code_context = None;
        }
        return Ok(symbols);
    }

    // Read the source file for body extraction
    let source_code = match std::fs::read(file_path) {
        Ok(bytes) => bytes,
        Err(_e) => {
            debug!(file_path = %file_path, "Failed to read file for code body extraction");
            // Return symbols with context stripped if file can't be read
            for symbol in symbols.iter_mut() {
                symbol.code_context = None;
            }
            return Ok(symbols);
        }
    };

    // Extract bodies based on mode
    for symbol in symbols.iter_mut() {
        let should_extract = match mode {
            "minimal" => symbol.parent_id.is_none(), // Top-level only
            "full" => true,                          // All symbols
            _ => false,                              // Unknown mode, don't extract
        };

        if should_extract {
            // Extract the code bytes for this symbol
            let start_byte = symbol.start_byte as usize;
            let end_byte = symbol.end_byte as usize;

            if start_byte < source_code.len() && end_byte <= source_code.len() {
                // Use lossy conversion to handle potential UTF-8 issues
                let code_bytes = &source_code[start_byte..end_byte];
                symbol.code_context = Some(String::from_utf8_lossy(code_bytes).to_string());
            } else {
                warn!(
                    symbol_name = %symbol.name,
                    start_byte = start_byte,
                    end_byte = end_byte,
                    file_size = source_code.len(),
                    "Symbol byte range out of bounds, skipping extraction"
                );
                symbol.code_context = None;
            }
        } else {
            // Don't extract for this symbol based on mode
            symbol.code_context = None;
        }
    }

    Ok(symbols)
}
