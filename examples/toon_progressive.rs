//! Progressive complexity test for TOON

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Symbol {
    file_path: String,
    start_line: u32,
    end_line: u32,
    name: String,
    kind: String,
    confidence: Option<f32>,  // Optional field
    code_context: Option<String>,  // Optional string with newlines
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Response {
    tool: String,
    query: String,
    confidence: f32,  // Non-optional f32
    insights: String,
    results: Vec<Symbol>,
}

fn test_round_trip(name: &str, response: &Response) {
    println!("\n=== Test: {} ===", name);

    let toon = toon_format::encode_default(response).unwrap();
    println!("TOON ({} chars):\n{}\n", toon.len(), toon);

    match toon_format::decode_default::<Response>(&toon) {
        Ok(decoded) => {
            if response == &decoded {
                println!("✓ Round-trip successful!");
            } else {
                println!("✗ Round-trip failed: structs not equal");
                println!("Original: {:#?}", response);
                println!("Decoded: {:#?}", decoded);
            }
        }
        Err(e) => {
            println!("✗ Decode failed: {}", e);
        }
    }
}

fn main() {
    println!("Testing TOON with progressively complex structures\n");

    // Test 1: Simple - no optionals, no newlines
    let simple = Response {
        tool: "test".into(),
        query: "query".into(),
        confidence: 0.85,
        insights: "test".into(),
        results: vec![Symbol {
            file_path: "a.rs".into(),
            start_line: 1,
            end_line: 2,
            name: "foo".into(),
            kind: "function".into(),
            confidence: Some(0.9),
            code_context: Some("fn foo() {}".into()),
        }],
    };
    test_round_trip("Simple (with optionals)", &simple);

    // Test 2: None optionals
    let with_nones = Response {
        tool: "test".into(),
        query: "query".into(),
        confidence: 0.85,
        insights: "test".into(),
        results: vec![Symbol {
            file_path: "a.rs".into(),
            start_line: 1,
            end_line: 2,
            name: "foo".into(),
            kind: "function".into(),
            confidence: None,  // None
            code_context: None,  // None
        }],
    };
    test_round_trip("With None optionals", &with_nones);

    // Test 3: Multi-line code context
    let with_newlines = Response {
        tool: "test".into(),
        query: "query".into(),
        confidence: 0.85,
        insights: "test".into(),
        results: vec![Symbol {
            file_path: "a.rs".into(),
            start_line: 1,
            end_line: 3,
            name: "foo".into(),
            kind: "function".into(),
            confidence: Some(0.9),
            code_context: Some("fn foo() {\n    println!(\"hello\");\n}".into()),
        }],
    };
    test_round_trip("With newlines in code_context", &with_newlines);

    // Test 4: Multiple results
    let multi_results = Response {
        tool: "test".into(),
        query: "query".into(),
        confidence: 0.85,
        insights: "Mostly Functions (3 of 3)".into(),
        results: vec![
            Symbol {
                file_path: "a.rs".into(),
                start_line: 1,
                end_line: 3,
                name: "foo".into(),
                kind: "function".into(),
                confidence: Some(0.92),
                code_context: Some("fn foo() {\n    code\n}".into()),
            },
            Symbol {
                file_path: "b.rs".into(),
                start_line: 10,
                end_line: 15,
                name: "bar".into(),
                kind: "function".into(),
                confidence: Some(0.78),
                code_context: Some("fn bar() { stuff }".into()),
            },
            Symbol {
                file_path: "c.rs".into(),
                start_line: 20,
                end_line: 22,
                name: "baz".into(),
                kind: "function".into(),
                confidence: None,
                code_context: None,
            },
        ],
    };
    test_round_trip("Multiple results with mixed optionals", &multi_results);
}
