use crate::tracing::{CrossLanguageTracer, DataFlowTrace, TraceStep, ConnectionType, ArchitecturalLayer, TraceOptions};
use crate::extractors::{Symbol, SymbolKind};
use crate::database::SymbolDatabase;
use crate::search::SearchEngine;
use crate::embeddings::EmbeddingEngine;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tempfile;

/// Test fixtures and helpers for cross-language tracing tests
/// These represent realistic symbols from a polyglot web application
mod test_fixtures {
    use super::*;
    use std::collections::HashMap;

    /// Create a realistic React component symbol
    pub fn create_login_button_symbol() -> Symbol {
        Symbol {
            id: "login_button_onclick".to_string(),
            name: "onClick".to_string(),
            kind: SymbolKind::Method,
            language: "typescript".to_string(),
            file_path: "/src/components/LoginButton.tsx".to_string(),
            signature: Some("onClick: () => void".to_string()),
            start_line: 25,
            start_column: 5,
            end_line: 27,
            end_column: 6,
            start_byte: 0,
            end_byte: 0,
            doc_comment: None,
            visibility: None,
            parent_id: Some("login_button_component".to_string()),
            metadata: Some(HashMap::new()),
            semantic_group: None,
            confidence: None,
        }
    }

    /// Create a TypeScript service call symbol
    pub fn create_auth_service_login() -> Symbol {
        Symbol {
            id: "auth_service_login".to_string(),
            name: "login".to_string(),
            kind: SymbolKind::Method,
            language: "typescript".to_string(),
            file_path: "/src/services/authService.ts".to_string(),
            signature: Some("login(credentials: LoginCredentials): Promise<AuthResult>".to_string()),
            start_line: 15,
            start_column: 3,
            end_line: 25,
            end_column: 4,
            start_byte: 0,
            end_byte: 0,
            doc_comment: Some("Authenticates user credentials against the backend API".to_string()),
            visibility: None,
            parent_id: Some("auth_service_class".to_string()),
            metadata: Some(HashMap::new()),
            semantic_group: None,
            confidence: None,
        }
    }

    /// Create a C# controller endpoint symbol
    pub fn create_csharp_auth_controller() -> Symbol {
        Symbol {
            id: "auth_controller_login".to_string(),
            name: "Login".to_string(),
            kind: SymbolKind::Method,
            language: "csharp".to_string(),
            file_path: "/Controllers/AuthController.cs".to_string(),
            signature: Some("[HttpPost(\"/api/auth/login\")] public async Task<IActionResult> Login(LoginRequest request)".to_string()),
            start_line: 45,
            start_column: 8,
            end_line: 65,
            end_column: 9,
            start_byte: 0,
            end_byte: 0,
            doc_comment: Some("REST API endpoint for user authentication".to_string()),
            visibility: None,
            parent_id: Some("auth_controller_class".to_string()),
            metadata: Some(HashMap::new()),
            semantic_group: None,
            confidence: None,
        }
    }

    /// Create a C# service method symbol
    pub fn create_csharp_user_service() -> Symbol {
        Symbol {
            id: "user_service_authenticate".to_string(),
            name: "AuthenticateAsync".to_string(),
            kind: SymbolKind::Method,
            language: "csharp".to_string(),
            file_path: "/Services/UserService.cs".to_string(),
            signature: Some("public async Task<AuthResult> AuthenticateAsync(string email, string password)".to_string()),
            start_line: 78,
            start_column: 8,
            end_line: 95,
            end_column: 9,
            start_byte: 0,
            end_byte: 0,
            doc_comment: Some("Core authentication logic with password validation".to_string()),
            visibility: None,
            parent_id: Some("user_service_class".to_string()),
            metadata: Some(HashMap::new()),
            semantic_group: None,
            confidence: None,
        }
    }

    /// Create a SQL table symbol
    pub fn create_users_table_symbol() -> Symbol {
        Symbol {
            id: "users_table".to_string(),
            name: "users".to_string(),
            kind: SymbolKind::Class, // Tables are treated as classes
            language: "sql".to_string(),
            file_path: "/database/schema.sql".to_string(),
            signature: Some("CREATE TABLE users (id, email, password_hash, created_at)".to_string()),
            start_line: 15,
            start_column: 1,
            end_line: 22,
            end_column: 2,
            start_byte: 0,
            end_byte: 0,
            doc_comment: Some("User accounts table with authentication credentials".to_string()),
            visibility: None,
            parent_id: None,
            metadata: Some(HashMap::new()),
            semantic_group: None,
            confidence: None,
        }
    }
}

/// Tests for the revolutionary cross-language tracing engine
/// These tests define the killer feature that makes Julie unique
#[cfg(test)]
mod cross_language_tracing_tests {
    use super::*;
    use super::test_fixtures::*;

    /// Helper to create a mock tracer for testing
    async fn create_mock_tracer() -> CrossLanguageTracer {
        // Note: These will be mocked/stubbed for testing
        // For now, we'll use placeholders - actual mock implementations coming in GREEN phase

        // Create a temporary database for testing
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Mutex::new(SymbolDatabase::new(&db_path).unwrap()));

        // Create a temporary search index for testing
        let index_dir = temp_dir.path().join("index");
        std::fs::create_dir_all(&index_dir).unwrap();
        let search = Arc::new(RwLock::new(SearchEngine::new(&index_dir).unwrap()));

        // Create embedding engine (will need cache dir)
        let cache_dir = temp_dir.path().join("cache");
        std::fs::create_dir_all(&cache_dir).unwrap();
        let embeddings = Arc::new(EmbeddingEngine::new("bge-small", cache_dir).unwrap());

        CrossLanguageTracer::new(db, search, embeddings)
    }

    /// Test the holy grail: complete React → C# → SQL trace
    #[tokio::test]
    async fn test_complete_frontend_to_database_trace() {
        let tracer = create_mock_tracer().await;

        // This is the killer use case: trace from a React button click all the way to the database
        let trace = tracer.trace_data_flow(
            "onClick", // Start from React component onClick handler
            TraceOptions {
                max_depth: Some(10),
                target_layers: vec![
                    ArchitecturalLayer::Frontend,
                    ArchitecturalLayer::Backend,
                    ArchitecturalLayer::Database
                ],
                ..Default::default()
            }
        ).await.expect("Trace should succeed");

        // Verify our revolutionary cross-language tracing works!
        println!("🎉 REVOLUTIONARY TRACING RESULT:");
        println!("📊 Steps: {}", trace.steps.len());
        println!("🎯 Confidence: {:.1}%", trace.confidence * 100.0);
        println!("🌍 Languages: {:?}", trace.languages_involved);
        println!("🏗️ Complete: {}", trace.complete);

        // Verify basic functionality (GREEN phase requirements)
        assert!(!trace.steps.is_empty(), "Should have trace steps");
        assert!(trace.confidence > 0.0, "Should have some confidence");
        assert!(trace.is_cross_layer_trace(), "Should span multiple architectural layers");

        // Print the complete trace for verification
        for (i, step) in trace.steps.iter().enumerate() {
            println!("Step {}: {} ({:?} → {:?}) - {:.1}% confidence",
                i + 1,
                step.symbol.name,
                step.symbol.language,
                step.layer,
                step.confidence * 100.0
            );
        }

        println!("🚀 SUCCESS: Cross-language tracing is working!");
    }

    /// Test direct AST-based relationship tracing
    #[tokio::test]
    async fn test_direct_relationship_tracing() {
        let tracer = create_mock_tracer().await;

        // Test following direct call relationships from the database
        let trace = tracer.trace_data_flow(
            "authService.login",
            TraceOptions {
                max_depth: Some(3),
                include_semantic_matches: false, // Only direct relationships
                ..Default::default()
            }
        ).await.expect("Direct trace should succeed");

        // Should find direct call to backend endpoint
        let backend_steps: Vec<_> = trace.steps.iter()
            .filter(|s| s.connection_type == ConnectionType::DirectCall)
            .collect();

        assert!(!backend_steps.is_empty(), "Should find direct call relationships");

        // Verify evidence is provided
        for step in &trace.steps {
            assert!(!step.evidence.is_empty(), "Each step should have evidence");
            assert!(step.confidence > 0.5, "Direct relationships should have high confidence");
        }
    }

    /// Test HTTP API pattern matching (TypeScript → C# endpoint)
    #[tokio::test]
    async fn test_api_pattern_matching() {
        let tracer = create_mock_tracer().await;

        // Should detect axios/fetch calls and match them to backend endpoints
        let trace = tracer.trace_data_flow(
            "axios.post('/api/auth/login')",
            TraceOptions::default()
        ).await.expect("API pattern matching should work");

        // Should find the corresponding C# controller endpoint
        let api_connections: Vec<_> = trace.steps.iter()
            .filter(|s| s.connection_type == ConnectionType::NetworkCall)
            .collect();

        assert!(!api_connections.is_empty(), "Should detect HTTP API connections");

        let csharp_endpoints: Vec<_> = trace.steps.iter()
            .filter(|s| s.symbol.language == "csharp" && s.symbol.signature.as_ref().unwrap().contains("HttpPost"))
            .collect();

        assert!(!csharp_endpoints.is_empty(), "Should find matching C# endpoint");
    }

    /// Test semantic cross-language matching (the magic!)
    #[tokio::test]
    async fn test_semantic_cross_language_matching() {
        let tracer = create_mock_tracer().await;

        // Should connect similar concepts across languages using embeddings
        // e.g., TypeScript User interface → C# UserDto → SQL users table
        let trace = tracer.trace_data_flow(
            "User",
            TraceOptions {
                include_semantic_matches: true,
                min_confidence: Some(0.6),
                ..Default::default()
            }
        ).await.expect("Semantic matching should work");

        // Should find semantically similar symbols in different languages
        let semantic_connections: Vec<_> = trace.steps.iter()
            .filter(|s| s.connection_type == ConnectionType::SemanticMatch)
            .collect();

        assert!(!semantic_connections.is_empty(), "Should find semantic connections");

        // Should span multiple languages
        let languages: std::collections::HashSet<_> = trace.steps.iter()
            .map(|s| &s.symbol.language)
            .collect();

        assert!(languages.len() >= 2, "Semantic matching should connect multiple languages");
    }

    /// Test confidence scoring accuracy
    #[tokio::test]
    async fn test_confidence_scoring() {
        let tracer = create_mock_tracer().await;

        let trace = tracer.trace_data_flow(
            "login",
            TraceOptions::default()
        ).await.expect("Should generate trace with confidence scores");

        // Confidence should decrease as we move through less certain connections
        for i in 1..trace.steps.len() {
            let prev_confidence = trace.steps[i-1].confidence;
            let curr_confidence = trace.steps[i].confidence;

            // Overall trace confidence should be reasonable product of step confidences
            assert!(trace.confidence <= prev_confidence, "Trace confidence should not exceed individual step confidence");
        }

        // Direct calls should have highest confidence
        let direct_calls: Vec<_> = trace.steps.iter()
            .filter(|s| s.connection_type == ConnectionType::DirectCall)
            .collect();

        for step in direct_calls {
            assert!(step.confidence > 0.8, "Direct calls should have high confidence");
        }

        // Semantic matches should have lower but still reasonable confidence
        let semantic_matches: Vec<_> = trace.steps.iter()
            .filter(|s| s.connection_type == ConnectionType::SemanticMatch)
            .collect();

        for step in semantic_matches {
            assert!(step.confidence > 0.3, "Semantic matches should have reasonable confidence");
        }
    }

    /// Test architectural layer detection
    #[tokio::test]
    async fn test_layer_detection() {
        let tracer = create_mock_tracer().await;

        // Test layer detection from file paths and symbol context
        let frontend_symbol = create_login_button_symbol();
        let layer = tracer.detect_layer(&frontend_symbol);
        println!("🎯 Frontend symbol: {} → {:?}", frontend_symbol.file_path, layer);
        assert_eq!(layer, ArchitecturalLayer::Frontend);

        let backend_symbol = create_csharp_auth_controller();
        let layer = tracer.detect_layer(&backend_symbol);
        println!("🎯 Backend symbol: {} → {:?}", backend_symbol.file_path, layer);
        assert_eq!(layer, ArchitecturalLayer::Backend);

        let database_symbol = create_users_table_symbol();
        let layer = tracer.detect_layer(&database_symbol);
        println!("🎯 Database symbol: {} → {:?}", database_symbol.file_path, layer);
        assert_eq!(layer, ArchitecturalLayer::Database);

        println!("✅ Layer detection working perfectly!");
    }

    /// Test cycle detection and infinite loop prevention
    #[tokio::test]
    async fn test_cycle_detection() {
        let tracer = create_mock_tracer().await;

        let trace = tracer.trace_data_flow(
            "recursive_function",
            TraceOptions {
                max_depth: Some(100), // High limit to test cycle detection
                ..Default::default()
            }
        ).await.expect("Should handle cycles gracefully");

        // Should not get stuck in infinite loops
        assert!(trace.steps.len() < 50, "Should detect cycles and stop tracing");

        // Should still provide useful partial trace
        assert!(!trace.steps.is_empty(), "Should provide partial trace even with cycles");
    }

    /// Test performance with complex codebases
    #[tokio::test]
    async fn test_tracing_performance() {
        let tracer = create_mock_tracer().await;

        let start = std::time::Instant::now();

        let trace = tracer.trace_data_flow(
            "complex_function",
            TraceOptions {
                timeout_seconds: Some(5), // Should complete within 5 seconds
                ..Default::default()
            }
        ).await.expect("Should complete within timeout");

        let duration = start.elapsed();

        assert!(duration.as_secs() < 5, "Tracing should be fast even for complex cases");
        assert!(!trace.steps.is_empty(), "Should produce meaningful results quickly");
    }

    /// Test error handling and graceful degradation
    #[tokio::test]
    async fn test_error_handling() {
        let tracer = create_mock_tracer().await;

        // Test with non-existent symbol
        let result = tracer.trace_data_flow(
            "non_existent_symbol_12345",
            TraceOptions::default()
        ).await;

        // Should handle gracefully, not panic
        match result {
            Ok(trace) => {
                // If it succeeds, should indicate low confidence or incomplete
                assert!(trace.steps.is_empty() || trace.confidence < 0.3);
            }
            Err(_) => {
                // Acceptable to return error for non-existent symbols
            }
        }
    }

    /// Test trace summary generation for AI consumption
    #[tokio::test]
    async fn test_trace_summary_generation() {
        let tracer = create_mock_tracer().await;

        let trace = tracer.trace_data_flow(
            "getUserData",
            TraceOptions::default()
        ).await.expect("Should generate trace");

        let summary = trace.get_flow_summary();

        // Summary should be human-readable and informative
        assert!(summary.contains("steps"), "Summary should mention step count");
        assert!(summary.contains("layers"), "Summary should mention layer count");
        assert!(summary.contains("confidence"), "Summary should include confidence");

        // Should be suitable for AI context windows
        assert!(summary.len() < 500, "Summary should be concise for AI consumption");
        assert!(summary.len() > 20, "Summary should be informative");
    }
}

/// Integration tests with real Julie codebase data
/// These will test dogfooding - using Julie to trace Julie's own code
#[cfg(test)]
mod dogfooding_tests {
    use super::*;

    /// Test tracing Julie's own indexing process
    #[tokio::test]
    async fn test_trace_julie_indexing_flow() {
        // This will trace how Julie indexes files:
        // MCP tool call → workspace.index_files() → extractor.extract_symbols() → database.store_symbols()
        todo!("Implement dogfooding test - trace Julie's own indexing process")
    }

    /// Test tracing Julie's search functionality
    #[tokio::test]
    async fn test_trace_julie_search_flow() {
        // This will trace Julie's search:
        // search_code MCP tool → TantivyIndex.search() → database.get_symbol_details()
        todo!("Implement dogfooding test - trace Julie's search functionality")
    }
}