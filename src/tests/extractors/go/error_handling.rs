//! Go Error Handling Tests - Custom Error Types and Patterns
//!
//! Tests for extracting Go's error handling patterns:
//! - Custom error types (implementing error interface)
//! - Error() method implementation
//! - Unwrap() method for error chains
//! - Nested errors (error wrapping)
//! - Generic Result types with error handling
//! - Error constructors (Ok, Err)

use crate::extractors::base::SymbolKind;
use crate::extractors::go::GoExtractor;
use crate::tests::test_utils::init_parser;
use std::path::PathBuf;

#[test]
fn test_extract_custom_error_types_and_patterns() {
    let code = r#"
package main

import (
    "errors"
    "fmt"
)

// Custom error types
type ValidationError struct {
    Field   string
    Message string
    Code    int
}

func (e ValidationError) Error() string {
    return fmt.Sprintf("validation error on field '%s': %s (code: %d)", e.Field, e.Message, e.Code)
}

func (e ValidationError) Unwrap() error {
    return errors.New(e.Message)
}

// Custom error with nested error
type DatabaseError struct {
    Operation string
    Err       error
}

func (e DatabaseError) Error() string {
    return fmt.Sprintf("database %s failed: %v", e.Operation, e.Err)
}

func (e DatabaseError) Unwrap() error {
    return e.Err
}

// Result type for better error handling
type Result[T any] struct {
    Value T
    Err   error
}

func (r Result[T]) IsOk() bool {
    return r.Err == nil
}

func (r Result[T]) IsErr() bool {
    return r.Err != nil
}

func (r Result[T]) Unwrap() (T, error) {
    return r.Value, r.Err
}

// Ok creates a successful result
func Ok[T any](value T) Result[T] {
    return Result[T]{Value: value}
}

// Err creates an error result
func Err[T any](err error) Result[T] {
    var zero T
    return Result[T]{Value: zero, Err: err}
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

    let validation_error = symbols.iter().find(|s| s.name == "ValidationError");
    assert!(validation_error.is_some());
    let validation_error = validation_error.unwrap();
    assert_eq!(validation_error.kind, SymbolKind::Class);
    assert!(
        validation_error
            .signature
            .as_ref()
            .unwrap()
            .contains("type ValidationError struct")
    );

    let error_method = symbols.iter().find(|s| s.name == "Error");
    assert!(error_method.is_some());
    let error_method = error_method.unwrap();
    assert!(
        error_method
            .signature
            .as_ref()
            .unwrap()
            .contains("func (e ValidationError) Error() string")
    );

    let unwrap_method = symbols.iter().find(|s| s.name == "Unwrap");
    assert!(unwrap_method.is_some());
    let unwrap_method = unwrap_method.unwrap();
    assert!(
        unwrap_method
            .signature
            .as_ref()
            .unwrap()
            .contains("func (e ValidationError) Unwrap() error")
    );

    let database_error = symbols.iter().find(|s| s.name == "DatabaseError");
    assert!(database_error.is_some());
    let database_error = database_error.unwrap();
    assert_eq!(database_error.kind, SymbolKind::Class);

    let result_type = symbols.iter().find(|s| s.name == "Result");
    assert!(result_type.is_some());
    let result_type = result_type.unwrap();
    assert!(
        result_type
            .signature
            .as_ref()
            .unwrap()
            .contains("type Result[T any] struct")
    );

    let is_ok_method = symbols.iter().find(|s| s.name == "IsOk");
    assert!(is_ok_method.is_some());
    let is_ok_method = is_ok_method.unwrap();
    assert!(
        is_ok_method
            .signature
            .as_ref()
            .unwrap()
            .contains("func (r Result[T]) IsOk() bool")
    );

    let ok_func = symbols.iter().find(|s| s.name == "Ok");
    assert!(ok_func.is_some());
    let ok_func = ok_func.unwrap();
    assert!(
        ok_func
            .signature
            .as_ref()
            .unwrap()
            .contains("func Ok[T any](value T) Result[T]")
    );

    let err_func = symbols.iter().find(|s| s.name == "Err");
    assert!(err_func.is_some());
    let err_func = err_func.unwrap();
    assert!(
        err_func
            .signature
            .as_ref()
            .unwrap()
            .contains("func Err[T any](err error) Result[T]")
    );
}
