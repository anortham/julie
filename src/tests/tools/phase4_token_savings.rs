//! Phase 4 Token Savings Tests
//!
//! Measure token reduction from data structure optimizations (skip_serializing_if).

use anyhow::Result;
use crate::extractors::{Symbol, SymbolKind};
use crate::extractors::base::Visibility;
use serde_json;

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: Create a Symbol with many null fields (typical real-world scenario)
    fn create_sparse_symbol(name: &str) -> Symbol {
        Symbol {
            id: format!("test_{}", name),
            name: name.to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "src/tools/shared.rs".to_string(),
            start_line: 100,
            start_column: 4,
            end_line: 110,
            end_column: 5,
            start_byte: 3000,
            end_byte: 3500,
            signature: Some(format!("fn {}()", name)),
            doc_comment: None,  // NULL
            visibility: None,  // NULL
            parent_id: None,  // NULL
            metadata: None,  // NULL
            semantic_group: None,  // NULL
            confidence: None,  // NULL
            code_context: None,  // NULL
            content_type: None,  // NULL
        }
    }

    #[test]
    fn test_null_field_omission_json() -> Result<()> {
        // Create a symbol with 8 null fields
        let symbol = create_sparse_symbol("test_func");

        // Serialize to JSON
        let json = serde_json::to_string(&symbol)?;

        // Verify null fields are NOT present in JSON output
        assert!(!json.contains("\"doc_comment\""), "doc_comment should be omitted");
        assert!(!json.contains("\"visibility\""), "visibility should be omitted");
        assert!(!json.contains("\"parent_id\""), "parent_id should be omitted");
        assert!(!json.contains("\"metadata\""), "metadata should be omitted");
        assert!(!json.contains("\"semantic_group\""), "semantic_group should be omitted");
        assert!(!json.contains("\"confidence\""), "confidence should be omitted");
        assert!(!json.contains("\"code_context\""), "code_context should be omitted");
        assert!(!json.contains("\"content_type\""), "content_type should be omitted");

        // Verify non-null fields ARE present
        assert!(json.contains("\"name\""), "name should be present");
        assert!(json.contains("\"signature\""), "signature should be present (has value)");

        println!("✅ Optimized JSON: {} chars", json.len());
        println!("   Sample: {}...", &json[..json.len().min(200)]);

        Ok(())
    }

    #[test]
    fn test_token_savings_measurement() -> Result<()> {
        // Create 10 symbols with sparse fields (realistic scenario)
        let symbols: Vec<Symbol> = (0..10)
            .map(|i| create_sparse_symbol(&format!("func_{}", i)))
            .collect();

        // Serialize to JSON
        let json = serde_json::to_string_pretty(&symbols)?;
        let char_count = json.len();

        // Count how many null fields would have been serialized without optimization
        // Each null field adds approximately: "field_name": null, (20-30 chars)
        let null_fields_per_symbol = 8; // doc_comment, visibility, parent_id, metadata, semantic_group, confidence, code_context, content_type
        let avg_chars_per_null_field = 25; // "semantic_group": null,
        let estimated_waste_without_optimization = symbols.len() * null_fields_per_symbol * avg_chars_per_null_field;

        println!("\n📊 Phase 4 Token Savings Analysis:");
        println!("   Symbols: {}", symbols.len());
        println!("   Null fields per symbol: {}", null_fields_per_symbol);
        println!("   Total null fields omitted: {}", symbols.len() * null_fields_per_symbol);
        println!("   Optimized JSON size: {} chars", char_count);
        println!("   Estimated size without optimization: ~{} chars", char_count + estimated_waste_without_optimization);
        println!("   Estimated savings: ~{} chars ({:.1}%)",
            estimated_waste_without_optimization,
            (estimated_waste_without_optimization as f64 / (char_count + estimated_waste_without_optimization) as f64) * 100.0
        );

        // Verify actual optimization (no null fields in output)
        let null_count = json.matches("null").count();
        assert_eq!(null_count, 0, "Expected no 'null' values in optimized JSON");

        println!("   ✅ Verified: 0 null values in output");

        // Rough token estimate (1 token ≈ 4 chars)
        let estimated_tokens_saved = estimated_waste_without_optimization / 4;
        println!("   💰 Estimated tokens saved: ~{} tokens", estimated_tokens_saved);

        Ok(())
    }

    #[test]
    fn test_non_null_fields_still_serialized() -> Result<()> {
        // Create a symbol with ALL fields populated
        let symbol = Symbol {
            id: "test_full".to_string(),
            name: "full_func".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "src/lib.rs".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 10,
            end_column: 1,
            start_byte: 0,
            end_byte: 500,
            signature: Some("fn full_func()".to_string()),
            doc_comment: Some("/// This is a doc comment".to_string()),
            visibility: Some(Visibility::Public),
            parent_id: Some("parent_123".to_string()),
            metadata: Some(std::collections::HashMap::new()),
            semantic_group: Some("utilities".to_string()),
            confidence: Some(0.95),
            code_context: Some("fn main() { ... }".to_string()),
            content_type: Some("code".to_string()),
        };

        let json = serde_json::to_string(&symbol)?;

        // Verify ALL fields are present when they have values
        assert!(json.contains("\"signature\""), "signature should be present");
        assert!(json.contains("\"doc_comment\""), "doc_comment should be present");
        assert!(json.contains("\"visibility\""), "visibility should be present");
        assert!(json.contains("\"parent_id\""), "parent_id should be present");
        assert!(json.contains("\"metadata\""), "metadata should be present");
        assert!(json.contains("\"semantic_group\""), "semantic_group should be present");
        assert!(json.contains("\"confidence\""), "confidence should be present");
        assert!(json.contains("\"code_context\""), "code_context should be present");
        assert!(json.contains("\"content_type\""), "content_type should be present");

        println!("✅ All non-null fields present: {} chars", json.len());

        Ok(())
    }
}
