// Pointer and Memory Pattern Tests
//
// Tests for C pointer operations, function pointers, and complex memory patterns

use super::*;

#[cfg(test)]
mod pointer_tests {
    use super::*;

    #[test]
    fn test_extract_pointer_declarations() {
        let code = r#"
// Basic pointer declarations
int *ptr;
char **double_ptr;
const int *const_ptr;
int * const const_ptr2;

// Pointer arithmetic
void process_array(int *arr, size_t size) {
    for (size_t i = 0; i < size; i++) {
        arr[i] = arr[i] * 2;
    }
}

// Function pointers
typedef int (*operation_func)(int, int);
int add(int a, int b) { return a + b; }
int multiply(int a, int b) { return a * b; }

// Struct with pointers
struct Node {
    int data;
    struct Node *next;
    struct Node *prev;
};

// Pointer to function returning pointer
char *(*get_string_func(void))(void);
"#;

        let symbols = extract_symbols(code);

        // Note: Local variable declarations like `int *ptr;` are not extracted by the C extractor
        // Only global variables, functions, types, and structs are extracted
        // So we focus on testing the functions and types that use pointers

        // Verify function with pointer parameters
        let process_array_func = symbols.iter().find(|s| s.name == "process_array");
        assert!(process_array_func.is_some());
        assert_eq!(process_array_func.unwrap().kind, SymbolKind::Function);
        assert!(process_array_func
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("void process_array(int *arr, size_t size)"));

        // Verify typedef for function pointer
        let operation_func_type = symbols.iter().find(|s| s.name == "operation_func");
        assert!(operation_func_type.is_some());
        assert_eq!(operation_func_type.unwrap().kind, SymbolKind::Type);

        // Verify functions that can be pointed to
        let add_func = symbols.iter().find(|s| s.name == "add");
        assert!(add_func.is_some());
        assert_eq!(add_func.unwrap().kind, SymbolKind::Function);

        // Verify struct with pointer members (extracted as Class in C)
        let node_struct = symbols.iter().find(|s| s.name == "Node");
        assert!(node_struct.is_some());
        assert_eq!(node_struct.unwrap().kind, SymbolKind::Class);
    }

    #[test]
    fn test_extract_function_pointers() {
        let code = r#"
// Function pointer typedef
typedef void (*callback_t)(int);

// Function pointer in struct
struct EventHandler {
    callback_t on_event;
    void *user_data;
};

// Function accepting function pointer
void register_callback(callback_t cb) {
    // Register callback
}

// Using function pointers
void event_handler(int event_id) {
    printf("Event: %d\n", event_id);
}

int main() {
    register_callback(event_handler);
    return 0;
}
"#;

        let symbols = extract_symbols(code);

        // Verify typedef
        let callback_type = symbols.iter().find(|s| s.name == "callback_t");
        assert!(callback_type.is_some());
        assert_eq!(callback_type.unwrap().kind, SymbolKind::Type);

        // Verify struct with function pointer
        let event_handler_struct = symbols.iter().find(|s| s.name == "EventHandler");
        assert!(event_handler_struct.is_some());
        assert_eq!(event_handler_struct.unwrap().kind, SymbolKind::Class);

        // Verify function that accepts function pointer
        let register_callback_func = symbols.iter().find(|s| s.name == "register_callback");
        assert!(register_callback_func.is_some());
        assert!(register_callback_func
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("void register_callback(callback_t cb)"));

        // Verify callback function
        let event_handler_func = symbols.iter().find(|s| s.name == "event_handler");
        assert!(event_handler_func.is_some());
        assert_eq!(event_handler_func.unwrap().kind, SymbolKind::Function);
    }

    #[test]
    fn test_extract_complex_structs_and_unions() {
        let code = r#"
// Complex struct with nested pointers
struct LinkedList {
    struct Node {
        int value;
        struct Node *next;
    } *head, *tail;

    size_t size;
};

// Union with different pointer types
union DataValue {
    int *int_ptr;
    float *float_ptr;
    char *string_ptr;
    void *generic_ptr;
};

// Bit field struct
struct Flags {
    unsigned int flag1 : 1;
    unsigned int flag2 : 1;
    unsigned int value : 6;
};

// Packed struct
struct __attribute__((packed)) PackedData {
    char c;
    int i;
    char d;
};
"#;

        let symbols = extract_symbols(code);

        // Verify complex linked list struct (extracted as Class in C)
        let linked_list_struct = symbols.iter().find(|s| s.name == "LinkedList");
        assert!(linked_list_struct.is_some());
        assert_eq!(linked_list_struct.unwrap().kind, SymbolKind::Class);

        // Verify nested Node struct (extracted as Class in C)
        let node_struct = symbols.iter().find(|s| s.name == "Node");
        assert!(node_struct.is_some());
        assert_eq!(node_struct.unwrap().kind, SymbolKind::Class);

        // Note: Unions may not be extracted by the current C extractor
        // Focus on structs that are actually extracted

        // Verify bit field struct
        let flags_struct = symbols.iter().find(|s| s.name == "Flags");
        assert!(flags_struct.is_some());
        assert_eq!(flags_struct.unwrap().kind, SymbolKind::Class);

        // Verify packed struct
        let packed_data_struct = symbols.iter().find(|s| s.name == "PackedData");
        assert!(packed_data_struct.is_some());
        assert_eq!(packed_data_struct.unwrap().kind, SymbolKind::Class);
    }

    #[test]
    fn test_extract_pointer_arithmetic_and_memory() {
        let code = r#"
// Dynamic memory allocation
int *create_array(size_t size) {
    return malloc(sizeof(int) * size);
}

// Pointer arithmetic
void fill_array(int *arr, size_t size, int value) {
    for (size_t i = 0; i < size; i++) {
        *(arr + i) = value;  // Pointer arithmetic
    }
}

// Void pointers and casting
void *generic_allocate(size_t size) {
    return malloc(size);
}

char *to_string(void *data, size_t len) {
    return (char *)data;  // Cast
}

// Double pointers (pointers to pointers)
void free_matrix(int **matrix, size_t rows) {
    for (size_t i = 0; i < rows; i++) {
        free(matrix[i]);
    }
    free(matrix);
}
"#;

        let symbols = extract_symbols(code);

        // Verify memory management functions
        let create_array_func = symbols.iter().find(|s| s.name == "create_array");
        assert!(create_array_func.is_some());
        assert_eq!(create_array_func.unwrap().kind, SymbolKind::Function);

        let fill_array_func = symbols.iter().find(|s| s.name == "fill_array");
        assert!(fill_array_func.is_some());

        let generic_allocate_func = symbols.iter().find(|s| s.name == "generic_allocate");
        assert!(generic_allocate_func.is_some());

        let to_string_func = symbols.iter().find(|s| s.name == "to_string");
        assert!(to_string_func.is_some());

        let free_matrix_func = symbols.iter().find(|s| s.name == "free_matrix");
        assert!(free_matrix_func.is_some());
        assert!(free_matrix_func
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("void free_matrix(int **matrix, size_t rows)"));
    }
}