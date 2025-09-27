use super::{parse_c, SymbolKind};

#[cfg(test)]
mod tests {
    use super::*;

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
        let (mut extractor, tree) = parse_c(code, "basic.c");
        let symbols = extractor.extract_symbols(&tree);

        // Include statements - Miller expects these to be found
        let stdio_include = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains("#include <stdio.h>"))
        });
        assert!(stdio_include.is_some());
        assert_eq!(stdio_include.unwrap().kind, SymbolKind::Import);

        let custom_include = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains("#include \"custom_header.h\""))
        });
        assert!(custom_include.is_some());

        // Macro definitions - Miller tests these specific macros
        let max_size_macro = symbols.iter().find(|s| s.name == "MAX_SIZE");
        assert!(max_size_macro.is_some());
        assert_eq!(max_size_macro.unwrap().kind, SymbolKind::Constant);
        assert!(max_size_macro
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("#define MAX_SIZE 1024"));

        let min_macro = symbols.iter().find(|s| s.name == "MIN");
        assert!(min_macro.is_some());
        assert!(min_macro
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("#define MIN(a, b)"));

        let debug_macro = symbols.iter().find(|s| s.name == "DEBUG");
        assert!(debug_macro.is_some());

        let log_macro = symbols.iter().find(|s| s.name == "LOG");
        assert!(log_macro.is_some());

        // Typedefs - Miller checks these type definitions
        let error_code_typedef = symbols.iter().find(|s| s.name == "ErrorCode");
        assert!(error_code_typedef.is_some());
        assert_eq!(error_code_typedef.as_ref().unwrap().kind, SymbolKind::Type);
        let error_code_signature = error_code_typedef
            .as_ref()
            .unwrap()
            .signature
            .as_ref()
            .unwrap();
        assert!(error_code_signature.contains("typedef"));
        assert!(error_code_signature.contains("ErrorCode"));

        let uint64_typedef = symbols.iter().find(|s| s.name == "uint64_t");
        assert!(uint64_typedef.is_some());

        let string_typedef = symbols.iter().find(|s| s.name == "String");
        assert!(string_typedef.is_some());

        let point_typedef = symbols.iter().find(|s| s.name == "Point");
        assert!(point_typedef.is_some());
        assert!(point_typedef
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("typedef struct Point"));

        // Struct definitions - Miller expects these as classes
        let point_struct = symbols.iter().find(|s| {
            s.name == "Point"
                && s.kind == SymbolKind::Class
                && s.signature
                    .as_ref()
                    .map_or(false, |sig| sig.contains("struct Point"))
        });
        assert!(point_struct.is_some());
        assert_eq!(point_struct.unwrap().kind, SymbolKind::Class); // Structs as classes

        let rectangle_struct = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains("struct Rectangle"))
        });
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
        assert!(global_counter
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("int global_counter = 0"));

        let static_counter = symbols.iter().find(|s| s.name == "static_counter");
        assert!(static_counter.is_some());
        assert!(static_counter
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("static int static_counter"));

        let external_counter = symbols.iter().find(|s| s.name == "external_counter");
        assert!(external_counter.is_some());
        assert!(external_counter
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("extern int external_counter"));

        let pi_constant = symbols.iter().find(|s| s.name == "PI");
        assert!(pi_constant.is_some());
        assert!(pi_constant
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("const double PI"));

        let volatile_flag = symbols.iter().find(|s| s.name == "interrupt_flag");
        assert!(volatile_flag.is_some());
        assert!(volatile_flag
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("volatile int interrupt_flag"));

        // Array variables - Miller tests array declarations
        let global_buffer = symbols.iter().find(|s| s.name == "global_buffer");
        assert!(global_buffer.is_some());
        assert!(global_buffer
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("char global_buffer[MAX_SIZE]"));

        let origin = symbols.iter().find(|s| s.name == "origin");
        assert!(origin.is_some());
        assert!(origin
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("Point origin = {0.0, 0.0}"));

        // Function declarations and definitions - Miller tests various function types
        let add_function = symbols.iter().find(|s| s.name == "add");
        assert!(add_function.is_some());
        assert_eq!(add_function.unwrap().kind, SymbolKind::Function);
        assert!(add_function
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("int add(int a, int b)"));

        let subtract_function = symbols.iter().find(|s| s.name == "subtract");
        assert!(subtract_function.is_some());

        let multiply_function = symbols.iter().find(|s| s.name == "multiply");
        assert!(multiply_function.is_some());
        assert!(multiply_function
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("double multiply(double x, double y)"));

        // Complex parameter functions - Miller tests complex signatures
        let process_data_function = symbols.iter().find(|s| s.name == "process_data");
        assert!(process_data_function.is_some());
        assert!(process_data_function
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("ErrorCode process_data(const char* input"));

        let swap_function = symbols.iter().find(|s| s.name == "swap_integers");
        assert!(swap_function.is_some());
        assert!(swap_function
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("void swap_integers(int* a, int* b)"));

        let sum_array_function = symbols.iter().find(|s| s.name == "sum_array");
        assert!(sum_array_function.is_some());
        assert!(sum_array_function
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("double sum_array(const double arr[], int count)"));

        // Variadic function - Miller tests variadic parameters
        let sum_variadic_function = symbols.iter().find(|s| s.name == "sum_variadic");
        assert!(sum_variadic_function.is_some());
        assert!(sum_variadic_function
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("int sum_variadic(int count, ...)"));

        // Static function - Miller tests static functions
        let internal_helper = symbols.iter().find(|s| s.name == "internal_helper");
        assert!(internal_helper.is_some());
        assert!(internal_helper
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("static void internal_helper()"));

        // Inline function - Miller tests inline functions
        let square_function = symbols.iter().find(|s| s.name == "square");
        assert!(square_function.is_some());
        assert!(square_function
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("inline int square(int x)"));

        // Function returning pointer - Miller tests pointer return types
        let create_greeting = symbols.iter().find(|s| s.name == "create_greeting");
        assert!(create_greeting.is_some());
        assert!(create_greeting
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("char* create_greeting(const char* name)"));

        // Function pointer typedef - ensure function pointer name appears in extracted signatures
        assert!(symbols.iter().any(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains("CompareFn"))
        }));

        // Function with function pointer parameter - Miller tests complex parameter types
        let sort_array_function = symbols.iter().find(|s| s.name == "sort_array");
        assert!(sort_array_function.is_some());
        assert!(sort_array_function
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("CompareFn compare"));

        // Comparison functions - Miller tests these implementations
        let compare_integers = symbols.iter().find(|s| s.name == "compare_integers");
        assert!(compare_integers.is_some());

        let compare_strings = symbols.iter().find(|s| s.name == "compare_strings");
        assert!(compare_strings.is_some());

        // Main function - Miller tests main function signature
        let main_function = symbols.iter().find(|s| s.name == "main");
        assert!(main_function.is_some());
        assert!(main_function
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("int main(int argc, char* argv[])"));
    }
}
