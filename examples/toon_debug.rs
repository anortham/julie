//! Debug TOON deserialization issue

use serde::{Deserialize, Serialize};

// Simplified version of our structure
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct SimpleResponse {
    tool: String,
    query: String,
    total_found: usize,
    results: Vec<SimpleSymbol>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct SimpleSymbol {
    file_path: String,
    line: u32,
    name: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create test data
    let response = SimpleResponse {
        tool: "fast_search".to_string(),
        query: "test".to_string(),
        total_found: 2,
        results: vec![
            SimpleSymbol {
                file_path: "src/main.rs".to_string(),
                line: 10,
                name: "main".to_string(),
            },
            SimpleSymbol {
                file_path: "src/lib.rs".to_string(),
                line: 20,
                name: "helper".to_string(),
            },
        ],
    };

    println!("=== Original Struct ===");
    println!("{:#?}\n", response);

    // Encode to TOON
    let toon = toon_format::encode_default(&response)?;
    println!("=== TOON Format ===");
    println!("{}\n", toon);

    // Try to decode back
    println!("=== Attempting Decode ===");
    match toon_format::decode_default::<SimpleResponse>(&toon) {
        Ok(decoded) => {
            println!("✓ Success!");
            println!("{:#?}\n", decoded);
            assert_eq!(response, decoded, "Round-trip failed!");
            println!("✓ Round-trip successful!");
        }
        Err(e) => {
            println!("✗ Decode failed: {}", e);
            println!("\nLet's try decoding to serde_json::Value to see what TOON thinks it is:");
            let value: serde_json::Value = toon_format::decode_default(&toon)?;
            println!("{:#?}", value);
        }
    }

    Ok(())
}
