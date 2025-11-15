//! Go Edge Cases and Malformed Code Tests
//!
//! Tests for handling unusual and edge case Go constructs:
//! - Empty structs
//! - Embedded types in structs
//! - Embedded interfaces
//! - Complex function signatures
//! - Named return values
//! - Malformed code (missing braces)
//! - Variadic functions
//! - Function types
//! - Channel types (bidirectional, send-only, receive-only)
//! - Pointer vs value receivers
//! - Type aliases vs type definitions
//! - init() functions
//! - Multiple variable declarations

use crate::extractors::base::SymbolKind;
use crate::extractors::go::GoExtractor;
use crate::tests::test_utils::init_parser;
use std::path::PathBuf;

#[test]
fn test_handle_edge_cases_and_malformed_code() {
    let code = r#"
package main

// Edge cases and unusual Go constructs

// Empty struct
type Empty struct{}

// Struct with embedded types
type EmbeddedStruct struct {
    Empty
    *User
    io.Reader
    value int
}

// Interface with embedded interfaces
type ComplexInterface interface {
    io.Reader
    io.Writer
    fmt.Stringer
    CustomMethod() error
}

// Function with complex signature
func ComplexFunction(
    ctx context.Context,
    args ...interface{},
) (result chan<- string, cleanup func() error, err error) {
    return nil, nil, nil
}

// Function with named return values
func NamedReturns(x, y int) (sum, product int) {
    sum = x + y
    product = x * y
    return // naked return
}

// Malformed code that shouldn't crash parser
type MissingBrace struct {
    field int
// Missing closing brace

// Variadic function
func VariadicFunction(format string, args ...interface{}) {
    fmt.Printf(format, args...)
}

// Function type
type HandlerFunc func(http.ResponseWriter, *http.Request)

// Channel types
type Channels struct {
    input    <-chan string
    output   chan<- int
    bidirect chan bool
}

// Method with pointer receiver vs value receiver
func (e Empty) ValueMethod() {}
func (e *Empty) PointerMethod() {}

// Type alias vs type definition
type TypeAlias = string
type TypeDefinition string

// Package-level function with init
func init() {
    // Initialization code
}

// Multiple variable declarations
var a, b, c int
var (
    x = 1
    y = 2
    z string
)
"#;
    let tree = init_parser(code, "go");
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = GoExtractor::new(
        "go".to_string(),
        "test.go".to_string(),
        code.to_string(),
        &workspace_root,
    );

    // Should not panic even with malformed code
    let symbols = extractor.extract_symbols(&tree);
    let _relationships = extractor.extract_relationships(&tree, &symbols);

    // Should still extract valid symbols
    let empty = symbols.iter().find(|s| s.name == "Empty");
    assert!(empty.is_some());
    assert_eq!(empty.unwrap().kind, SymbolKind::Class);

    let embedded_struct = symbols.iter().find(|s| s.name == "EmbeddedStruct");
    assert!(embedded_struct.is_some());
    assert_eq!(embedded_struct.unwrap().kind, SymbolKind::Class);

    let complex_interface = symbols.iter().find(|s| s.name == "ComplexInterface");
    assert!(complex_interface.is_some());
    assert_eq!(complex_interface.unwrap().kind, SymbolKind::Interface);

    let complex_function = symbols.iter().find(|s| s.name == "ComplexFunction");
    assert!(complex_function.is_some());
    assert!(
        complex_function
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("func ComplexFunction")
    );

    let named_returns = symbols.iter().find(|s| s.name == "NamedReturns");
    assert!(named_returns.is_some());
    assert!(
        named_returns
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("(sum, product int)")
    );

    let variadic_func = symbols.iter().find(|s| s.name == "VariadicFunction");
    assert!(variadic_func.is_some());
    assert!(
        variadic_func
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("args ...interface{}")
    );

    let handler_func = symbols.iter().find(|s| s.name == "HandlerFunc");
    assert!(handler_func.is_some());
    assert_eq!(handler_func.unwrap().kind, SymbolKind::Type);

    let channels = symbols.iter().find(|s| s.name == "Channels");
    assert!(channels.is_some());
    assert_eq!(channels.unwrap().kind, SymbolKind::Class);

    let type_alias = symbols.iter().find(|s| s.name == "TypeAlias");
    assert!(type_alias.is_some());
    assert_eq!(type_alias.unwrap().kind, SymbolKind::Type);

    let type_definition = symbols.iter().find(|s| s.name == "TypeDefinition");
    assert!(type_definition.is_some());
    assert_eq!(type_definition.unwrap().kind, SymbolKind::Type);

    let init_func = symbols.iter().find(|s| s.name == "init");
    assert!(init_func.is_some());
    assert_eq!(init_func.unwrap().kind, SymbolKind::Function);
}
