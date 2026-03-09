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

    // Pre-split source into lines for line-based fallback (used when byte offsets unavailable)
    let source_str = String::from_utf8_lossy(&source_code);
    let source_lines: Vec<&str> = source_str.lines().collect();

    // Extract bodies based on mode
    for symbol in symbols.iter_mut() {
        let should_extract = match mode {
            "minimal" => symbol.parent_id.is_none(), // Top-level only
            "full" => true,                          // All symbols
            _ => false,                              // Unknown mode, don't extract
        };

        if should_extract {
            let start_byte = symbol.start_byte as usize;
            let end_byte = symbol.end_byte as usize;

            if start_byte == 0 && end_byte == 0 && symbol.start_line > 0 {
                // Byte offsets unavailable (e.g. Vue SFC symbols from create_symbol_manual).
                // Fall back to line-based extraction using start_line/end_line (1-indexed).
                let start_idx = (symbol.start_line as usize).saturating_sub(1);
                let end_idx = symbol.end_line as usize; // inclusive end → exclusive slice

                if start_idx < source_lines.len() && end_idx <= source_lines.len() {
                    let code = source_lines[start_idx..end_idx].join("\n");
                    symbol.code_context = Some(code);
                } else {
                    debug!(
                        symbol_name = %symbol.name,
                        start_line = symbol.start_line,
                        end_line = symbol.end_line,
                        total_lines = source_lines.len(),
                        "Symbol line range out of bounds, skipping extraction"
                    );
                    symbol.code_context = None;
                }
            } else if start_byte < source_code.len() && end_byte <= source_code.len() {
                // Standard byte-based extraction
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
