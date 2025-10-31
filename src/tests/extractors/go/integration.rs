//! Go Integration Tests - Comprehensive and Large File Handling
//!
//! Tests for extracting symbols from realistic Go code:
//! - Comprehensive feature coverage (structs, interfaces, methods, functions)
//! - Large files with many symbols (performance testing)
//! - Multi-struct/interface patterns
//! - Constants and variables
//! - HTTP handlers and services
//! - Generated code patterns

use crate::extractors::base::SymbolKind;
use crate::extractors::go::GoExtractor;
use crate::tests::test_utils::init_parser;
use std::path::PathBuf;

#[test]
fn test_handle_comprehensive_go_code() {
    let code = r#"
package main

import (
    "context"
    "fmt"
    "net/http"
)

// User represents a user in the system
type User struct {
    ID       int64  `json:"id"`
    Name     string `json:"name"`
    Email    string `json:"email,omitempty"`
}

// UserService interface for user operations
type UserService interface {
    GetUser(ctx context.Context, id int64) (*User, error)
    CreateUser(ctx context.Context, user *User) error
}

// UserRepository implements UserService
type UserRepository struct {
    db *sql.DB
}

// GetUser retrieves a user by ID
func (r *UserRepository) GetUser(ctx context.Context, id int64) (*User, error) {
    // Implementation here
    return nil, nil
}

// CreateUser creates a new user
func (r *UserRepository) CreateUser(ctx context.Context, user *User) error {
    // Implementation here
    return nil
}

// ProcessUsers processes users concurrently
func ProcessUsers(users []User) <-chan User {
    resultCh := make(chan User, len(users))

    go func() {
        defer close(resultCh)
        for _, user := range users {
            resultCh <- user
        }
    }()

    return resultCh
}

const (
    MaxUsers = 1000
    DefaultTimeout = 30
)

var (
    GlobalConfig *Config
    Logger       *log.Logger
)

func main() {
    http.HandleFunc("/users", handleUsers)
    fmt.Println("Server starting")
    http.ListenAndServe(":8080", nil)
}

func handleUsers(w http.ResponseWriter, r *http.Request) {
    // HTTP handler implementation
}
"#;
    let tree = init_parser(code, "go");
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor =
        GoExtractor::new("go".to_string(), "test.go".to_string(), code.to_string(), &workspace_root);
    let symbols = extractor.extract_symbols(&tree);

    // Check we extracted all major symbols
    assert!(symbols.iter().any(|s| s.name == "main"));
    assert!(symbols.iter().any(|s| s.name == "User"));
    assert!(symbols.iter().any(|s| s.name == "UserService"));
    assert!(symbols.iter().any(|s| s.name == "UserRepository"));
    assert!(symbols.iter().any(|s| s.name == "GetUser"));
    assert!(symbols.iter().any(|s| s.name == "CreateUser"));
    assert!(symbols.iter().any(|s| s.name == "ProcessUsers"));
    assert!(symbols.iter().any(|s| s.name == "MaxUsers"));
    assert!(symbols.iter().any(|s| s.name == "GlobalConfig"));
    assert!(symbols.iter().any(|s| s.name == "handleUsers"));

    // Check specific features
    let user_struct = symbols.iter().find(|s| s.name == "User");
    assert!(user_struct.is_some());
    assert_eq!(user_struct.unwrap().kind, SymbolKind::Class);

    let user_service = symbols.iter().find(|s| s.name == "UserService");
    assert!(user_service.is_some());
    assert_eq!(user_service.unwrap().kind, SymbolKind::Interface);

    let get_user = symbols.iter().find(|s| s.name == "GetUser");
    assert!(get_user.is_some());
    assert_eq!(get_user.unwrap().kind, SymbolKind::Method);
    assert!(get_user
        .unwrap()
        .signature
        .as_ref()
        .unwrap()
        .contains("func (r *UserRepository) GetUser"));

    let process_users = symbols.iter().find(|s| s.name == "ProcessUsers");
    assert!(process_users.is_some());
    assert!(process_users
        .unwrap()
        .signature
        .as_ref()
        .unwrap()
        .contains("<-chan User"));
}

#[test]
fn test_handle_large_go_files_with_many_symbols() {
    // Generate a large Go file with many types and functions
    let mut types = String::new();
    for i in 0..20 {
        types.push_str(&format!(
            r#"
// Service{i} represents service {i}
type Service{i} struct {{
    ID     int64
    Name   string
    Config map[string]interface{{}}
    Active bool
}}

func (s *Service{i}) Start() error {{
    s.Active = true
    return nil
}}

func (s *Service{i}) Stop() error {{
    s.Active = false
    return nil
}}

func (s *Service{i}) GetStatus() string {{
    if s.Active {{
        return "running"
    }}
    return "stopped"
}}

func NewService{i}(name string) *Service{i} {{
    return &Service{i}{{
        ID:     {i},
        Name:   name,
        Config: make(map[string]interface{{}}),
        Active: false,
    }}
}}
"#,
            i = i
        ));
    }

    let code = format!(
        r#"
package main

import (
    "context"
    "fmt"
    "sync"
    "time"
)

// Constants
const (
    MaxConnections = 1000
    DefaultTimeout = 30 * time.Second
    Version        = "1.0.0"
    BuildDate      = "2023-01-01"
)

// Global variables
var (
    GlobalCounter int64
    GlobalMutex   sync.RWMutex
    GlobalConfig  map[string]interface{{}}
    Logger        *CustomLogger
)

// Common types
type ProcessedData struct {{
    ID   int
    Data []byte
}}

type Result struct {{
    Success bool
    Message string
    Data    interface{{}}
}}

{types}

func main() {{
    fmt.Println("Application started")
}}
"#,
        types = types
    );

    let tree = init_parser(&code, "go");
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor =
        GoExtractor::new("go".to_string(), "test.go".to_string(), code.to_string(), &workspace_root);
    let symbols = extractor.extract_symbols(&tree);
    let _relationships = extractor.extract_relationships(&tree, &symbols);

    // Should extract many symbols
    assert!(symbols.len() > 100);

    // Check that all generated services were extracted
    for i in 0..20 {
        let service_name = format!("Service{}", i);
        let service = symbols.iter().find(|s| s.name == service_name);
        assert!(service.is_some());
        assert_eq!(service.unwrap().kind, SymbolKind::Class);
    }
}
