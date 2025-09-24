#[cfg(test)]
mod go_extractor_tests {
    use crate::extractors::go::GoExtractor;
    use crate::extractors::base::{SymbolKind, Visibility};
    use crate::tests::test_utils::init_parser;

    #[test]
    fn test_extract_package_declarations() {
        let code = r#"
package main

package utils
"#;
        let tree = init_parser(code, "go");
        let mut extractor = GoExtractor::new("go".to_string(), "test.go".to_string(), code.to_string());
        let symbols = extractor.extract_symbols(&tree);

        let main_package = symbols.iter().find(|s| s.name == "main");
        assert!(main_package.is_some());
        let main_package = main_package.unwrap();
        assert_eq!(main_package.kind, SymbolKind::Namespace);
        assert_eq!(main_package.signature.as_ref().unwrap(), "package main");
        assert_eq!(main_package.visibility, Some(Visibility::Public));
    }

    #[test]
    fn test_extract_struct_definitions() {
        let code = r#"
package main

type User struct {
    ID    int64  `json:"id"`
    Name  string `json:"name"`
    Email string `json:"email,omitempty"`
}

type Point struct {
    X, Y float64
}
"#;
        let tree = init_parser(code, "go");
        let mut extractor = GoExtractor::new("go".to_string(), "test.go".to_string(), code.to_string());
        let symbols = extractor.extract_symbols(&tree);

        let user_struct = symbols.iter().find(|s| s.name == "User");
        assert!(user_struct.is_some());
        let user_struct = user_struct.unwrap();
        assert_eq!(user_struct.kind, SymbolKind::Class);
        assert_eq!(user_struct.signature.as_ref().unwrap(), "type User struct");
        assert_eq!(user_struct.visibility, Some(Visibility::Public));

        let point_struct = symbols.iter().find(|s| s.name == "Point");
        assert!(point_struct.is_some());
        let point_struct = point_struct.unwrap();
        assert_eq!(point_struct.visibility, Some(Visibility::Public));
    }

    #[test]
    fn test_extract_interface_definitions() {
        let code = r#"
package main

type UserService interface {
    GetUser(id int64) (*User, error)
    CreateUser(user *User) error
    UpdateUser(user *User) error
}

type Reader interface {
    Read([]byte) (int, error)
}
"#;
        let tree = init_parser(code, "go");
        let mut extractor = GoExtractor::new("go".to_string(), "test.go".to_string(), code.to_string());
        let symbols = extractor.extract_symbols(&tree);

        let user_service = symbols.iter().find(|s| s.name == "UserService");
        assert!(user_service.is_some());
        let user_service = user_service.unwrap();
        assert_eq!(user_service.kind, SymbolKind::Interface);
        assert_eq!(user_service.signature.as_ref().unwrap(), "type UserService interface");
        assert_eq!(user_service.visibility, Some(Visibility::Public));

        let reader = symbols.iter().find(|s| s.name == "Reader");
        assert!(reader.is_some());
        let reader = reader.unwrap();
        assert_eq!(reader.kind, SymbolKind::Interface);
    }

    #[test]
    fn test_extract_type_aliases() {
        let code = r#"
package main

type UserID int64
type Username string
type Config map[string]interface{}
"#;
        let tree = init_parser(code, "go");
        let mut extractor = GoExtractor::new("go".to_string(), "test.go".to_string(), code.to_string());
        let symbols = extractor.extract_symbols(&tree);

        let user_id = symbols.iter().find(|s| s.name == "UserID");
        assert!(user_id.is_some());
        let user_id = user_id.unwrap();
        assert_eq!(user_id.kind, SymbolKind::Type);
        assert!(user_id.signature.as_ref().unwrap().contains("type UserID = int64"));

        let config = symbols.iter().find(|s| s.name == "Config");
        assert!(config.is_some());
        let config = config.unwrap();
        assert!(config.signature.as_ref().unwrap().contains("type Config = map[string]interface{}"));
    }

    #[test]
    fn test_extract_standalone_functions() {
        let code = r#"
package main

func Add(a, b int) int {
    return a + b
}

func ProcessUsers(users []User) (<-chan User, error) {
    resultCh := make(chan User)
    return resultCh, nil
}

func main() {
    fmt.Println("Hello, World!")
}

func privateHelper() {
    // private function
}
"#;
        let tree = init_parser(code, "go");
        let mut extractor = GoExtractor::new("go".to_string(), "test.go".to_string(), code.to_string());
        let symbols = extractor.extract_symbols(&tree);

        let add_func = symbols.iter().find(|s| s.name == "Add");
        assert!(add_func.is_some());
        let add_func = add_func.unwrap();
        assert_eq!(add_func.kind, SymbolKind::Function);
        assert!(add_func.signature.as_ref().unwrap().contains("func Add(a, b int) int"));
        assert_eq!(add_func.visibility, Some(Visibility::Public));

        let process_func = symbols.iter().find(|s| s.name == "ProcessUsers");
        assert!(process_func.is_some());
        let process_func = process_func.unwrap();
        assert!(process_func.signature.as_ref().unwrap().contains("func ProcessUsers"));
        assert!(process_func.signature.as_ref().unwrap().contains("<-chan User"));

        let main_func = symbols.iter().find(|s| s.name == "main" && s.kind == SymbolKind::Function);
        assert!(main_func.is_some());
        let main_func = main_func.unwrap();
        assert_eq!(main_func.visibility, Some(Visibility::Private));

        let private_func = symbols.iter().find(|s| s.name == "privateHelper");
        assert!(private_func.is_some());
        let private_func = private_func.unwrap();
        assert_eq!(private_func.visibility, Some(Visibility::Private));
    }

    #[test]
    fn test_extract_methods_with_receivers() {
        let code = r#"
package main

type User struct {
    Name string
    Age  int
}

func (u *User) GetName() string {
    return u.Name
}

func (u User) IsAdult() bool {
    return u.Age >= 18
}

func (u *User) SetName(name string) {
    u.Name = name
}
"#;
        let tree = init_parser(code, "go");
        let mut extractor = GoExtractor::new("go".to_string(), "test.go".to_string(), code.to_string());
        let symbols = extractor.extract_symbols(&tree);

        let get_name = symbols.iter().find(|s| s.name == "GetName");
        assert!(get_name.is_some());
        let get_name = get_name.unwrap();
        assert_eq!(get_name.kind, SymbolKind::Method);
        assert!(get_name.signature.as_ref().unwrap().contains("func (u *User) GetName() string"));
        assert_eq!(get_name.visibility, Some(Visibility::Public));

        let is_adult = symbols.iter().find(|s| s.name == "IsAdult");
        assert!(is_adult.is_some());
        let is_adult = is_adult.unwrap();
        assert!(is_adult.signature.as_ref().unwrap().contains("func (u User) IsAdult() bool"));

        let set_name = symbols.iter().find(|s| s.name == "SetName");
        assert!(set_name.is_some());
        let set_name = set_name.unwrap();
        assert!(set_name.signature.as_ref().unwrap().contains("func (u *User) SetName(name string)"));
    }

    #[test]
    fn test_extract_import_declarations() {
        let code = r#"
package main

import "fmt"
import "net/http"

import (
    "context"
    "encoding/json"
    log "github.com/sirupsen/logrus"
    _ "github.com/lib/pq"
)
"#;
        let tree = init_parser(code, "go");
        let mut extractor = GoExtractor::new("go".to_string(), "test.go".to_string(), code.to_string());
        let symbols = extractor.extract_symbols(&tree);

        let fmt_import = symbols.iter().find(|s| s.name == "fmt");
        assert!(fmt_import.is_some());
        let fmt_import = fmt_import.unwrap();
        assert_eq!(fmt_import.kind, SymbolKind::Import);
        assert!(fmt_import.signature.as_ref().unwrap().contains("import \"fmt\""));

        let http_import = symbols.iter().find(|s| s.name == "http");
        assert!(http_import.is_some());
        let http_import = http_import.unwrap();
        assert!(http_import.signature.as_ref().unwrap().contains("import \"net/http\""));

        let log_import = symbols.iter().find(|s| s.name == "log");
        assert!(log_import.is_some());
        let log_import = log_import.unwrap();
        assert!(log_import.signature.as_ref().unwrap().contains("import log \"github.com/sirupsen/logrus\""));
    }

    #[test]
    fn test_extract_constant_declarations() {
        let code = r#"
package main

const MaxUsers = 1000
const DefaultTimeout = 30

const (
    StatusActive   = "active"
    StatusInactive = "inactive"
    StatusPending  = "pending"
)
"#;
        let tree = init_parser(code, "go");
        let mut extractor = GoExtractor::new("go".to_string(), "test.go".to_string(), code.to_string());
        let symbols = extractor.extract_symbols(&tree);

        let max_users = symbols.iter().find(|s| s.name == "MaxUsers");
        assert!(max_users.is_some());
        let max_users = max_users.unwrap();
        assert_eq!(max_users.kind, SymbolKind::Constant);
        assert!(max_users.signature.as_ref().unwrap().contains("const MaxUsers = 1000"));
        assert_eq!(max_users.visibility, Some(Visibility::Public));

        let status_active = symbols.iter().find(|s| s.name == "StatusActive");
        assert!(status_active.is_some());
        let status_active = status_active.unwrap();
        assert!(status_active.signature.as_ref().unwrap().contains("const StatusActive = \"active\""));
    }

    #[test]
    fn test_extract_variable_declarations() {
        let code = r#"
package main

var GlobalConfig *Config
var Logger *log.Logger

var (
    Version    = "1.0.0"
    BuildTime  string
    debugMode  bool
)
"#;
        let tree = init_parser(code, "go");
        let mut extractor = GoExtractor::new("go".to_string(), "test.go".to_string(), code.to_string());
        let symbols = extractor.extract_symbols(&tree);

        let global_config = symbols.iter().find(|s| s.name == "GlobalConfig");
        assert!(global_config.is_some());
        let global_config = global_config.unwrap();
        assert_eq!(global_config.kind, SymbolKind::Variable);
        assert!(global_config.signature.as_ref().unwrap().contains("var GlobalConfig *Config"));
        assert_eq!(global_config.visibility, Some(Visibility::Public));

        let version = symbols.iter().find(|s| s.name == "Version");
        assert!(version.is_some());
        let version = version.unwrap();
        assert!(version.signature.as_ref().unwrap().contains("var Version = \"1.0.0\""));

        let debug_mode = symbols.iter().find(|s| s.name == "debugMode");
        assert!(debug_mode.is_some());
        let debug_mode = debug_mode.unwrap();
        assert_eq!(debug_mode.visibility, Some(Visibility::Private));
    }

    #[test]
    fn test_handle_channel_types_and_goroutines() {
        let code = r#"
package main

func ProcessData(input <-chan string) chan string {
    output := make(chan string)

    go func() {
        defer close(output)
        for data := range input {
            processed := processItem(data)
            output <- processed
        }
    }()

    return output
}

func SendData(ch chan<- string, data string) {
    ch <- data
}
"#;
        let tree = init_parser(code, "go");
        let mut extractor = GoExtractor::new("go".to_string(), "test.go".to_string(), code.to_string());
        let symbols = extractor.extract_symbols(&tree);

        let process_data = symbols.iter().find(|s| s.name == "ProcessData");
        assert!(process_data.is_some());
        let process_data = process_data.unwrap();
        assert!(process_data.signature.as_ref().unwrap().contains("<-chan string"));
        assert!(process_data.signature.as_ref().unwrap().contains("chan string"));

        let send_data = symbols.iter().find(|s| s.name == "SendData");
        assert!(send_data.is_some());
        let send_data = send_data.unwrap();
        assert!(send_data.signature.as_ref().unwrap().contains("chan<- string"));
    }

    #[test]
    fn test_infer_types_from_go_annotations() {
        let code = r#"
package main

func GetName() string {
    return "test"
}

func Calculate(x, y int) float64 {
    return float64(x + y)
}

var Count int = 42
var Message string = "hello"
"#;
        let tree = init_parser(code, "go");
        let mut extractor = GoExtractor::new("go".to_string(), "test.go".to_string(), code.to_string());
        let symbols = extractor.extract_symbols(&tree);
        let types = extractor.infer_types(&symbols);

        let get_name = symbols.iter().find(|s| s.name == "GetName");
        assert!(get_name.is_some());
        let get_name = get_name.unwrap();
        assert_eq!(types.get(&get_name.id), Some(&"string".to_string()));

        let calculate = symbols.iter().find(|s| s.name == "Calculate");
        assert!(calculate.is_some());
        let calculate = calculate.unwrap();
        assert_eq!(types.get(&calculate.id), Some(&"float64".to_string()));

        let count = symbols.iter().find(|s| s.name == "Count");
        assert!(count.is_some());
        let count = count.unwrap();
        assert_eq!(types.get(&count.id), Some(&"int".to_string()));
    }

    #[test]
    fn test_extract_method_receiver_relationships() {
        let code = r#"
package main

type User struct {
    Name string
}

func (u *User) GetName() string {
    return u.Name
}

func (u *User) SetName(name string) {
    u.Name = name
}
"#;
        let tree = init_parser(code, "go");
        let mut extractor = GoExtractor::new("go".to_string(), "test.go".to_string(), code.to_string());
        let symbols = extractor.extract_symbols(&tree);
        let relationships = extractor.extract_relationships(&tree, &symbols);

        // Should find method-receiver relationships
        assert!(!relationships.is_empty());
    }

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

func (s *Stack[T]) Pop() (T, bool) {
    if len(s.items) == 0 {
        var zero T
        return zero, false
    }
    index := len(s.items) - 1
    item := s.items[index]
    s.items = s.items[:index]
    return item, true
}

// Generic function with constraints
func Max[T Ordered](a, b T) T {
    if a > b {
        return a
    }
    return b
}

// Generic interface
type Comparable[T any] interface {
    Compare(other T) int
}

// Generic map utility
func Map[T, U any](slice []T, fn func(T) U) []U {
    result := make([]U, len(slice))
    for i, v := range slice {
        result[i] = fn(v)
    }
    return result
}

// Type union constraint
type Numeric interface {
    int | int32 | int64 | float32 | float64
}

func Sum[T Numeric](values ...T) T {
    var sum T
    for _, v := range values {
        sum += v
    }
    return sum
}
"#;
        let tree = init_parser(code, "go");
        let mut extractor = GoExtractor::new("go".to_string(), "test.go".to_string(), code.to_string());
        let symbols = extractor.extract_symbols(&tree);

        let ordered = symbols.iter().find(|s| s.name == "Ordered");
        assert!(ordered.is_some());
        let ordered = ordered.unwrap();
        assert_eq!(ordered.kind, SymbolKind::Interface);
        assert!(ordered.signature.as_ref().unwrap().contains("type Ordered interface"));

        let stack = symbols.iter().find(|s| s.name == "Stack");
        assert!(stack.is_some());
        let stack = stack.unwrap();
        assert_eq!(stack.kind, SymbolKind::Class);
        assert!(stack.signature.as_ref().unwrap().contains("type Stack[T any] struct"));

        let push_method = symbols.iter().find(|s| s.name == "Push");
        assert!(push_method.is_some());
        let push_method = push_method.unwrap();
        assert!(push_method.signature.as_ref().unwrap().contains("func (s *Stack[T]) Push(item T)"));

        let pop_method = symbols.iter().find(|s| s.name == "Pop");
        assert!(pop_method.is_some());
        let pop_method = pop_method.unwrap();
        assert!(pop_method.signature.as_ref().unwrap().contains("func (s *Stack[T]) Pop() (T, bool)"));

        let max_func = symbols.iter().find(|s| s.name == "Max");
        assert!(max_func.is_some());
        let max_func = max_func.unwrap();
        assert!(max_func.signature.as_ref().unwrap().contains("func Max[T Ordered](a, b T) T"));

        let comparable = symbols.iter().find(|s| s.name == "Comparable");
        assert!(comparable.is_some());
        let comparable = comparable.unwrap();
        assert!(comparable.signature.as_ref().unwrap().contains("type Comparable[T any] interface"));

        let map_func = symbols.iter().find(|s| s.name == "Map");
        assert!(map_func.is_some());
        let map_func = map_func.unwrap();
        assert!(map_func.signature.as_ref().unwrap().contains("func Map[T, U any]"));

        let numeric = symbols.iter().find(|s| s.name == "Numeric");
        assert!(numeric.is_some());
        let numeric = numeric.unwrap();
        assert!(numeric.signature.as_ref().unwrap().contains("int | int32 | int64 | float32 | float64"));

        let sum_func = symbols.iter().find(|s| s.name == "Sum");
        assert!(sum_func.is_some());
        let sum_func = sum_func.unwrap();
        assert!(sum_func.signature.as_ref().unwrap().contains("func Sum[T Numeric](values ...T) T"));
    }

    #[test]
    fn test_extract_concurrency_primitives_and_patterns() {
        let code = r#"
package main

import (
    "context"
    "sync"
    "time"
)

// WorkerPool represents a pool of workers
type WorkerPool struct {
    workers    int
    jobQueue   chan Job
    resultCh   chan Result
    wg         sync.WaitGroup
    mu         sync.RWMutex
    done       chan struct{}
    ctx        context.Context
    cancel     context.CancelFunc
}

// Job represents work to be done
type Job struct {
    ID   int
    Data interface{}
}

// Result represents the result of a job
type Result struct {
    JobID int
    Data  interface{}
    Error error
}

// NewWorkerPool creates a new worker pool
func NewWorkerPool(workers int, bufferSize int) *WorkerPool {
    ctx, cancel := context.WithCancel(context.Background())
    return &WorkerPool{
        workers:  workers,
        jobQueue: make(chan Job, bufferSize),
        resultCh: make(chan Result, bufferSize),
        done:     make(chan struct{}),
        ctx:      ctx,
        cancel:   cancel,
    }
}

// Start starts the worker pool
func (wp *WorkerPool) Start() {
    for i := 0; i < wp.workers; i++ {
        wp.wg.Add(1)
        go wp.worker(i)
    }
}

// worker is the main worker goroutine
func (wp *WorkerPool) worker(id int) {
    defer wp.wg.Done()

    for {
        select {
        case job := <-wp.jobQueue:
            result := wp.processJob(job)
            select {
            case wp.resultCh <- result:
            case <-wp.ctx.Done():
                return
            }
        case <-wp.ctx.Done():
            return
        }
    }
}
"#;
        let tree = init_parser(code, "go");
        let mut extractor = GoExtractor::new("go".to_string(), "test.go".to_string(), code.to_string());
        let symbols = extractor.extract_symbols(&tree);

        let worker_pool = symbols.iter().find(|s| s.name == "WorkerPool");
        assert!(worker_pool.is_some());
        let worker_pool = worker_pool.unwrap();
        assert_eq!(worker_pool.kind, SymbolKind::Class);
        assert!(worker_pool.signature.as_ref().unwrap().contains("type WorkerPool struct"));

        let job = symbols.iter().find(|s| s.name == "Job");
        assert!(job.is_some());
        let job = job.unwrap();
        assert_eq!(job.kind, SymbolKind::Class);

        let result_symbol = symbols.iter().find(|s| s.name == "Result");
        assert!(result_symbol.is_some());
        let result_symbol = result_symbol.unwrap();
        assert_eq!(result_symbol.kind, SymbolKind::Class);

        let new_worker_pool = symbols.iter().find(|s| s.name == "NewWorkerPool");
        assert!(new_worker_pool.is_some());
        let new_worker_pool = new_worker_pool.unwrap();
        assert!(new_worker_pool.signature.as_ref().unwrap().contains("func NewWorkerPool(workers int, bufferSize int) *WorkerPool"));

        let start_method = symbols.iter().find(|s| s.name == "Start");
        assert!(start_method.is_some());
        let start_method = start_method.unwrap();
        assert!(start_method.signature.as_ref().unwrap().contains("func (wp *WorkerPool) Start()"));

        let worker_method = symbols.iter().find(|s| s.name == "worker");
        assert!(worker_method.is_some());
        let worker_method = worker_method.unwrap();
        assert!(worker_method.signature.as_ref().unwrap().contains("func (wp *WorkerPool) worker(id int)"));
    }

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
        let mut extractor = GoExtractor::new("go".to_string(), "test.go".to_string(), code.to_string());
        let symbols = extractor.extract_symbols(&tree);

        let validation_error = symbols.iter().find(|s| s.name == "ValidationError");
        assert!(validation_error.is_some());
        let validation_error = validation_error.unwrap();
        assert_eq!(validation_error.kind, SymbolKind::Class);
        assert!(validation_error.signature.as_ref().unwrap().contains("type ValidationError struct"));

        let error_method = symbols.iter().find(|s| s.name == "Error");
        assert!(error_method.is_some());
        let error_method = error_method.unwrap();
        assert!(error_method.signature.as_ref().unwrap().contains("func (e ValidationError) Error() string"));

        let unwrap_method = symbols.iter().find(|s| s.name == "Unwrap");
        assert!(unwrap_method.is_some());
        let unwrap_method = unwrap_method.unwrap();
        assert!(unwrap_method.signature.as_ref().unwrap().contains("func (e ValidationError) Unwrap() error"));

        let database_error = symbols.iter().find(|s| s.name == "DatabaseError");
        assert!(database_error.is_some());
        let database_error = database_error.unwrap();
        assert_eq!(database_error.kind, SymbolKind::Class);

        let result_type = symbols.iter().find(|s| s.name == "Result");
        assert!(result_type.is_some());
        let result_type = result_type.unwrap();
        assert!(result_type.signature.as_ref().unwrap().contains("type Result[T any] struct"));

        let is_ok_method = symbols.iter().find(|s| s.name == "IsOk");
        assert!(is_ok_method.is_some());
        let is_ok_method = is_ok_method.unwrap();
        assert!(is_ok_method.signature.as_ref().unwrap().contains("func (r Result[T]) IsOk() bool"));

        let ok_func = symbols.iter().find(|s| s.name == "Ok");
        assert!(ok_func.is_some());
        let ok_func = ok_func.unwrap();
        assert!(ok_func.signature.as_ref().unwrap().contains("func Ok[T any](value T) Result[T]"));

        let err_func = symbols.iter().find(|s| s.name == "Err");
        assert!(err_func.is_some());
        let err_func = err_func.unwrap();
        assert!(err_func.signature.as_ref().unwrap().contains("func Err[T any](err error) Result[T]"));
    }

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
        let mut extractor = GoExtractor::new("go".to_string(), "test.go".to_string(), code.to_string());
        let symbols = extractor.extract_symbols(&tree);

        let reader = symbols.iter().find(|s| s.name == "Reader");
        assert!(reader.is_some());
        let reader = reader.unwrap();
        assert_eq!(reader.kind, SymbolKind::Interface);
        assert!(reader.signature.as_ref().unwrap().contains("type Reader interface"));

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
        assert!(read_write_closer.signature.as_ref().unwrap().contains("type ReadWriteCloser interface"));

        let container = symbols.iter().find(|s| s.name == "Container");
        assert!(container.is_some());
        let container = container.unwrap();
        assert_eq!(container.kind, SymbolKind::Class);
        assert!(container.signature.as_ref().unwrap().contains("type Container struct"));

        let process_value = symbols.iter().find(|s| s.name == "ProcessValue");
        assert!(process_value.is_some());
        let process_value = process_value.unwrap();
        assert!(process_value.signature.as_ref().unwrap().contains("func ProcessValue(value interface{}) string"));
    }

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
        let mut extractor = GoExtractor::new("go".to_string(), "test.go".to_string(), code.to_string());
        let symbols = extractor.extract_symbols(&tree);

        let test_user_service = symbols.iter().find(|s| s.name == "TestUserService");
        assert!(test_user_service.is_some());
        let test_user_service = test_user_service.unwrap();
        assert_eq!(test_user_service.kind, SymbolKind::Function);
        assert!(test_user_service.signature.as_ref().unwrap().contains("func TestUserService(t *testing.T)"));

        let test_validation = symbols.iter().find(|s| s.name == "TestValidation");
        assert!(test_validation.is_some());
        let test_validation = test_validation.unwrap();
        assert!(test_validation.signature.as_ref().unwrap().contains("func TestValidation(t *testing.T)"));

        let benchmark_user_creation = symbols.iter().find(|s| s.name == "BenchmarkUserCreation");
        assert!(benchmark_user_creation.is_some());
        let benchmark_user_creation = benchmark_user_creation.unwrap();
        assert!(benchmark_user_creation.signature.as_ref().unwrap().contains("func BenchmarkUserCreation(b *testing.B)"));

        let example_func = symbols.iter().find(|s| s.name == "ExampleUserService_CreateUser");
        assert!(example_func.is_some());
        let example_func = example_func.unwrap();
        assert!(example_func.signature.as_ref().unwrap().contains("func ExampleUserService_CreateUser()"));

        let fuzz_func = symbols.iter().find(|s| s.name == "FuzzUserValidation");
        assert!(fuzz_func.is_some());
        let fuzz_func = fuzz_func.unwrap();
        assert!(fuzz_func.signature.as_ref().unwrap().contains("func FuzzUserValidation(f *testing.F)"));
    }

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
        let mut extractor = GoExtractor::new("go".to_string(), "test.go".to_string(), code.to_string());
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
        assert!(get_user.unwrap().signature.as_ref().unwrap().contains("func (r *UserRepository) GetUser"));

        let process_users = symbols.iter().find(|s| s.name == "ProcessUsers");
        assert!(process_users.is_some());
        assert!(process_users.unwrap().signature.as_ref().unwrap().contains("<-chan User"));
    }

    #[test]
    fn test_handle_large_go_files_with_many_symbols() {
        // Generate a large Go file with many types and functions
        let mut types = String::new();
        for i in 0..20 {
            types.push_str(&format!(r#"
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
"#, i = i));
        }

        let code = format!(r#"
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
"#, types = types);

        let tree = init_parser(&code, "go");
        let mut extractor = GoExtractor::new("go".to_string(), "test.go".to_string(), code.to_string());
        let symbols = extractor.extract_symbols(&tree);
        let relationships = extractor.extract_relationships(&tree, &symbols);

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
        let mut extractor = GoExtractor::new("go".to_string(), "test.go".to_string(), code.to_string());

        // Should not panic even with malformed code
        let symbols = extractor.extract_symbols(&tree);
        let relationships = extractor.extract_relationships(&tree, &symbols);

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
        assert!(complex_function.unwrap().signature.as_ref().unwrap().contains("func ComplexFunction"));

        let named_returns = symbols.iter().find(|s| s.name == "NamedReturns");
        assert!(named_returns.is_some());
        assert!(named_returns.unwrap().signature.as_ref().unwrap().contains("(sum, product int)"));

        let variadic_func = symbols.iter().find(|s| s.name == "VariadicFunction");
        assert!(variadic_func.is_some());
        assert!(variadic_func.unwrap().signature.as_ref().unwrap().contains("args ...interface{}"));

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
}