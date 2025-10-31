//! Go Type Assertions and Interfaces Tests
//!
//! Tests for extracting Go's interface and type assertion features:
//! - Basic interfaces (Reader, Writer, Closer)
//! - Composed interfaces (embedding)
//! - Empty interface (interface{}) usage
//! - Type assertions (value.(Type))
//! - Type switches (switch value.(type))
//! - Interface method declarations

use crate::extractors::base::SymbolKind;
use crate::extractors::go::GoExtractor;
use crate::tests::test_utils::init_parser;
use std::path::PathBuf;

#[test]
fn test_extract_interfaces_type_assertions_and_switches() {
    let code = r#"
package main

import (
    "fmt"
    "reflect"
)

// Basic interfaces
type Reader interface {
    Read([]byte) (int, error)
}

type Writer interface {
    Write([]byte) (int, error)
}

type Closer interface {
    Close() error
}

// Composed interface
type ReadWriteCloser interface {
    Reader
    Writer
    Closer
}

// Interface with type constraints
type Stringer interface {
    String() string
}

// Empty interface usage
type Container struct {
    Value interface{}
}

func (c *Container) Set(value interface{}) {
    c.Value = value
}

func (c *Container) Get() interface{} {
    return c.Value
}

func (c *Container) GetString() (string, bool) {
    if str, ok := c.Value.(string); ok {
        return str, true
    }
    return "", false
}

// Type assertion and type switches
func ProcessValue(value interface{}) string {
    switch v := value.(type) {
    case string:
        return fmt.Sprintf("String: %s", v)
    case int:
        return fmt.Sprintf("Integer: %d", v)
    case float64:
        return fmt.Sprintf("Float: %.2f", v)
    case bool:
        return fmt.Sprintf("Boolean: %t", v)
    case nil:
        return "Nil value"
    case Stringer:
        return fmt.Sprintf("Stringer: %s", v.String())
    default:
        return fmt.Sprintf("Unknown type: %T", v)
    }
}
"#;
    let tree = init_parser(code, "go");
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor =
        GoExtractor::new("go".to_string(), "test.go".to_string(), code.to_string(), &workspace_root);
    let symbols = extractor.extract_symbols(&tree);

    let reader = symbols.iter().find(|s| s.name == "Reader");
    assert!(reader.is_some());
    let reader = reader.unwrap();
    assert_eq!(reader.kind, SymbolKind::Interface);
    assert!(reader
        .signature
        .as_ref()
        .unwrap()
        .contains("type Reader interface"));

    let writer = symbols.iter().find(|s| s.name == "Writer");
    assert!(writer.is_some());
    let writer = writer.unwrap();
    assert_eq!(writer.kind, SymbolKind::Interface);

    let closer = symbols.iter().find(|s| s.name == "Closer");
    assert!(closer.is_some());
    let closer = closer.unwrap();
    assert_eq!(closer.kind, SymbolKind::Interface);

    let read_write_closer = symbols.iter().find(|s| s.name == "ReadWriteCloser");
    assert!(read_write_closer.is_some());
    let read_write_closer = read_write_closer.unwrap();
    assert_eq!(read_write_closer.kind, SymbolKind::Interface);
    assert!(read_write_closer
        .signature
        .as_ref()
        .unwrap()
        .contains("type ReadWriteCloser interface"));

    let container = symbols.iter().find(|s| s.name == "Container");
    assert!(container.is_some());
    let container = container.unwrap();
    assert_eq!(container.kind, SymbolKind::Class);
    assert!(container
        .signature
        .as_ref()
        .unwrap()
        .contains("type Container struct"));

    let process_value = symbols.iter().find(|s| s.name == "ProcessValue");
    assert!(process_value.is_some());
    let process_value = process_value.unwrap();
    assert!(process_value
        .signature
        .as_ref()
        .unwrap()
        .contains("func ProcessValue(value interface{}) string"));
}
