//! Integration test to reproduce the bug where .R files don't extract symbols
//!
//! BUG: R extractor works perfectly in unit tests (74/74 passing)
//! BUT: Real .R files in fixtures/r/real-world/ extract ZERO symbols
//!
//! This test reads the actual ggplot2-geom-point.R file and attempts extraction.

use crate::extractors::r::RExtractor;
use crate::language::get_tree_sitter_language;
use std::fs;
use std::path::Path;
use tree_sitter::Parser;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extractors::base::SymbolKind;

    #[test]
    fn test_bug_reproduction_real_r_file_not_extracting() {
        // Step 1: Read the actual R file that's failing in production
        // Use absolute path from CARGO_MANIFEST_DIR to avoid CWD issues in parallel tests
        let r_file_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/r/real-world/ggplot2-geom-point.R");
        let content =
            fs::read_to_string(&r_file_path).expect("Failed to read R file - file should exist");

        println!("📄 Read R file: {} bytes", content.len());
        println!("📄 First 200 chars: {}", &content[..200.min(content.len())]);

        // Step 2: Get tree-sitter language for R
        let language = get_tree_sitter_language("r").expect("R language should be supported");

        println!("✅ Got tree-sitter language for R");

        // Step 3: Parse the R code
        let mut parser = Parser::new();
        parser
            .set_language(&language)
            .expect("Failed to set R language in parser");

        let tree = parser
            .parse(&content, None)
            .expect("Failed to parse R code");

        println!("🌳 Parsed R code successfully");
        println!("🌳 Root node: {:?}", tree.root_node().kind());
        println!("🌳 Child count: {}", tree.root_node().child_count());

        // Step 4: Create extractor (same as production does in mod.rs:322)
        let workspace_root = Path::new(".");
        let mut extractor = RExtractor::new(
            "r".to_string(),
            r_file_path.to_string_lossy().to_string(),
            content.clone(),
            workspace_root,
        );

        println!("🔧 Created RExtractor");

        // Step 5: Extract symbols
        let symbols = extractor.extract_symbols(&tree);

        println!("📊 Extracted {} symbols", symbols.len());

        for (i, symbol) in symbols.iter().enumerate() {
            println!("  Symbol {}: {} ({:?})", i + 1, symbol.name, symbol.kind);
        }

        // Step 6: VERIFY - we expect to find these 4 functions:
        // - geom_point
        // - translate_shape_string
        // - stat_summary
        // - filter_outliers

        assert!(
            symbols.len() >= 4,
            "❌ BUG CONFIRMED: Expected at least 4 R functions, found {}. This is the bug!",
            symbols.len()
        );

        let function_names: Vec<&str> = symbols
            .iter()
            .filter(|s| matches!(s.kind, SymbolKind::Function))
            .map(|s| s.name.as_str())
            .collect();

        println!("📋 Function names found: {:?}", function_names);

        assert!(
            function_names.contains(&"geom_point"),
            "Should extract geom_point function"
        );
        assert!(
            function_names.contains(&"translate_shape_string"),
            "Should extract translate_shape_string function"
        );
        assert!(
            function_names.contains(&"stat_summary"),
            "Should extract stat_summary function"
        );
        assert!(
            function_names.contains(&"filter_outliers"),
            "Should extract filter_outliers function"
        );
    }
}
