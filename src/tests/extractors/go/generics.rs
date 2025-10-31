//! Generic Types and Constraints Tests for Go Extractor
//!
//! Tests for extracting Go 1.18+ generic features:
//! - Generic constraint interfaces
//! - Generic structs with type parameters
//! - Generic methods
//! - Generic functions with multiple type parameters
//! - Type constraint validation

use crate::extractors::base::SymbolKind;
use crate::extractors::go::GoExtractor;
use crate::tests::test_utils::init_parser;
use std::path::PathBuf;

#[test]
fn test_extract_generic_types_and_constraints() {
    let code = r#"
package main

import "fmt"

// Generic constraint interface
type Ordered interface {
    ~int | ~int8 | ~int16 | ~int32 | ~int64 |
    ~uint | ~uint8 | ~uint16 | ~uint32 | ~uint64 | ~uintptr |
    ~float32 | ~float64 |
    ~string
}

// Generic struct with type parameter
type Stack[T any] struct {
    items []T
}

// Generic method
func (s *Stack[T]) Push(item T) {
    s.items = append(s.items, item)
}

// Generic function with multiple type parameters
func Map[T any, U any](slice []T, f func(T) U) []U {
    result := make([]U, len(slice))
    for i, v := range slice {
        result[i] = f(v)
    }
    return result
}

// Generic function with constraint
func Min[T Ordered](a, b T) T {
    if a < b {
        return a
    }
    return b
}

// Complex generic type
type Pair[K comparable, V any] struct {
    Key   K
    Value V
}

// Generic interface
type Getter[T any] interface {
    Get() T
}

// Implementation of generic interface
type IntGetter int

func (i IntGetter) Get() int {
    return int(i)
}

// Generic function with type switch
func PrintType[T any](v T) {
    switch any(v).(type) {
    case int:
        fmt.Println("int")
    case string:
        fmt.Println("string")
    default:
        fmt.Println("unknown")
    }
}

// Nested generic types
type Result[T any, E error] struct {
    value T
    err   E
}

func main() {
    // Usage examples
    stack := Stack[int]{}
    stack.Push(1)

    numbers := []int{1, 2, 3}
    strings := Map(numbers, func(n int) string {
        return fmt.Sprintf("%d", n)
    })

    min := Min(5, 10)

    pair := Pair[string, int]{Key: "age", Value: 30}

    var getter Getter[int] = IntGetter(42)
    value := getter.Get()

    PrintType(42)
    PrintType("hello")

    result := Result[int, error]{value: 42, err: nil}

    fmt.Println(strings, min, pair, value, result)
}
"#;
    let tree = init_parser(code, "go");
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor =
        GoExtractor::new("go".to_string(), "test.go".to_string(), code.to_string(), &workspace_root);
    let symbols = extractor.extract_symbols(&tree);

    // Verify generic constraint interface
    let ordered_interface = symbols.iter().find(|s| s.name == "Ordered");
    assert!(
        ordered_interface.is_some(),
        "Should extract Ordered constraint interface"
    );
    let ordered = ordered_interface.unwrap();
    assert_eq!(ordered.kind, SymbolKind::Interface);

    // Verify generic struct
    let stack_struct = symbols.iter().find(|s| s.name == "Stack");
    assert!(stack_struct.is_some(), "Should extract Stack generic struct");
    let stack = stack_struct.unwrap();
    assert_eq!(stack.kind, SymbolKind::Class);
    assert!(
        stack.signature.as_ref().unwrap().contains("[T any]"),
        "Stack signature should include type parameter"
    );

    // Verify generic method
    let push_method = symbols.iter().find(|s| s.name == "Push");
    assert!(
        push_method.is_some(),
        "Should extract Push generic method"
    );

    // Verify generic functions
    let map_func = symbols.iter().find(|s| s.name == "Map");
    assert!(
        map_func.is_some(),
        "Should extract Map generic function"
    );

    let min_func = symbols.iter().find(|s| s.name == "Min");
    assert!(
        min_func.is_some(),
        "Should extract Min generic function with constraint"
    );

    // Verify complex generic types
    let pair_struct = symbols.iter().find(|s| s.name == "Pair");
    assert!(
        pair_struct.is_some(),
        "Should extract Pair generic struct"
    );

    let getter_interface = symbols.iter().find(|s| s.name == "Getter");
    assert!(
        getter_interface.is_some(),
        "Should extract Getter generic interface"
    );

    let result_struct = symbols.iter().find(|s| s.name == "Result");
    assert!(
        result_struct.is_some(),
        "Should extract Result generic struct"
    );
}
