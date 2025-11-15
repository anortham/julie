//! Go Test Functions and Build Constraints Tests
//!
//! Tests for extracting Go's testing and build tag features:
//! - Build constraints/tags (//+build integration, //+build !race)
//! - Test functions (TestXxx)
//! - Table-driven tests (subtests with t.Run)
//! - Benchmark functions (BenchmarkXxx)
//! - Example functions (ExampleXxx)
//! - Fuzzing tests (FuzzXxx) - Go 1.18+

use crate::extractors::base::SymbolKind;
use crate::extractors::go::GoExtractor;
use crate::tests::test_utils::init_parser;
use std::path::PathBuf;

#[test]
fn test_extract_test_functions_and_build_constraints() {
    let code = r#"
// +build integration
// +build !race

package main

import (
    "testing"
    "time"
)

// Test function
func TestUserService(t *testing.T) {
    service := NewUserService()

    user := &User{Name: "John Doe"}
    err := service.CreateUser(user)
    if err != nil {
        t.Errorf("CreateUser failed: %v", err)
    }
}

// Table-driven test
func TestValidation(t *testing.T) {
    tests := []struct {
        name     string
        input    string
        expected bool
    }{
        {"valid email", "test@example.com", true},
        {"invalid email", "invalid", false},
        {"empty string", "", false},
    }

    for _, tt := range tests {
        t.Run(tt.name, func(t *testing.T) {
            result := IsValidEmail(tt.input)
            if result != tt.expected {
                t.Errorf("IsValidEmail(%s) = %v, want %v", tt.input, result, tt.expected)
            }
        })
    }
}

// Benchmark function
func BenchmarkUserCreation(b *testing.B) {
    service := NewUserService()

    b.ResetTimer()
    for i := 0; i < b.N; i++ {
        user := &User{Name: fmt.Sprintf("User %d", i)}
        service.CreateUser(user)
    }
}

// Example function
func ExampleUserService_CreateUser() {
    service := NewUserService()
    user := &User{Name: "John Doe"}

    err := service.CreateUser(user)
    if err != nil {
        fmt.Printf("Error: %v", err)
        return
    }

    fmt.Printf("User created with ID: %d", user.ID)
    // Output: User created with ID: 1
}

// Fuzzing test (Go 1.18+)
func FuzzUserValidation(f *testing.F) {
    f.Add("test@example.com")
    f.Add("invalid")
    f.Add("")

    f.Fuzz(func(t *testing.T, email string) {
        result := IsValidEmail(email)
        // Test that the function doesn't panic
        _ = result
    })
}
"#;
    let tree = init_parser(code, "go");
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = GoExtractor::new(
        "go".to_string(),
        "test.go".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);

    let test_user_service = symbols.iter().find(|s| s.name == "TestUserService");
    assert!(test_user_service.is_some());
    let test_user_service = test_user_service.unwrap();
    assert_eq!(test_user_service.kind, SymbolKind::Function);
    assert!(
        test_user_service
            .signature
            .as_ref()
            .unwrap()
            .contains("func TestUserService(t *testing.T)")
    );

    let test_validation = symbols.iter().find(|s| s.name == "TestValidation");
    assert!(test_validation.is_some());
    let test_validation = test_validation.unwrap();
    assert!(
        test_validation
            .signature
            .as_ref()
            .unwrap()
            .contains("func TestValidation(t *testing.T)")
    );

    let benchmark_user_creation = symbols.iter().find(|s| s.name == "BenchmarkUserCreation");
    assert!(benchmark_user_creation.is_some());
    let benchmark_user_creation = benchmark_user_creation.unwrap();
    assert!(
        benchmark_user_creation
            .signature
            .as_ref()
            .unwrap()
            .contains("func BenchmarkUserCreation(b *testing.B)")
    );

    let example_func = symbols
        .iter()
        .find(|s| s.name == "ExampleUserService_CreateUser");
    assert!(example_func.is_some());
    let example_func = example_func.unwrap();
    assert!(
        example_func
            .signature
            .as_ref()
            .unwrap()
            .contains("func ExampleUserService_CreateUser()")
    );

    let fuzz_func = symbols.iter().find(|s| s.name == "FuzzUserValidation");
    assert!(fuzz_func.is_some());
    let fuzz_func = fuzz_func.unwrap();
    assert!(
        fuzz_func
            .signature
            .as_ref()
            .unwrap()
            .contains("func FuzzUserValidation(f *testing.F)")
    );
}
