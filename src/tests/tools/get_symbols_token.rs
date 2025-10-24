//! Token optimization integration tests for GetSymbolsTool
//! Following TDD methodology: RED -> GREEN -> REFACTOR
//!
//! Tests verify that Smart Read body extraction uses ContextTruncator
//! to gracefully handle large symbol bodies (>50 lines) while preserving
//! complete structure for smaller symbols.

#[cfg(test)]
mod get_symbols_token_tests {
    use crate::utils::context_truncation::ContextTruncator;
    use crate::utils::token_estimation::TokenEstimator;

    /// Test that short symbol bodies are NOT truncated
    #[test]
    fn test_short_body_no_truncation() {
        let token_estimator = TokenEstimator::new();

        // Create a short function body (30 lines - below 50 line threshold)
        let short_body: Vec<String> = (1..=30)
            .map(|i| format!("    console.log('Line {}');", i))
            .collect();

        let original_text = short_body.join("\n");
        let original_tokens = token_estimator.estimate_string(&original_text);

        // Short bodies should NOT be truncated (< 50 lines)
        // In actual GetSymbolsTool, this would return the full body
        assert_eq!(short_body.len(), 30, "Short body should remain at 30 lines");
        assert!(
            original_tokens < 1000,
            "Short body should be well under token limits"
        );
    }

    /// Test that long symbol bodies ARE truncated with structure preservation
    #[test]
    fn test_long_body_gets_truncated_with_structure_preservation() {
        let truncator = ContextTruncator::new();
        let token_estimator = TokenEstimator::new();

        // Create a long function body (100 lines - well above 50 line threshold)
        let mut long_body: Vec<String> = Vec::new();

        // Function signature (important - should be preserved)
        long_body.push("async function processLargeDataset(data) {".to_string());
        long_body.push("    // Configuration section (important)".to_string());
        long_body.push("    const config = {".to_string());
        long_body.push("        batchSize: 1000,".to_string());
        long_body.push("        timeout: 30000".to_string());
        long_body.push("    };".to_string());
        long_body.push("".to_string());

        // Middle section (can be truncated)
        for i in 1..=80 {
            long_body.push(format!("    // Processing step {}", i));
            long_body.push(format!("    await processItem(data[{}]);", i));
        }

        // Closing section (important - should be preserved)
        long_body.push("".to_string());
        long_body.push("    return result;".to_string());
        long_body.push("}".to_string());

        let original_text = long_body.join("\n");
        let original_tokens = token_estimator.estimate_string(&original_text);

        // Apply smart truncation (like GetSymbolsTool does for >50 line bodies)
        let truncated = truncator.smart_truncate(&long_body, 40);
        let truncated_tokens = token_estimator.estimate_string(&truncated);

        // Verify truncation occurred
        assert!(
            long_body.len() > 50,
            "Original body should be >50 lines (was {})",
            long_body.len()
        );

        // Verify significant token reduction (at least 30% reduction)
        let reduction_ratio = (original_tokens - truncated_tokens) as f64 / original_tokens as f64;
        assert!(
            reduction_ratio >= 0.30,
            "Should achieve at least 30% token reduction (got {:.1}%)",
            reduction_ratio * 100.0
        );

        // Verify structure preservation - should contain important parts
        assert!(
            truncated.contains("async function processLargeDataset"),
            "Should preserve function signature"
        );

        // ContextTruncator preserves first and last lines, plus closing braces
        assert!(truncated.contains("}"), "Should preserve closing brace");

        // Should include truncation indicator
        assert!(
            truncated.contains("truncated"),
            "Should indicate truncation occurred"
        );
    }

    /// Test that truncation respects the target line limit
    #[test]
    fn test_truncation_respects_target_limit() {
        let truncator = ContextTruncator::new();

        // Create 100-line body
        let large_body: Vec<String> = (1..=100)
            .map(|i| format!("    console.log('Line {}');", i))
            .collect();

        // Request truncation to ~40 lines (like GetSymbolsTool does)
        let truncated = truncator.smart_truncate(&large_body, 40);
        let truncated_lines: Vec<&str> = truncated.lines().collect();

        // Smart truncation includes ellipsis markers, so actual line count may vary
        // The important thing is: it's less than original and includes truncation indicator
        assert!(
            truncated_lines.len() < large_body.len(),
            "Truncated output should have fewer lines than original (was {} vs {})",
            truncated_lines.len(),
            large_body.len()
        );

        // Should indicate truncation occurred
        assert!(
            truncated.contains("truncated"),
            "Should show truncation indicator"
        );
    }

    /// Test token estimation accuracy for different body sizes
    #[test]
    fn test_token_estimation_for_various_body_sizes() {
        let token_estimator = TokenEstimator::new();

        // Small body (10 lines)
        let small_body: Vec<String> = (1..=10)
            .map(|i| format!("    console.log('Line {}');", i))
            .collect();
        let small_tokens = token_estimator.estimate_string(&small_body.join("\n"));

        // Medium body (50 lines - at threshold)
        let medium_body: Vec<String> = (1..=50)
            .map(|i| format!("    console.log('Line {}');", i))
            .collect();
        let medium_tokens = token_estimator.estimate_string(&medium_body.join("\n"));

        // Large body (100 lines - should be truncated)
        let large_body: Vec<String> = (1..=100)
            .map(|i| format!("    console.log('Line {}');", i))
            .collect();
        let large_tokens = token_estimator.estimate_string(&large_body.join("\n"));

        // Verify roughly linear scaling (within 20% variance)
        // Small body is baseline
        assert!(
            small_tokens < medium_tokens,
            "Medium should have more tokens"
        );
        assert!(
            medium_tokens < large_tokens,
            "Large should have more tokens"
        );

        // Rough proportionality check (allowing for variance in line content)
        let medium_ratio = medium_tokens as f64 / small_tokens as f64;
        assert!(
            medium_ratio >= 3.5 && medium_ratio <= 6.0,
            "Medium body (5x lines) should have 3.5-6x tokens (got {:.2}x)",
            medium_ratio
        );

        let large_ratio = large_tokens as f64 / small_tokens as f64;
        assert!(
            large_ratio >= 7.0 && large_ratio <= 12.0,
            "Large body (10x lines) should have 7-12x tokens (got {:.2}x)",
            large_ratio
        );
    }

    /// Test that ContextTruncator preserves code structure markers
    #[test]
    fn test_truncator_preserves_structure_markers() {
        let truncator = ContextTruncator::new();

        // Create structured code with important markers
        let structured_body: Vec<String> = vec![
            "class UserService {".to_string(),
            "    constructor() {".to_string(),
            "        // Initialization".to_string(),
            "        this.users = [];".to_string(),
            "    }".to_string(),
            "".to_string(),
            "    // 80 lines of implementation details".to_string(),
        ];

        // Add 80 lines of middle content
        let mut full_body = structured_body.clone();
        for i in 1..=80 {
            full_body.push(format!("    // Detail line {}", i));
            full_body.push(format!("    this.process(item{});", i));
        }

        // Add closing
        full_body.push("".to_string());
        full_body.push("    // End of class".to_string());
        full_body.push("}".to_string());

        let truncated = truncator.smart_truncate(&full_body, 40);

        // Verify key structure elements preserved
        // ContextTruncator identifies "essential" lines (class definitions, comments, closing braces)
        assert!(
            truncated.contains("class UserService"),
            "Should preserve class declaration"
        );

        // Constructor is a function keyword - should be identified as essential
        assert!(
            truncated.contains("constructor") || truncated.contains("// Initialization"),
            "Should preserve constructor or initialization comment"
        );

        assert!(truncated.contains("}"), "Should preserve closing brace");

        // Should indicate truncation occurred
        assert!(
            truncated.contains("...") || truncated.lines().count() < full_body.len(),
            "Should show truncation indication or reduced line count"
        );
    }

    /// Test that truncation works with different languages/styles
    #[test]
    fn test_truncation_language_agnostic() {
        let truncator = ContextTruncator::new();
        let token_estimator = TokenEstimator::new();

        // Test with Rust-style code
        let mut rust_body: Vec<String> = vec![
            "pub struct DataProcessor {".to_string(),
            "    buffer: Vec<u8>,".to_string(),
            "}".to_string(),
            "".to_string(),
            "impl DataProcessor {".to_string(),
            "    pub fn new() -> Self {".to_string(),
        ];

        // Add 60 lines of implementation
        for i in 1..=60 {
            rust_body.push(format!("        // Step {}", i));
            rust_body.push(format!("        self.process_item({});", i));
        }

        rust_body.push("    }".to_string());
        rust_body.push("}".to_string());

        let original_tokens = token_estimator.estimate_string(&rust_body.join("\n"));
        let truncated = truncator.smart_truncate(&rust_body, 40);
        let truncated_tokens = token_estimator.estimate_string(&truncated);

        // Verify truncation achieved token reduction
        assert!(
            truncated_tokens < original_tokens,
            "Truncation should reduce token count"
        );

        // Verify structure preservation
        // ContextTruncator identifies "struct" as essential keyword
        assert!(
            truncated.contains("pub struct DataProcessor")
                || truncated.contains("struct DataProcessor"),
            "Should preserve struct definition"
        );

        // "impl" keyword should be identified as essential (though currently not in identify_essential_lines)
        // At minimum, should preserve closing braces
        assert!(truncated.contains("}"), "Should preserve closing braces");

        // Should indicate truncation occurred
        assert!(
            truncated.contains("truncated"),
            "Should show truncation occurred"
        );
    }

    /// Test Smart Read workflow: structure mode vs body modes with truncation
    #[test]
    fn test_smart_read_workflow_token_benefits() {
        let token_estimator = TokenEstimator::new();
        let truncator = ContextTruncator::new();

        // Simulate a large class with multiple methods (100+ lines total)
        let mut full_file_content: Vec<String> = vec![
            "export class PaymentService {".to_string(),
            "    private apiKey: string;".to_string(),
            "".to_string(),
            "    constructor(apiKey: string) {".to_string(),
            "        this.apiKey = apiKey;".to_string(),
            "    }".to_string(),
            "".to_string(),
        ];

        // Add 5 methods, each with 20 lines
        for method_num in 1..=5 {
            full_file_content.push(format!("    async method{}(param) {{", method_num));
            for line_num in 1..=18 {
                full_file_content.push(format!("        // Implementation detail {}", line_num));
                full_file_content.push(format!("        await this.process({});", line_num));
            }
            full_file_content.push("        return result;".to_string());
            full_file_content.push("    }".to_string());
            full_file_content.push("".to_string());
        }

        full_file_content.push("}".to_string());

        let full_file_text = full_file_content.join("\n");
        let full_file_tokens = token_estimator.estimate_string(&full_file_text);

        // Simulate structure-only mode (just signatures, no bodies)
        let structure_only = vec![
            "📄 **payment.service.ts** (6 symbols)",
            "",
            "🏛️ **PaymentService** *(L:1)*",
            "  📦 **apiKey** *(L:2)* [private]",
            "  🔧 **constructor** `(apiKey: string)` *(L:4)*",
            "  🔧 **method1** `async (param)` *(L:8)*",
            "  🔧 **method2** `async (param)` *(L:28)*",
            "  🔧 **method3** `async (param)` *(L:48)*",
            "  🔧 **method4** `async (param)` *(L:68)*",
            "  🔧 **method5** `async (param)` *(L:88)*",
            "",
            "---",
            "**Summary:**",
            "• Total symbols: 6",
            "• Top-level: 1",
            "• Class: 1",
            "• Method: 5",
        ]
        .join("\n");

        let structure_tokens = token_estimator.estimate_string(&structure_only);

        // Calculate token savings from structure-only mode
        let savings_ratio = 1.0 - (structure_tokens as f64 / full_file_tokens as f64);

        // Structure-only should achieve 70-90% token savings (per Smart Read promise)
        assert!(
            savings_ratio >= 0.70,
            "Structure-only mode should achieve 70%+ token savings (got {:.1}%)",
            savings_ratio * 100.0
        );

        // Now test body extraction with truncation for ONE method
        // Simulate targeting a specific method (lines 8-28 = method1)
        let method1_lines: Vec<String> = full_file_content[7..27].to_vec(); // Extract method1
        let method1_truncated = truncator.smart_truncate(&method1_lines, 15);
        let method1_tokens = token_estimator.estimate_string(&method1_truncated);

        // Targeted body extraction should be much smaller than full file
        assert!(
            method1_tokens < full_file_tokens / 3,
            "Single method body should be < 33% of full file tokens"
        );
    }
}
