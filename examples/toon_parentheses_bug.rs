//! TOON Bug: Parentheses in string values cause parsing errors

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Data {
    message: String,
    count: usize,
}

fn main() {
    println!("=== TOON Bug Reproduction: Parentheses in Strings ===\n");

    // Test 1: Without parentheses
    let without_parens = Data {
        message: "Mostly Functions".into(),
        count: 3,
    };
    let toon1 = toon_format::encode_default(&without_parens).unwrap();
    println!("Test 1: WITHOUT parentheses");
    println!("TOON: {}", toon1);
    match toon_format::decode_default::<Data>(&toon1) {
        Ok(_) => println!("✓ Decode successful\n"),
        Err(e) => println!("✗ Decode failed: {}\n", e),
    }

    // Test 2: With parentheses
    let with_parens = Data {
        message: "Mostly Functions (3 of 3)".into(),
        count: 3,
    };
    let toon2 = toon_format::encode_default(&with_parens).unwrap();
    println!("Test 2: WITH parentheses");
    println!("TOON: {}", toon2);
    match toon_format::decode_default::<Data>(&toon2) {
        Ok(_) => println!("✓ Decode successful\n"),
        Err(e) => println!("✗ Decode failed: {}\n", e),
    }

    // Test 3: With parentheses AND multiple fields
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct MultiField {
        field1: String,
        field2: String,
        field3: usize,
    }

    let multi = MultiField {
        field1: "test".into(),
        field2: "Mostly Functions (3 of 3)".into(),
        field3: 42,
    };
    let toon3 = toon_format::encode_default(&multi).unwrap();
    println!("Test 3: Parentheses with multiple fields");
    println!("TOON:\n{}", toon3);
    match toon_format::decode_default::<MultiField>(&toon3) {
        Ok(_) => println!("✓ Decode successful\n"),
        Err(e) => println!("✗ Decode failed: {}\n", e),
    }

    // Test 4: Just to confirm - other special chars
    let special = Data {
        message: "Test [with] {braces}".into(),
        count: 1,
    };
    let toon4 = toon_format::encode_default(&special).unwrap();
    println!("Test 4: Other special characters");
    println!("TOON: {}", toon4);
    match toon_format::decode_default::<Data>(&toon4) {
        Ok(_) => println!("✓ Decode successful\n"),
        Err(e) => println!("✗ Decode failed: {}\n", e),
    }
}
