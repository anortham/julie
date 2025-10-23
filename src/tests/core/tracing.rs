// Inline tests extracted from src/tracing/mod.rs
// Test count: 2 tests
// Original module size: 127 lines (lines 515-641)

#[cfg(test)]
mod tests {
    use crate::extractors::{Symbol, SymbolKind};
    use crate::tracing::{ArchitecturalLayer, CrossLanguageTracer, TraceOptions};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    /// Create a test symbol for cross-language tracing
    fn create_test_symbol() -> Symbol {
        Symbol {
            id: "test_onclick".to_string(),
            name: "onClick".to_string(),
            kind: SymbolKind::Method,
            language: "typescript".to_string(),
            file_path: "/src/components/Button.tsx".to_string(),
            signature: Some("onClick: () => void".to_string()),
            start_line: 25,
            start_column: 5,
            end_line: 27,
            end_column: 6,
            start_byte: 512,
            end_byte: 580,
            parent_id: Some("button_component".to_string()),
            doc_comment: None,
            visibility: None,
            semantic_group: Some("ui-events".to_string()),
            confidence: Some(0.95),
            metadata: Some(HashMap::new()),
            code_context: None,
        }
    }

    /// Helper to create a mock tracer for testing
    async fn create_test_tracer() -> CrossLanguageTracer {
        use crate::database::SymbolDatabase;
        use crate::embeddings::EmbeddingEngine;

        // Create a temporary database for testing
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Mutex::new(SymbolDatabase::new(&db_path).unwrap()));

        // Create embedding engine (will need cache dir)
        let cache_dir = temp_dir.path().join("cache");
        std::fs::create_dir_all(&cache_dir).unwrap();
        let embeddings =
            Arc::new(EmbeddingEngine::new("bge-small", cache_dir, db.clone()).await.unwrap());

        CrossLanguageTracer::new(db, embeddings)
    }

    #[cfg_attr(
        not(feature = "network_models"),
        ignore = "requires downloadable embedding model"
    )]
    #[tokio::test]
    async fn test_revolutionary_cross_language_tracing() {
        let tracer = create_test_tracer().await;

        println!("🚀 Testing revolutionary cross-language tracing...");

        // This is the killer use case: trace from a React button click
        let trace = tracer
            .trace_data_flow(
                "onClick",
                TraceOptions {
                    max_depth: Some(5),
                    target_layers: vec![
                        ArchitecturalLayer::Frontend,
                        ArchitecturalLayer::Backend,
                        ArchitecturalLayer::Database,
                    ],
                    ..Default::default()
                },
            )
            .await
            .expect("Trace should succeed");

        // Verify our revolutionary cross-language tracing works!
        println!("🎉 REVOLUTIONARY TRACING RESULT:");
        println!("📊 Steps: {}", trace.steps.len());
        println!("🎯 Confidence: {:.1}%", trace.confidence * 100.0);
        println!("🌍 Languages: {:?}", trace.languages_involved);
        println!("🏗️ Complete: {}", trace.complete);

        // Print the complete trace for verification
        for (i, step) in trace.steps.iter().enumerate() {
            println!(
                "Step {}: {} ({} → {:?}) - {:.1}% confidence",
                i + 1,
                step.symbol.name,
                step.symbol.language,
                step.layer,
                step.confidence * 100.0
            );
        }

        // Verify basic GREEN phase functionality
        assert!(!trace.steps.is_empty(), "Should have trace steps");
        assert!(trace.confidence > 0.0, "Should have some confidence");
        assert!(
            trace.is_cross_layer_trace(),
            "Should span multiple architectural layers"
        );

        println!("🚀 SUCCESS: Cross-language tracing GREEN phase is working!");
    }

    #[cfg_attr(
        not(feature = "network_models"),
        ignore = "requires downloadable embedding model"
    )]
    #[tokio::test]
    async fn test_layer_detection() {
        let tracer = create_test_tracer().await;

        // Test layer detection from file paths and symbol context
        let test_symbol = create_test_symbol();
        let layer = tracer.detect_layer(&test_symbol);

        println!("🎯 Testing layer detection:");
        println!(
            "   Symbol: {} in {}",
            test_symbol.name, test_symbol.file_path
        );
        println!("   Detected layer: {:?}", layer);

        assert_eq!(layer, ArchitecturalLayer::Frontend);
        println!("✅ Layer detection working correctly!");
    }
}
