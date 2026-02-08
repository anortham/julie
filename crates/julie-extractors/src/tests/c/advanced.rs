use super::{SymbolKind, parse_c};

#[cfg(test)]
mod tests {
    use super::*;

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
        let (mut extractor, tree) = parse_c(code, "advanced.c");
        let symbols = extractor.extract_symbols(&tree);

        // Advanced macros - Tests sophisticated preprocessing
        let stringify_macro = symbols.iter().find(|s| s.name == "STRINGIFY");
        assert!(stringify_macro.is_some());
        assert!(
            stringify_macro
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("#define STRINGIFY(x) #x")
        );

        let concat_macro = symbols.iter().find(|s| s.name == "CONCAT");
        assert!(concat_macro.is_some());

        let array_size_macro = symbols.iter().find(|s| s.name == "ARRAY_SIZE");
        assert!(array_size_macro.is_some());

        // Complex structs - Tests advanced data structures
        let account_struct = symbols.iter().find(|s| s.name == "Account");
        assert!(account_struct.is_some());
        assert_eq!(account_struct.unwrap().kind, SymbolKind::Struct);
        assert!(
            account_struct
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("typedef struct")
        );

        let node_struct = symbols.iter().find(|s| s.name == "Node");
        assert!(node_struct.is_some());

        let tree_node_struct = symbols.iter().find(|s| s.name == "TreeNode");
        assert!(tree_node_struct.is_some());

        // Union types - Tests union extraction
        let value_union = symbols.iter().find(|s| s.name == "Value");
        assert!(value_union.is_some());
        let value_union_symbol = value_union.as_ref().unwrap();
        let value_union_signature = value_union_symbol.signature.as_ref().unwrap();
        assert!(value_union_signature.contains("typedef"));
        assert!(value_union_signature.contains("typedef struct Value"));
        let metadata = value_union_symbol
            .metadata
            .as_ref()
            .expect("Union metadata");
        let underlying_type = metadata
            .get("underlyingType")
            .and_then(|v| v.as_str())
            .expect("Union underlying type");
        assert!(underlying_type.contains("union"));

        // Function pointer typedefs appear inside struct metadata rather than standalone symbols
        let operation_entry_struct = symbols
            .iter()
            .find(|s| s.name == "OperationEntry")
            .expect("OperationEntry struct");
        let operation_entry_metadata = operation_entry_struct
            .metadata
            .as_ref()
            .expect("OperationEntry metadata");
        let entry_underlying = operation_entry_metadata
            .get("underlyingType")
            .and_then(|v| v.as_str())
            .expect("OperationEntry underlying type");
        assert!(entry_underlying.contains("BinaryOperation"));

        // Struct with function pointers - Tests virtual table patterns
        let generic_object_struct = symbols
            .iter()
            .find(|s| s.name == "GenericObject")
            .expect("GenericObject struct");
        let generic_object_metadata = generic_object_struct
            .metadata
            .as_ref()
            .expect("GenericObject metadata");
        let generic_object_underlying = generic_object_metadata
            .get("underlyingType")
            .and_then(|v| v.as_str())
            .expect("GenericObject underlying type");
        assert!(generic_object_underlying.contains("void (*destroy)(void* data)"));
        assert!(generic_object_underlying.contains("void* (*clone)(const void* data)"));

        // Bit fields - Tests bit field structures
        let flags_struct = symbols
            .iter()
            .find(|s| s.name == "Flags")
            .expect("Flags struct");
        let flags_underlying = flags_struct
            .metadata
            .as_ref()
            .and_then(|meta| meta.get("underlyingType"))
            .and_then(|v| v.as_str())
            .expect("Flags underlying type");
        assert!(flags_underlying.contains("unsigned int is_valid : 1"));

        let network_packet_struct = symbols
            .iter()
            .find(|s| s.name == "NetworkPacket")
            .expect("NetworkPacket struct");
        let network_packet_underlying = network_packet_struct
            .metadata
            .as_ref()
            .and_then(|meta| meta.get("underlyingType"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(network_packet_underlying.contains("Flags"));

        // Memory management structures - Tests memory pool patterns
        assert!(symbols.iter().any(|s| s.name == "MemoryBlock"));

        assert!(symbols.iter().any(|s| s.name == "MemoryPool"));

        // Global arrays - Tests array declarations
        let operation_table = symbols
            .iter()
            .find(|s| s.name == "operation_table")
            .expect("Operation table array");
        assert!(
            operation_table
                .signature
                .as_ref()
                .unwrap()
                .contains("OperationEntry operation_table[]")
        );

        // Memory management functions - Tests function signatures
        let create_memory_pool = symbols.iter().find(|s| s.name == "create_memory_pool");
        assert!(create_memory_pool.is_some());
        assert!(
            create_memory_pool
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("MemoryPool* create_memory_pool(size_t total_size)")
        );

        // Signal handling - Tests signal handling patterns
        let shutdown_flag = symbols.iter().find(|s| s.name == "shutdown_flag");
        assert!(shutdown_flag.is_some());
        assert!(
            shutdown_flag
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("volatile sig_atomic_t shutdown_flag")
        );

        let signal_handler = symbols.iter().find(|s| s.name == "signal_handler");
        assert!(signal_handler.is_some());
        assert!(
            signal_handler
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("void signal_handler(int signum)")
        );

        // Extern "C" handling - Tests linkage specifications
        let extern_c = symbols
            .iter()
            .filter(|s| {
                s.signature
                    .as_ref()
                    .map_or(false, |sig| sig.contains("extern \"C\""))
            })
            .count();
        assert!(extern_c >= 1);

        // Standard library includes - Tests include extraction
        let stdint_include = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains("#include <stdint.h>"))
        });
        assert!(stdint_include.is_some());

        let stdbool_include = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains("#include <stdbool.h>"))
        });
        assert!(stdbool_include.is_some());

        let complex_include = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains("#include <complex.h>"))
        });
        assert!(complex_include.is_some());

        let signal_include = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains("#include <signal.h>"))
        });
        assert!(signal_include.is_some());

        let unistd_include = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains("#include <unistd.h>"))
        });
        assert!(unistd_include.is_some());
    }
}
