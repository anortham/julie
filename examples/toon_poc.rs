//! Proof of Concept: TOON format for Julie search results
//!
//! Tests whether toon-format crate can encode OptimizedResponse

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Symbol {
    file_path: String,
    start_line: u32,
    end_line: u32,
    name: String,
    kind: String,
    confidence: Option<f32>,
    code_context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OptimizedResponse {
    tool: String,
    query: String,
    search_method: String,
    total_found: usize,
    confidence: f32,
    insights: String,
    results: Vec<Symbol>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create sample data matching Julie's actual structure
    let response = OptimizedResponse {
        tool: "fast_search".to_string(),
        query: "getUserData".to_string(),
        search_method: "hybrid".to_string(),
        total_found: 3,
        confidence: 0.85,
        insights: "Mostly Functions (3 of 3)".to_string(),
        results: vec![
            Symbol {
                file_path: "src/auth.rs".to_string(),
                start_line: 45,
                end_line: 52,
                name: "getUserData".to_string(),
                kind: "function".to_string(),
                confidence: Some(0.92),
                code_context: Some("pub fn getUserData(user_id: &str) -> Result<User> {\n    database.query(\"SELECT * FROM users WHERE id = ?\", user_id)\n}".to_string()),
            },
            Symbol {
                file_path: "src/db.rs".to_string(),
                start_line: 123,
                end_line: 145,
                name: "fetchUserData".to_string(),
                kind: "function".to_string(),
                confidence: Some(0.78),
                code_context: Some("async fn fetchUserData(id: String) -> Option<User> {\n    // Fetch user from database\n}".to_string()),
            },
            Symbol {
                file_path: "tests/auth_test.rs".to_string(),
                start_line: 12,
                end_line: 18,
                name: "mock_getUserData".to_string(),
                kind: "function".to_string(),
                confidence: Some(0.65),
                code_context: Some("fn mock_getUserData() -> User { User::default() }".to_string()),
            },
        ],
    };

    // Encode to JSON (current format)
    let json_output = serde_json::to_string_pretty(&response)?;
    println!("=== JSON Format (Current) ===");
    println!("{}", json_output);
    println!("\nJSON Token estimate: {} chars", json_output.len());

    // Encode to TOON format
    let toon_output = toon_format::encode_default(&response)?;
    println!("\n=== TOON Format ===");
    println!("{}", toon_output);
    println!("\nTOON Token estimate: {} chars", toon_output.len());

    // Calculate savings
    let savings_pct = ((json_output.len() - toon_output.len()) as f64 / json_output.len() as f64) * 100.0;
    println!("\n=== Comparison ===");
    println!("JSON: {} chars", json_output.len());
    println!("TOON: {} chars", toon_output.len());
    println!("Savings: {:.1}%", savings_pct);

    // Test round-trip (lossless?)
    // TOON requires wrapping in an object or using relaxed mode
    use toon_format::{DecodeOptions, EncodeOptions};

    // Try with relaxed decoding options
    let decode_opts = DecodeOptions {
        strict: false,  // Allow multiple root-level values
        ..Default::default()
    };

    match toon_format::decode::<OptimizedResponse>(&toon_output, &decode_opts) {
        Ok(decoded) => {
            println!("\n=== Round-trip Test ===");
            println!("✓ Successfully decoded back to OptimizedResponse");
            println!("Query preserved: {}", decoded.query);
            println!("Results count preserved: {}", decoded.results.len());
            println!("First result: {} at {}:{}",
                decoded.results[0].name,
                decoded.results[0].file_path,
                decoded.results[0].start_line
            );
        }
        Err(e) => {
            println!("\n=== Round-trip Test ===");
            println!("✗ Decoding failed: {}", e);
            println!("Note: TOON may need wrapped structure for complex objects");
        }
    }

    // Alternative: Wrap in an object for better TOON compatibility
    #[derive(Debug, Serialize, Deserialize)]
    struct WrappedResponse {
        search_result: OptimizedResponse,
    }

    let wrapped = WrappedResponse {
        search_result: response.clone(),
    };

    let wrapped_toon = toon_format::encode_default(&wrapped)?;
    println!("\n=== TOON Format (Wrapped) ===");
    println!("{}", wrapped_toon);
    println!("\nWrapped TOON Token estimate: {} chars", wrapped_toon.len());

    // Try decoding wrapped version
    let decoded_wrapped: WrappedResponse = toon_format::decode_default(&wrapped_toon)?;
    println!("\n=== Wrapped Round-trip Test ===");
    println!("✓ Successfully decoded wrapped version");
    println!("Query: {}", decoded_wrapped.search_result.query);

    Ok(())
}
