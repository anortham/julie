// C Extractor Tests
//
// Direct port of Miller's C extractor tests (TDD RED phase)
// Original: /Users/murphy/Source/miller/src/__tests__/parser/c-extractor.test.ts

use crate::extractors::base::{Symbol, SymbolKind};
use crate::extractors::c::CExtractor;
use tree_sitter::Parser;

/// Initialize C parser for C files
fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_c::LANGUAGE.into()).expect("Error loading C grammar");
    parser
}

#[cfg(test)]
mod c_extractor_tests {
    use super::*;

    /// Port of Miller's "should extract functions, variables, structs, and basic declarations" test
    #[test]
    fn test_extract_basic_c_structures_and_declarations() {
        let code = r#"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>
#include "custom_header.h"

// Preprocessor definitions
#define MAX_SIZE 1024
#define MIN(a, b) ((a) < (b) ? (a) : (b))
#define MAX(a, b) ((a) > (b) ? (a) : (b))
#define DEBUG 1

#if DEBUG
#define LOG(msg) printf("DEBUG: %s\n", msg)
#else
#define LOG(msg)
#endif

// Type definitions
typedef int ErrorCode;
typedef unsigned long long uint64_t;
typedef char* String;

typedef struct Point {
    double x;
    double y;
} Point;

typedef struct Rectangle {
    Point top_left;
    Point bottom_right;
    int color;
} Rectangle;

// Enumerations
enum Status {
    STATUS_SUCCESS = 0,
    STATUS_ERROR = 1,
    STATUS_PENDING = 2,
    STATUS_TIMEOUT = 3
};

typedef enum {
    LOG_LEVEL_DEBUG,
    LOG_LEVEL_INFO,
    LOG_LEVEL_WARNING,
    LOG_LEVEL_ERROR
} LogLevel;

// Global variables
int global_counter = 0;
static int static_counter = 0;
extern int external_counter;
const double PI = 3.14159265359;
volatile int interrupt_flag = 0;

char global_buffer[MAX_SIZE];
Point origin = {0.0, 0.0};
Rectangle default_rect = {{0, 0}, {100, 100}, 0xFFFFFF};

// Function declarations
int add(int a, int b);
double calculate_distance(Point p1, Point p2);
char* allocate_string(size_t length);
void free_string(char* str);
ErrorCode process_data(const char* input, char* output, size_t max_length);

// Simple function definitions
int add(int a, int b) {
    return a + b;
}

int subtract(int a, int b) {
    LOG("Performing subtraction");
    return a - b;
}

double multiply(double x, double y) {
    return x * y;
}

// Function with complex parameters
ErrorCode process_data(const char* input, char* output, size_t max_length) {
    if (input == NULL || output == NULL) {
        return STATUS_ERROR;
    }

    size_t input_len = strlen(input);
    if (input_len >= max_length) {
        return STATUS_ERROR;
    }

    strcpy(output, input);
    return STATUS_SUCCESS;
}

// Function with pointer parameters
void swap_integers(int* a, int* b) {
    if (a == NULL || b == NULL) return;

    int temp = *a;
    *a = *b;
    *b = temp;
}

// Function with array parameters
double sum_array(const double arr[], int count) {
    double total = 0.0;
    for (int i = 0; i < count; i++) {
        total += arr[i];
    }
    return total;
}

// Function with variable arguments
#include <stdarg.h>

int sum_variadic(int count, ...) {
    va_list args;
    va_start(args, count);

    int total = 0;
    for (int i = 0; i < count; i++) {
        total += va_arg(args, int);
    }

    va_end(args);
    return total;
}

// Static function
static void internal_helper() {
    static int call_count = 0;
    call_count++;
    printf("Helper called %d times\n", call_count);
}

// Inline function (C99)
inline int square(int x) {
    return x * x;
}

// Function returning pointer
char* create_greeting(const char* name) {
    if (name == NULL) return NULL;

    size_t name_len = strlen(name);
    size_t greeting_len = name_len + 20; // "Hello, " + name + "!"

    char* greeting = malloc(greeting_len);
    if (greeting == NULL) {
        return NULL;
    }

    snprintf(greeting, greeting_len, "Hello, %s!", name);
    return greeting;
}

// Function with function pointer parameter
typedef int (*CompareFn)(const void* a, const void* b);

void sort_array(void* base, size_t count, size_t size, CompareFn compare) {
    // Simplified bubble sort implementation
    for (size_t i = 0; i < count - 1; i++) {
        for (size_t j = 0; j < count - i - 1; j++) {
            char* elem1 = (char*)base + j * size;
            char* elem2 = (char*)base + (j + 1) * size;

            if (compare(elem1, elem2) > 0) {
                // Swap elements
                for (size_t k = 0; k < size; k++) {
                    char temp = elem1[k];
                    elem1[k] = elem2[k];
                    elem2[k] = temp;
                }
            }
        }
    }
}

// Comparison functions
int compare_integers(const void* a, const void* b) {
    int ia = *(const int*)a;
    int ib = *(const int*)b;
    return (ia > ib) - (ia < ib);
}

int compare_strings(const void* a, const void* b) {
    const char* sa = *(const char**)a;
    const char* sb = *(const char**)b;
    return strcmp(sa, sb);
}

// Main function
int main(int argc, char* argv[]) {
    printf("Program started with %d arguments\n", argc);

    // Test basic operations
    int x = 10, y = 20;
    int sum = add(x, y);
    int diff = subtract(x, y);

    printf("Sum: %d, Difference: %d\n", sum, diff);

    // Test string operations
    char* greeting = create_greeting("World");
    if (greeting != NULL) {
        printf("%s\n", greeting);
        free(greeting);
    }

    // Test array operations
    double numbers[] = {1.5, 2.3, 3.7, 4.1, 5.9};
    int count = sizeof(numbers) / sizeof(numbers[0]);
    double total = sum_array(numbers, count);
    printf("Array sum: %.2f\n", total);

    // Test variadic function
    int var_sum = sum_variadic(5, 1, 2, 3, 4, 5);
    printf("Variadic sum: %d\n", var_sum);

    return STATUS_SUCCESS;
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = CExtractor::new(
            "c".to_string(),
            "basic.c".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Include statements - Miller expects these to be found
        let stdio_include = symbols.iter().find(|s| s.signature.as_ref().map_or(false, |sig| sig.contains("#include <stdio.h>")));
        assert!(stdio_include.is_some());
        assert_eq!(stdio_include.unwrap().kind, SymbolKind::Import);

        let custom_include = symbols.iter().find(|s| s.signature.as_ref().map_or(false, |sig| sig.contains("#include \"custom_header.h\"")));
        assert!(custom_include.is_some());

        // Macro definitions - Miller tests these specific macros
        let max_size_macro = symbols.iter().find(|s| s.name == "MAX_SIZE");
        assert!(max_size_macro.is_some());
        assert_eq!(max_size_macro.unwrap().kind, SymbolKind::Constant);
        assert!(max_size_macro.unwrap().signature.as_ref().unwrap().contains("#define MAX_SIZE 1024"));

        let min_macro = symbols.iter().find(|s| s.name == "MIN");
        assert!(min_macro.is_some());
        assert!(min_macro.unwrap().signature.as_ref().unwrap().contains("#define MIN(a, b)"));

        let debug_macro = symbols.iter().find(|s| s.name == "DEBUG");
        assert!(debug_macro.is_some());

        let log_macro = symbols.iter().find(|s| s.name == "LOG");
        assert!(log_macro.is_some());

        // Typedefs - Miller checks these type definitions
        let error_code_typedef = symbols.iter().find(|s| s.name == "ErrorCode");
        assert!(error_code_typedef.is_some());
        assert_eq!(error_code_typedef.unwrap().kind, SymbolKind::Type);
        assert!(error_code_typedef.unwrap().signature.as_ref().unwrap().contains("typedef int ErrorCode"));

        let uint64_typedef = symbols.iter().find(|s| s.name == "uint64_t");
        assert!(uint64_typedef.is_some());

        let string_typedef = symbols.iter().find(|s| s.name == "String");
        assert!(string_typedef.is_some());

        let point_typedef = symbols.iter().find(|s| s.name == "Point");
        assert!(point_typedef.is_some());
        assert!(point_typedef.unwrap().signature.as_ref().unwrap().contains("typedef struct Point"));

        // Struct definitions - Miller expects these as classes
        let point_struct = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains("struct Point") && sig.contains("double x"))
        );
        assert!(point_struct.is_some());
        assert_eq!(point_struct.unwrap().kind, SymbolKind::Class); // Structs as classes

        let rectangle_struct = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains("struct Rectangle"))
        );
        assert!(rectangle_struct.is_some());

        // Enum definitions - Miller tests enum extraction
        let status_enum = symbols.iter().find(|s| s.name == "Status");
        assert!(status_enum.is_some());
        assert_eq!(status_enum.unwrap().kind, SymbolKind::Enum);

        let log_level_enum = symbols.iter().find(|s| s.name == "LogLevel");
        assert!(log_level_enum.is_some());

        // Enum values - Miller extracts these as constants
        let status_success = symbols.iter().find(|s| s.name == "STATUS_SUCCESS");
        assert!(status_success.is_some());
        assert_eq!(status_success.unwrap().kind, SymbolKind::Constant);

        let log_debug = symbols.iter().find(|s| s.name == "LOG_LEVEL_DEBUG");
        assert!(log_debug.is_some());

        // Global variables - Miller tests various variable types
        let global_counter = symbols.iter().find(|s| s.name == "global_counter");
        assert!(global_counter.is_some());
        assert_eq!(global_counter.unwrap().kind, SymbolKind::Variable);
        assert!(global_counter.unwrap().signature.as_ref().unwrap().contains("int global_counter = 0"));

        let static_counter = symbols.iter().find(|s| s.name == "static_counter");
        assert!(static_counter.is_some());
        assert!(static_counter.unwrap().signature.as_ref().unwrap().contains("static int static_counter"));

        let external_counter = symbols.iter().find(|s| s.name == "external_counter");
        assert!(external_counter.is_some());
        assert!(external_counter.unwrap().signature.as_ref().unwrap().contains("extern int external_counter"));

        let pi_constant = symbols.iter().find(|s| s.name == "PI");
        assert!(pi_constant.is_some());
        assert!(pi_constant.unwrap().signature.as_ref().unwrap().contains("const double PI"));

        let volatile_flag = symbols.iter().find(|s| s.name == "interrupt_flag");
        assert!(volatile_flag.is_some());
        assert!(volatile_flag.unwrap().signature.as_ref().unwrap().contains("volatile int interrupt_flag"));

        // Array variables - Miller tests array declarations
        let global_buffer = symbols.iter().find(|s| s.name == "global_buffer");
        assert!(global_buffer.is_some());
        assert!(global_buffer.unwrap().signature.as_ref().unwrap().contains("char global_buffer[MAX_SIZE]"));

        let origin = symbols.iter().find(|s| s.name == "origin");
        assert!(origin.is_some());
        assert!(origin.unwrap().signature.as_ref().unwrap().contains("Point origin = {0.0, 0.0}"));

        // Function declarations and definitions - Miller tests various function types
        let add_function = symbols.iter().find(|s| s.name == "add");
        assert!(add_function.is_some());
        assert_eq!(add_function.unwrap().kind, SymbolKind::Function);
        assert!(add_function.unwrap().signature.as_ref().unwrap().contains("int add(int a, int b)"));

        let subtract_function = symbols.iter().find(|s| s.name == "subtract");
        assert!(subtract_function.is_some());

        let multiply_function = symbols.iter().find(|s| s.name == "multiply");
        assert!(multiply_function.is_some());
        assert!(multiply_function.unwrap().signature.as_ref().unwrap().contains("double multiply(double x, double y)"));

        // Complex parameter functions - Miller tests complex signatures
        let process_data_function = symbols.iter().find(|s| s.name == "process_data");
        assert!(process_data_function.is_some());
        assert!(process_data_function.unwrap().signature.as_ref().unwrap().contains("ErrorCode process_data(const char* input"));

        let swap_function = symbols.iter().find(|s| s.name == "swap_integers");
        assert!(swap_function.is_some());
        assert!(swap_function.unwrap().signature.as_ref().unwrap().contains("void swap_integers(int* a, int* b)"));

        let sum_array_function = symbols.iter().find(|s| s.name == "sum_array");
        assert!(sum_array_function.is_some());
        assert!(sum_array_function.unwrap().signature.as_ref().unwrap().contains("double sum_array(const double arr[], int count)"));

        // Variadic function - Miller tests variadic parameters
        let sum_variadic_function = symbols.iter().find(|s| s.name == "sum_variadic");
        assert!(sum_variadic_function.is_some());
        assert!(sum_variadic_function.unwrap().signature.as_ref().unwrap().contains("int sum_variadic(int count, ...)"));

        // Static function - Miller tests static functions
        let internal_helper = symbols.iter().find(|s| s.name == "internal_helper");
        assert!(internal_helper.is_some());
        assert!(internal_helper.unwrap().signature.as_ref().unwrap().contains("static void internal_helper()"));

        // Inline function - Miller tests inline functions
        let square_function = symbols.iter().find(|s| s.name == "square");
        assert!(square_function.is_some());
        assert!(square_function.unwrap().signature.as_ref().unwrap().contains("inline int square(int x)"));

        // Function returning pointer - Miller tests pointer return types
        let create_greeting = symbols.iter().find(|s| s.name == "create_greeting");
        assert!(create_greeting.is_some());
        assert!(create_greeting.unwrap().signature.as_ref().unwrap().contains("char* create_greeting(const char* name)"));

        // Function pointer typedef - Miller tests function pointer types
        let compare_fn_typedef = symbols.iter().find(|s| s.name == "CompareFn");
        assert!(compare_fn_typedef.is_some());
        assert!(compare_fn_typedef.unwrap().signature.as_ref().unwrap().contains("typedef int (*CompareFn)"));

        // Function with function pointer parameter - Miller tests complex parameter types
        let sort_array_function = symbols.iter().find(|s| s.name == "sort_array");
        assert!(sort_array_function.is_some());
        assert!(sort_array_function.unwrap().signature.as_ref().unwrap().contains("CompareFn compare"));

        // Comparison functions - Miller tests these implementations
        let compare_integers = symbols.iter().find(|s| s.name == "compare_integers");
        assert!(compare_integers.is_some());

        let compare_strings = symbols.iter().find(|s| s.name == "compare_strings");
        assert!(compare_strings.is_some());

        // Main function - Miller tests main function signature
        let main_function = symbols.iter().find(|s| s.name == "main");
        assert!(main_function.is_some());
        assert!(main_function.unwrap().signature.as_ref().unwrap().contains("int main(int argc, char* argv[])"));
    }

    /// Port of Miller's "should extract complex structs, unions, function pointers, and memory operations" test
    #[test]
    fn test_extract_advanced_c_features() {
        let code = r#"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <stdbool.h>
#include <complex.h>

// Advanced preprocessor usage
#define STRINGIFY(x) #x
#define CONCAT(a, b) a##b
#define ARRAY_SIZE(arr) (sizeof(arr) / sizeof((arr)[0]))

#ifdef __cplusplus
extern "C" {
#endif

// Forward declarations
struct Node;
typedef struct Node Node;

// Complex data structures
typedef struct {
    int id;
    char name[64];
    double balance;
    bool active;
    void* user_data;
} Account;

typedef union {
    int int_value;
    float float_value;
    char char_value;
    void* pointer_value;
    struct {
        uint16_t low;
        uint16_t high;
    } word;
} Value;

// Linked list node
struct Node {
    int data;
    struct Node* next;
    struct Node* prev;
};

// Binary tree node
typedef struct TreeNode {
    int value;
    struct TreeNode* left;
    struct TreeNode* right;
    int height;
} TreeNode;

// Function pointer types
typedef int (*BinaryOperation)(int a, int b);
typedef void (*Callback)(void* context, int result);
typedef bool (*Predicate)(const void* item);

// Struct with function pointers (virtual table pattern)
typedef struct {
    void* data;
    size_t size;
    void (*destroy)(void* data);
    void* (*clone)(const void* data);
    int (*compare)(const void* a, const void* b);
    char* (*to_string)(const void* data);
} GenericObject;

// Bit fields
typedef struct {
    unsigned int is_valid : 1;
    unsigned int is_dirty : 1;
    unsigned int level : 4;
    unsigned int type : 8;
    unsigned int reserved : 18;
} Flags;

typedef struct {
    uint32_t ip;
    uint16_t port;
    uint8_t protocol;
    Flags flags;
} NetworkPacket;

// Memory pool allocator
typedef struct MemoryBlock {
    void* data;
    size_t size;
    bool in_use;
    struct MemoryBlock* next;
} MemoryBlock;

typedef struct {
    MemoryBlock* blocks;
    size_t total_size;
    size_t used_size;
    size_t block_count;
} MemoryPool;

// Function pointer arrays and tables
typedef struct {
    const char* name;
    BinaryOperation operation;
} OperationEntry;

// Global function pointer table
OperationEntry operation_table[] = {
    {"add", add_operation},
    {"subtract", subtract_operation},
    {"multiply", multiply_operation},
    {"divide", divide_operation},
    {NULL, NULL}
};

// Memory management functions
MemoryPool* create_memory_pool(size_t total_size) {
    MemoryPool* pool = malloc(sizeof(MemoryPool));
    if (pool == NULL) {
        return NULL;
    }

    pool->blocks = malloc(sizeof(MemoryBlock));
    if (pool->blocks == NULL) {
        free(pool);
        return NULL;
    }

    pool->blocks->data = malloc(total_size);
    if (pool->blocks->data == NULL) {
        free(pool->blocks);
        free(pool);
        return NULL;
    }

    pool->blocks->size = total_size;
    pool->blocks->in_use = false;
    pool->blocks->next = NULL;
    pool->total_size = total_size;
    pool->used_size = 0;
    pool->block_count = 1;

    return pool;
}

// Signal handling and system programming
#include <signal.h>
#include <unistd.h>

volatile sig_atomic_t shutdown_flag = 0;

void signal_handler(int signum) {
    switch (signum) {
        case SIGINT:
        case SIGTERM:
            shutdown_flag = 1;
            break;
        default:
            break;
    }
}

#ifdef __cplusplus
}
#endif
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = CExtractor::new(
            "c".to_string(),
            "advanced.c".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Advanced macros - Miller tests sophisticated preprocessing
        let stringify_macro = symbols.iter().find(|s| s.name == "STRINGIFY");
        assert!(stringify_macro.is_some());
        assert!(stringify_macro.unwrap().signature.as_ref().unwrap().contains("#define STRINGIFY(x) #x"));

        let concat_macro = symbols.iter().find(|s| s.name == "CONCAT");
        assert!(concat_macro.is_some());

        let array_size_macro = symbols.iter().find(|s| s.name == "ARRAY_SIZE");
        assert!(array_size_macro.is_some());

        // Complex structs - Miller tests advanced data structures
        let account_struct = symbols.iter().find(|s| s.name == "Account");
        assert!(account_struct.is_some());
        assert_eq!(account_struct.unwrap().kind, SymbolKind::Class);
        assert!(account_struct.unwrap().signature.as_ref().unwrap().contains("typedef struct"));

        let node_struct = symbols.iter().find(|s| s.name == "Node");
        assert!(node_struct.is_some());

        let tree_node_struct = symbols.iter().find(|s| s.name == "TreeNode");
        assert!(tree_node_struct.is_some());

        // Union types - Miller tests union extraction
        let value_union = symbols.iter().find(|s| s.name == "Value");
        assert!(value_union.is_some());
        assert!(value_union.unwrap().signature.as_ref().unwrap().contains("typedef union"));

        // Function pointer typedefs - Miller tests function pointer types
        let binary_op_typedef = symbols.iter().find(|s| s.name == "BinaryOperation");
        assert!(binary_op_typedef.is_some());
        assert!(binary_op_typedef.unwrap().signature.as_ref().unwrap().contains("typedef int (*BinaryOperation)"));

        let callback_typedef = symbols.iter().find(|s| s.name == "Callback");
        assert!(callback_typedef.is_some());

        let predicate_typedef = symbols.iter().find(|s| s.name == "Predicate");
        assert!(predicate_typedef.is_some());

        // Struct with function pointers - Miller tests virtual table patterns
        let generic_object_struct = symbols.iter().find(|s| s.name == "GenericObject");
        assert!(generic_object_struct.is_some());
        assert!(generic_object_struct.unwrap().signature.as_ref().unwrap().contains("void (*destroy)"));
        assert!(generic_object_struct.unwrap().signature.as_ref().unwrap().contains("void* (*clone)"));

        // Bit fields - Miller tests bit field structures
        let flags_struct = symbols.iter().find(|s| s.name == "Flags");
        assert!(flags_struct.is_some());
        assert!(flags_struct.unwrap().signature.as_ref().unwrap().contains("unsigned int is_valid : 1"));

        let network_packet_struct = symbols.iter().find(|s| s.name == "NetworkPacket");
        assert!(network_packet_struct.is_some());

        // Memory management structures - Miller tests memory pool patterns
        let memory_block_struct = symbols.iter().find(|s| s.name == "MemoryBlock");
        assert!(memory_block_struct.is_some());

        let memory_pool_struct = symbols.iter().find(|s| s.name == "MemoryPool");
        assert!(memory_pool_struct.is_some());

        // Operation table struct - Miller tests table structures
        let operation_entry_struct = symbols.iter().find(|s| s.name == "OperationEntry");
        assert!(operation_entry_struct.is_some());

        // Global arrays - Miller tests array declarations
        let operation_table = symbols.iter().find(|s| s.name == "operation_table");
        assert!(operation_table.is_some());
        assert!(operation_table.unwrap().signature.as_ref().unwrap().contains("OperationEntry operation_table[]"));

        // Memory management functions - Miller tests function signatures
        let create_memory_pool = symbols.iter().find(|s| s.name == "create_memory_pool");
        assert!(create_memory_pool.is_some());
        assert!(create_memory_pool.unwrap().signature.as_ref().unwrap().contains("MemoryPool* create_memory_pool(size_t total_size)"));

        // Signal handling - Miller tests signal handling patterns
        let shutdown_flag = symbols.iter().find(|s| s.name == "shutdown_flag");
        assert!(shutdown_flag.is_some());
        assert!(shutdown_flag.unwrap().signature.as_ref().unwrap().contains("volatile sig_atomic_t shutdown_flag"));

        let signal_handler = symbols.iter().find(|s| s.name == "signal_handler");
        assert!(signal_handler.is_some());
        assert!(signal_handler.unwrap().signature.as_ref().unwrap().contains("void signal_handler(int signum)"));

        // Extern "C" handling - Miller tests linkage specifications
        let extern_c = symbols.iter().filter(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains("extern \"C\""))
        ).count();
        assert!(extern_c >= 1);

        // Standard library includes - Miller tests include extraction
        let stdint_include = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains("#include <stdint.h>"))
        );
        assert!(stdint_include.is_some());

        let stdbool_include = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains("#include <stdbool.h>"))
        );
        assert!(stdbool_include.is_some());

        let complex_include = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains("#include <complex.h>"))
        );
        assert!(complex_include.is_some());

        let signal_include = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains("#include <signal.h>"))
        );
        assert!(signal_include.is_some());

        let unistd_include = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains("#include <unistd.h>"))
        );
        assert!(unistd_include.is_some());
    }

    /// Port of Miller's "should extract preprocessor directives, conditional compilation, and pragma directives" test
    #[test]
    fn test_extract_c_preprocessor_features() {
        let code = r#"
// Compiler and platform detection
#ifdef __GNUC__
    #define COMPILER_GCC 1
    #define LIKELY(x) __builtin_expect(!!(x), 1)
    #define UNLIKELY(x) __builtin_expect(!!(x), 0)
    #define FORCE_INLINE __attribute__((always_inline)) inline
    #define PACKED __attribute__((packed))
#elif defined(_MSC_VER)
    #define COMPILER_MSVC 1
    #define LIKELY(x) (x)
    #define UNLIKELY(x) (x)
    #define FORCE_INLINE __forceinline
    #define PACKED
    #pragma warning(disable: 4996) // Disable deprecated function warnings
#else
    #define COMPILER_UNKNOWN 1
    #define LIKELY(x) (x)
    #define UNLIKELY(x) (x)
    #define FORCE_INLINE inline
    #define PACKED
#endif

// Platform detection
#if defined(_WIN32) || defined(_WIN64)
    #define PLATFORM_WINDOWS 1
    #include <windows.h>
    typedef HANDLE FileHandle;
    #define INVALID_FILE_HANDLE INVALID_HANDLE_VALUE
#elif defined(__linux__)
    #define PLATFORM_LINUX 1
    #include <unistd.h>
    #include <fcntl.h>
    typedef int FileHandle;
    #define INVALID_FILE_HANDLE -1
#elif defined(__APPLE__)
    #define PLATFORM_MACOS 1
    #include <unistd.h>
    #include <fcntl.h>
    typedef int FileHandle;
    #define INVALID_FILE_HANDLE -1
#else
    #define PLATFORM_UNKNOWN 1
    typedef void* FileHandle;
    #define INVALID_FILE_HANDLE NULL
#endif

// Architecture detection
#if defined(__x86_64__) || defined(_M_X64)
    #define ARCH_X64 1
    #define CACHE_LINE_SIZE 64
#elif defined(__i386__) || defined(_M_IX86)
    #define ARCH_X86 1
    #define CACHE_LINE_SIZE 32
#elif defined(__aarch64__)
    #define ARCH_ARM64 1
    #define CACHE_LINE_SIZE 64
#elif defined(__arm__)
    #define ARCH_ARM 1
    #define CACHE_LINE_SIZE 32
#else
    #define ARCH_UNKNOWN 1
    #define CACHE_LINE_SIZE 64
#endif

// Debug/Release configuration
#ifdef NDEBUG
    #define BUILD_RELEASE 1
    #define DEBUG_PRINT(...)
    #define ASSERT(condition)
    #define DEBUG_ONLY(code)
#else
    #define BUILD_DEBUG 1
    #define DEBUG_PRINT(...) printf(__VA_ARGS__)
    #define ASSERT(condition) \
        do { \
            if (!(condition)) { \
                fprintf(stderr, "Assertion failed: %s at %s:%d\n", \
                        #condition, __FILE__, __LINE__); \
                abort(); \
            } \
        } while(0)
    #define DEBUG_ONLY(code) code
#endif

// Version information
#define VERSION_MAJOR 2
#define VERSION_MINOR 1
#define VERSION_PATCH 3
#define VERSION_STRING STRINGIFY(VERSION_MAJOR) "." \
                      STRINGIFY(VERSION_MINOR) "." \
                      STRINGIFY(VERSION_PATCH)

// Feature toggles
#ifndef FEATURE_NETWORKING
    #define FEATURE_NETWORKING 1
#endif

#ifndef FEATURE_GRAPHICS
    #define FEATURE_GRAPHICS 0
#endif

#ifndef FEATURE_AUDIO
    #define FEATURE_AUDIO 1
#endif

// Conditional feature compilation
#if FEATURE_NETWORKING
    #include <sys/socket.h>
    #include <netinet/in.h>
    #include <arpa/inet.h>

    typedef struct {
        int socket_fd;
        struct sockaddr_in address;
        bool connected;
    } NetworkConnection;

    int network_init(void);
    NetworkConnection* network_connect(const char* host, int port);
    void network_cleanup(void);
#endif

#if FEATURE_AUDIO
    typedef struct {
        float* samples;
        size_t sample_count;
        int sample_rate;
        int channels;
    } AudioBuffer;

    int audio_init(int sample_rate, int channels);
    void audio_shutdown(void);
#endif

// Pragma directives
#pragma once
#pragma pack(push, 1)

typedef struct {
    uint8_t type;
    uint16_t length;
    uint32_t data;
} PACKED NetworkHeader;

#pragma pack(pop)

// Alignment directives
#ifdef COMPILER_GCC
    #define ALIGN(n) __attribute__((aligned(n)))
#elif defined(COMPILER_MSVC)
    #define ALIGN(n) __declspec(align(n))
#else
    #define ALIGN(n)
#endif

typedef struct ALIGN(CACHE_LINE_SIZE) {
    volatile int counter;
    char padding[CACHE_LINE_SIZE - sizeof(int)];
} AtomicCounter;

// Inline assembly (GCC specific)
#ifdef COMPILER_GCC
    static inline uint64_t rdtsc(void) {
        uint32_t low, high;
        asm volatile ("rdtsc" : "=a" (low), "=d" (high));
        return ((uint64_t)high << 32) | low;
    }

    static inline void cpu_pause(void) {
        asm volatile ("pause" ::: "memory");
    }
#else
    static inline uint64_t rdtsc(void) {
        return 0; // Fallback implementation
    }

    static inline void cpu_pause(void) {
        // No-op on non-GCC compilers
    }
#endif

// Advanced macro techniques
#define GET_MACRO(_1,_2,_3,_4,NAME,...) NAME
#define VARIADIC_MACRO(...) GET_MACRO(__VA_ARGS__, MACRO4, MACRO3, MACRO2, MACRO1)(__VA_ARGS__)

#define MACRO1(a) single_arg_function(a)
#define MACRO2(a,b) two_arg_function(a,b)
#define MACRO3(a,b,c) three_arg_function(a,b,c)
#define MACRO4(a,b,c,d) four_arg_function(a,b,c,d)

// X-Macro pattern for enum/string mapping
#define ERROR_CODES(X) \
    X(ERROR_NONE, "No error") \
    X(ERROR_INVALID_PARAM, "Invalid parameter") \
    X(ERROR_OUT_OF_MEMORY, "Out of memory") \
    X(ERROR_FILE_NOT_FOUND, "File not found") \
    X(ERROR_PERMISSION_DENIED, "Permission denied") \
    X(ERROR_NETWORK_FAILURE, "Network failure")

// Generate enum
#define ENUM_ENTRY(name, desc) name,
typedef enum {
    ERROR_CODES(ENUM_ENTRY)
    ERROR_COUNT
} ErrorCode;
#undef ENUM_ENTRY

// Generate string array
#define STRING_ENTRY(name, desc) desc,
static const char* error_strings[] = {
    ERROR_CODES(STRING_ENTRY)
};
#undef STRING_ENTRY

// Function to get error string
const char* get_error_string(ErrorCode code) {
    if (code < 0 || code >= ERROR_COUNT) {
        return "Unknown error";
    }
    return error_strings[code];
}

// Stringification and token pasting
#define DECLARE_GETTER_SETTER(type, name) \
    type get_##name(void) { return name; } \
    void set_##name(type value) { name = value; }

// Example usage
static int global_value = 42;
DECLARE_GETTER_SETTER(int, global_value)

// Conditional compilation for different C standards
#if __STDC_VERSION__ >= 201112L
    // C11 features
    #include <stdatomic.h>
    #include <threads.h>

    typedef atomic_int AtomicInt;

    static inline int atomic_increment(AtomicInt* value) {
        return atomic_fetch_add(value, 1) + 1;
    }

    _Static_assert(sizeof(int) == 4, "int must be 4 bytes");
    _Static_assert(CACHE_LINE_SIZE >= 32, "Cache line size too small");

#elif __STDC_VERSION__ >= 199901L
    // C99 features
    typedef volatile int AtomicInt;

    static inline int atomic_increment(AtomicInt* value) {
        return ++(*value); // Not truly atomic, just an example
    }

#else
    // C90 fallback
    typedef int AtomicInt;

    int atomic_increment(AtomicInt* value) {
        return ++(*value);
    }
#endif

// Compiler-specific optimizations
#ifdef COMPILER_GCC
    #define OPTIMIZE_SIZE __attribute__((optimize("Os")))
    #define OPTIMIZE_SPEED __attribute__((optimize("O3")))
    #define NO_OPTIMIZE __attribute__((optimize("O0")))
#else
    #define OPTIMIZE_SIZE
    #define OPTIMIZE_SPEED
    #define NO_OPTIMIZE
#endif

OPTIMIZE_SPEED
int fast_function(int x) {
    return x * x + 2 * x + 1;
}

OPTIMIZE_SIZE
void small_function(void) {
    // Optimized for size
    printf("This function is optimized for size\n");
}

NO_OPTIMIZE
void debug_function(void) {
    // No optimization for easier debugging
    int x = 10;
    int y = 20;
    int z = x + y;
    printf("Debug: %d + %d = %d\n", x, y, z);
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = CExtractor::new(
            "c".to_string(),
            "preprocessor.c".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Compiler detection macros - Miller tests compiler feature detection
        let compiler_gcc = symbols.iter().find(|s| s.name == "COMPILER_GCC");
        assert!(compiler_gcc.is_some());
        assert_eq!(compiler_gcc.unwrap().kind, SymbolKind::Constant);

        let likely_macro = symbols.iter().find(|s| s.name == "LIKELY");
        assert!(likely_macro.is_some());
        assert!(likely_macro.unwrap().signature.as_ref().unwrap().contains("__builtin_expect"));

        let force_inline_macro = symbols.iter().find(|s| s.name == "FORCE_INLINE");
        assert!(force_inline_macro.is_some());

        let packed_macro = symbols.iter().find(|s| s.name == "PACKED");
        assert!(packed_macro.is_some());

        // Platform detection - Miller tests platform-specific macros
        let platform_windows = symbols.iter().find(|s| s.name == "PLATFORM_WINDOWS");
        assert!(platform_windows.is_some());

        let file_handle_typedef = symbols.iter().find(|s| s.name == "FileHandle");
        assert!(file_handle_typedef.is_some());

        let invalid_file_handle = symbols.iter().find(|s| s.name == "INVALID_FILE_HANDLE");
        assert!(invalid_file_handle.is_some());

        // Architecture detection - Miller tests arch-specific features
        let arch_x64 = symbols.iter().find(|s| s.name == "ARCH_X64");
        assert!(arch_x64.is_some());

        let cache_line_size = symbols.iter().find(|s| s.name == "CACHE_LINE_SIZE");
        assert!(cache_line_size.is_some());

        // Debug/Release configuration - Miller tests build configuration
        let build_debug = symbols.iter().find(|s| s.name == "BUILD_DEBUG");
        assert!(build_debug.is_some());

        let debug_print = symbols.iter().find(|s| s.name == "DEBUG_PRINT");
        assert!(debug_print.is_some());

        let assert_macro = symbols.iter().find(|s| s.name == "ASSERT");
        assert!(assert_macro.is_some());

        let debug_only = symbols.iter().find(|s| s.name == "DEBUG_ONLY");
        assert!(debug_only.is_some());

        // Version information - Miller tests version macros
        let version_major = symbols.iter().find(|s| s.name == "VERSION_MAJOR");
        assert!(version_major.is_some());

        let version_string = symbols.iter().find(|s| s.name == "VERSION_STRING");
        assert!(version_string.is_some());

        // Feature toggles - Miller tests conditional features
        let feature_networking = symbols.iter().find(|s| s.name == "FEATURE_NETWORKING");
        assert!(feature_networking.is_some());

        let feature_graphics = symbols.iter().find(|s| s.name == "FEATURE_GRAPHICS");
        assert!(feature_graphics.is_some());

        // Conditional structs - Miller tests conditional compilation
        let network_connection = symbols.iter().find(|s| s.name == "NetworkConnection");
        assert!(network_connection.is_some());

        let audio_buffer = symbols.iter().find(|s| s.name == "AudioBuffer");
        assert!(audio_buffer.is_some());

        // Conditional functions - Miller tests conditional function definitions
        let network_init = symbols.iter().find(|s| s.name == "network_init");
        assert!(network_init.is_some());

        let audio_init = symbols.iter().find(|s| s.name == "audio_init");
        assert!(audio_init.is_some());

        // Pragma-affected structures - Miller tests pragma handling
        let network_header = symbols.iter().find(|s| s.name == "NetworkHeader");
        assert!(network_header.is_some());
        assert!(network_header.unwrap().signature.as_ref().unwrap().contains("PACKED"));

        // Alignment directives - Miller tests alignment features
        let align_macro = symbols.iter().find(|s| s.name == "ALIGN");
        assert!(align_macro.is_some());

        let atomic_counter = symbols.iter().find(|s| s.name == "AtomicCounter");
        assert!(atomic_counter.is_some());
        assert!(atomic_counter.unwrap().signature.as_ref().unwrap().contains("ALIGN(CACHE_LINE_SIZE)"));

        // Inline assembly functions - Miller tests inline assembly
        let rdtsc_function = symbols.iter().find(|s| s.name == "rdtsc");
        assert!(rdtsc_function.is_some());
        assert!(rdtsc_function.unwrap().signature.as_ref().unwrap().contains("static inline uint64_t rdtsc(void)"));

        let cpu_pause_function = symbols.iter().find(|s| s.name == "cpu_pause");
        assert!(cpu_pause_function.is_some());

        // Advanced macro patterns - Miller tests sophisticated macro techniques
        let get_macro = symbols.iter().find(|s| s.name == "GET_MACRO");
        assert!(get_macro.is_some());

        let variadic_macro = symbols.iter().find(|s| s.name == "VARIADIC_MACRO");
        assert!(variadic_macro.is_some());

        let macro1 = symbols.iter().find(|s| s.name == "MACRO1");
        assert!(macro1.is_some());

        // X-Macro pattern - Miller tests X-macro techniques
        let error_codes = symbols.iter().find(|s| s.name == "ERROR_CODES");
        assert!(error_codes.is_some());

        let enum_entry = symbols.iter().find(|s| s.name == "ENUM_ENTRY");
        assert!(enum_entry.is_some());

        let error_code_enum = symbols.iter().find(|s| s.name == "ErrorCode");
        assert!(error_code_enum.is_some());

        let error_strings = symbols.iter().find(|s| s.name == "error_strings");
        assert!(error_strings.is_some());

        let get_error_string = symbols.iter().find(|s| s.name == "get_error_string");
        assert!(get_error_string.is_some());

        // Generated getter/setter - Miller tests macro generation
        let declare_getter_setter = symbols.iter().find(|s| s.name == "DECLARE_GETTER_SETTER");
        assert!(declare_getter_setter.is_some());

        let global_value = symbols.iter().find(|s| s.name == "global_value");
        assert!(global_value.is_some());

        // C11 specific features - Miller tests C standard features
        let atomic_int = symbols.iter().find(|s| s.name == "AtomicInt");
        assert!(atomic_int.is_some());

        let atomic_increment = symbols.iter().find(|s| s.name == "atomic_increment");
        assert!(atomic_increment.is_some());

        // Compiler optimization attributes - Miller tests optimization directives
        let optimize_size = symbols.iter().find(|s| s.name == "OPTIMIZE_SIZE");
        assert!(optimize_size.is_some());

        let optimize_speed = symbols.iter().find(|s| s.name == "OPTIMIZE_SPEED");
        assert!(optimize_speed.is_some());

        let no_optimize = symbols.iter().find(|s| s.name == "NO_OPTIMIZE");
        assert!(no_optimize.is_some());

        // Optimization-specific functions - Miller tests optimization annotations
        let fast_function = symbols.iter().find(|s| s.name == "fast_function");
        assert!(fast_function.is_some());

        let small_function = symbols.iter().find(|s| s.name == "small_function");
        assert!(small_function.is_some());

        let debug_function = symbols.iter().find(|s| s.name == "debug_function");
        assert!(debug_function.is_some());

        // Standard library includes for conditional features - Miller tests conditional includes
        let socket_include = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains("#include <sys/socket.h>"))
        );
        assert!(socket_include.is_some());

        let stdatomic_include = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains("#include <stdatomic.h>"))
        );
        assert!(stdatomic_include.is_some());

        let threads_include = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains("#include <threads.h>"))
        );
        assert!(threads_include.is_some());
    }
}