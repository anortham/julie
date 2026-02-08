use super::{SymbolKind, parse_c};

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

        // Include statements - expects these to be found
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

        // Macro definitions - Tests these specific macros
        let max_size_macro = symbols.iter().find(|s| s.name == "MAX_SIZE");
        assert!(max_size_macro.is_some());
        assert_eq!(max_size_macro.unwrap().kind, SymbolKind::Constant);
        assert!(
            max_size_macro
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("#define MAX_SIZE 1024")
        );

        let min_macro = symbols.iter().find(|s| s.name == "MIN");
        assert!(min_macro.is_some());
        assert!(
            min_macro
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("#define MIN(a, b)")
        );

        let debug_macro = symbols.iter().find(|s| s.name == "DEBUG");
        assert!(debug_macro.is_some());

        let log_macro = symbols.iter().find(|s| s.name == "LOG");
        assert!(log_macro.is_some());

        // Typedefs - checks these type definitions
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
        assert!(
            point_typedef
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("typedef struct Point")
        );

        // Struct definitions
        let point_struct = symbols.iter().find(|s| {
            s.name == "Point"
                && s.kind == SymbolKind::Struct
                && s.signature
                    .as_ref()
                    .map_or(false, |sig| sig.contains("struct Point"))
        });
        assert!(point_struct.is_some());
        assert_eq!(point_struct.unwrap().kind, SymbolKind::Struct);

        let rectangle_struct = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains("struct Rectangle"))
        });
        assert!(rectangle_struct.is_some());

        // Enum definitions - Tests enum extraction
        let status_enum = symbols.iter().find(|s| s.name == "Status");
        assert!(status_enum.is_some());
        assert_eq!(status_enum.unwrap().kind, SymbolKind::Enum);

        let log_level_enum = symbols.iter().find(|s| s.name == "LogLevel");
        assert!(log_level_enum.is_some());

        // Enum values - extracts these as constants
        let status_success = symbols.iter().find(|s| s.name == "STATUS_SUCCESS");
        assert!(status_success.is_some());
        assert_eq!(status_success.unwrap().kind, SymbolKind::Constant);

        let log_debug = symbols.iter().find(|s| s.name == "LOG_LEVEL_DEBUG");
        assert!(log_debug.is_some());

        // Global variables - Tests various variable types
        let global_counter = symbols.iter().find(|s| s.name == "global_counter");
        assert!(global_counter.is_some());
        assert_eq!(global_counter.unwrap().kind, SymbolKind::Variable);
        assert!(
            global_counter
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("int global_counter = 0")
        );

        let static_counter = symbols.iter().find(|s| s.name == "static_counter");
        assert!(static_counter.is_some());
        assert!(
            static_counter
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("static int static_counter")
        );

        let external_counter = symbols.iter().find(|s| s.name == "external_counter");
        assert!(external_counter.is_some());
        assert!(
            external_counter
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("extern int external_counter")
        );

        let pi_constant = symbols.iter().find(|s| s.name == "PI");
        assert!(pi_constant.is_some());
        assert!(
            pi_constant
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("const double PI")
        );

        let volatile_flag = symbols.iter().find(|s| s.name == "interrupt_flag");
        assert!(volatile_flag.is_some());
        assert!(
            volatile_flag
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("volatile int interrupt_flag")
        );

        // Array variables - Tests array declarations
        let global_buffer = symbols.iter().find(|s| s.name == "global_buffer");
        assert!(global_buffer.is_some());
        assert!(
            global_buffer
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("char global_buffer[MAX_SIZE]")
        );

        let origin = symbols.iter().find(|s| s.name == "origin");
        assert!(origin.is_some());
        assert!(
            origin
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("Point origin = {0.0, 0.0}")
        );

        // Function declarations and definitions - Tests various function types
        let add_function = symbols.iter().find(|s| s.name == "add");
        assert!(add_function.is_some());
        assert_eq!(add_function.unwrap().kind, SymbolKind::Function);
        assert!(
            add_function
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("int add(int a, int b)")
        );

        let subtract_function = symbols.iter().find(|s| s.name == "subtract");
        assert!(subtract_function.is_some());

        let multiply_function = symbols.iter().find(|s| s.name == "multiply");
        assert!(multiply_function.is_some());
        assert!(
            multiply_function
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("double multiply(double x, double y)")
        );

        // Complex parameter functions - Tests complex signatures
        let process_data_function = symbols.iter().find(|s| s.name == "process_data");
        assert!(process_data_function.is_some());
        assert!(
            process_data_function
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("ErrorCode process_data(const char* input")
        );

        let swap_function = symbols.iter().find(|s| s.name == "swap_integers");
        assert!(swap_function.is_some());
        assert!(
            swap_function
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("void swap_integers(int* a, int* b)")
        );

        let sum_array_function = symbols.iter().find(|s| s.name == "sum_array");
        assert!(sum_array_function.is_some());
        assert!(
            sum_array_function
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("double sum_array(const double arr[], int count)")
        );

        // Variadic function - Tests variadic parameters
        let sum_variadic_function = symbols.iter().find(|s| s.name == "sum_variadic");
        assert!(sum_variadic_function.is_some());
        assert!(
            sum_variadic_function
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("int sum_variadic(int count, ...)")
        );

        // Static function - Tests static functions
        let internal_helper = symbols.iter().find(|s| s.name == "internal_helper");
        assert!(internal_helper.is_some());
        assert!(
            internal_helper
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("static void internal_helper()")
        );

        // Inline function - Tests inline functions
        let square_function = symbols.iter().find(|s| s.name == "square");
        assert!(square_function.is_some());
        assert!(
            square_function
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("inline int square(int x)")
        );

        // Function returning pointer - Tests pointer return types
        let create_greeting = symbols.iter().find(|s| s.name == "create_greeting");
        assert!(create_greeting.is_some());
        assert!(
            create_greeting
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("char* create_greeting(const char* name)")
        );

        // Function pointer typedef - ensure function pointer name appears in extracted signatures
        assert!(symbols.iter().any(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains("CompareFn"))
        }));

        // Function with function pointer parameter - Tests complex parameter types
        let sort_array_function = symbols.iter().find(|s| s.name == "sort_array");
        assert!(sort_array_function.is_some());
        assert!(
            sort_array_function
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("CompareFn compare")
        );

        // Comparison functions - Tests these implementations
        let compare_integers = symbols.iter().find(|s| s.name == "compare_integers");
        assert!(compare_integers.is_some());

        let compare_strings = symbols.iter().find(|s| s.name == "compare_strings");
        assert!(compare_strings.is_some());

        // Main function - Tests main function signature
        let main_function = symbols.iter().find(|s| s.name == "main");
        assert!(main_function.is_some());
        assert!(
            main_function
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("int main(int argc, char* argv[])")
        );
    }

    #[test]
    fn test_extract_advanced_variadic_functions() {
        let code = r#"
    #include <stdio.h>
    #include <stdarg.h>

    // Advanced variadic function with type checking
    int sum_integers(int count, ...) {
        va_list args;
        va_start(args, count);
        int sum = 0;
        for(int i = 0; i < count; i++) {
            sum += va_arg(args, int);
        }
        va_end(args);
        return sum;
    }

    // Variadic function with mixed types
    double average(int count, ...) {
        va_list args;
        va_start(args, count);
        double sum = 0.0;
        for(int i = 0; i < count; i++) {
            sum += va_arg(args, double);
        }
        va_end(args);
        return sum / count;
    }

    // Printf-style variadic function
    void log_message(const char* format, ...) {
        va_list args;
        va_start(args, format);
        vprintf(format, args);
        va_end(args);
    }

    // Variadic function with struct parameters
    typedef struct {
        int x, y;
    } Point;

    Point create_point(int x, int y) {
        return (Point){x, y};
    }

    void draw_polygon(int num_points, ...) {
        va_list args;
        va_start(args, num_points);

        printf("Drawing polygon with %d points:\n", num_points);
        for(int i = 0; i < num_points; i++) {
            Point p = va_arg(args, Point);
            printf("  Point %d: (%d, %d)\n", i+1, p.x, p.y);
        }

        va_end(args);
    }

    // Recursive variadic template-like function (simulated)
    int max_value(int first, ...) {
        va_list args;
        va_start(args, first);

        int max = first;
        int value;
        while((value = va_arg(args, int)) != -1) {  // -1 sentinel
            if(value > max) max = value;
        }

        va_end(args);
        return max;
    }

    int main() {
        // Test variadic functions
        int sum = sum_integers(4, 1, 2, 3, 4);
        double avg = average(3, 1.5, 2.5, 3.5);
        log_message("Sum: %d, Average: %.2f\n", sum, avg);

        Point p1 = create_point(0, 0);
        Point p2 = create_point(10, 0);
        Point p3 = create_point(10, 10);
        Point p4 = create_point(0, 10);

        draw_polygon(4, p1, p2, p3, p4);

        int maximum = max_value(5, 10, 3, 8, 1, -1);
        printf("Maximum value: %d\n", maximum);

        return 0;
    }
    "#;
        let (mut extractor, tree) = parse_c(code, "variadic.c");
        let symbols = extractor.extract_symbols(&tree);

        // Advanced variadic function with type checking
        let sum_integers = symbols.iter().find(|s| s.name == "sum_integers");
        assert!(sum_integers.is_some());
        assert!(
            sum_integers
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("int sum_integers(int count, ...)")
        );

        // Variadic function with mixed types
        let average_func = symbols.iter().find(|s| s.name == "average");
        assert!(average_func.is_some());
        assert!(
            average_func
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("double average(int count, ...)")
        );

        // Printf-style variadic function
        let log_message = symbols.iter().find(|s| s.name == "log_message");
        assert!(log_message.is_some());
        assert!(
            log_message
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("void log_message(const char* format, ...)")
        );

        // Variadic function with struct parameters
        let draw_polygon = symbols.iter().find(|s| s.name == "draw_polygon");
        assert!(draw_polygon.is_some());
        assert!(
            draw_polygon
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("void draw_polygon(int num_points, ...)")
        );

        // Recursive variadic function
        let max_value = symbols.iter().find(|s| s.name == "max_value");
        assert!(max_value.is_some());
        assert!(
            max_value
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("int max_value(int first, ...)")
        );
    }

    #[test]
    fn test_extract_static_extern_linkage_patterns() {
        let code = r#"
    // External declarations
    extern int global_counter;
    extern void external_function(void);
    extern const char* get_version(void);

    // Static file-scoped variables
    static int file_static_var = 42;
    static const char* file_static_string = "file scope";

    // Static file-scoped functions
    static void file_static_function(void) {
        static int call_count = 0;
        call_count++;
        printf("Called %d times\n", call_count);
    }

    static int file_static_helper(int x) {
        return x * 2;
    }

    // Module with internal linkage
    typedef struct {
        int id;
        char name[50];
    } Record;

    static Record* records = NULL;
    static size_t record_count = 0;

    static void init_records(void) {
        records = calloc(10, sizeof(Record));
        record_count = 0;
    }

    static Record* find_record(int id) {
        for(size_t i = 0; i < record_count; i++) {
            if(records[i].id == id) {
                return &records[i];
            }
        }
        return NULL;
    }

    static void add_record(int id, const char* name) {
        if(record_count < 10) {
            records[record_count].id = id;
            strncpy(records[record_count].name, name, 49);
            record_count++;
        }
    }

    // Public interface functions
    void record_init(void) {
        init_records();
    }

    Record* record_find(int id) {
        return find_record(id);
    }

    void record_add(int id, const char* name) {
        add_record(id, name);
    }

    // External linkage override
    extern inline int external_inline_function(int x) {
        return x + 1;
    }

    // Static inline functions
    static inline int static_inline_helper(int a, int b) {
        return a > b ? a : b;
    }

    static inline void* static_inline_alloc(size_t size) {
        return malloc(size);
    }

    // Function with mixed linkage
    int public_function(int x) {
        static int static_local = 0;
        static_local += x;
        return static_local + file_static_helper(x);
    }

    // External reference to standard library
    extern FILE* stdin;
    extern FILE* stdout;
    extern FILE* stderr;

    // External math functions
    extern double sin(double);
    extern double cos(double);
    extern double sqrt(double);
    "#;
        let (mut extractor, tree) = parse_c(code, "linkage.c");
        let symbols = extractor.extract_symbols(&tree);

        // External declarations
        let global_counter = symbols.iter().find(|s| s.name == "global_counter");
        assert!(global_counter.is_some());
        assert!(
            global_counter
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("int global_counter")
        );

        let external_function = symbols.iter().find(|s| s.name == "external_function");
        assert!(external_function.is_some());
        assert!(
            external_function
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("void external_function(void)")
        );

        // Static file-scoped variables
        let file_static_var = symbols.iter().find(|s| s.name == "file_static_var");
        assert!(file_static_var.is_some());
        assert_eq!(file_static_var.unwrap().kind, SymbolKind::Variable);

        let file_static_string = symbols.iter().find(|s| s.name == "file_static_string");
        assert!(file_static_string.is_some());
        assert_eq!(file_static_string.unwrap().kind, SymbolKind::Variable);

        // Static file-scoped functions
        let file_static_function = symbols.iter().find(|s| s.name == "file_static_function");
        assert!(file_static_function.is_some());
        assert!(
            file_static_function
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("static void file_static_function(void)")
        );

        let file_static_helper = symbols.iter().find(|s| s.name == "file_static_helper");
        assert!(file_static_helper.is_some());
        assert!(
            file_static_helper
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("static int file_static_helper(int x)")
        );

        // Static module variables
        let records = symbols.iter().find(|s| s.name == "records");
        assert!(records.is_some());
        assert_eq!(records.unwrap().kind, SymbolKind::Variable);

        let record_count = symbols.iter().find(|s| s.name == "record_count");
        assert!(record_count.is_some());
        assert_eq!(record_count.unwrap().kind, SymbolKind::Variable);

        // Static helper functions
        let init_records = symbols.iter().find(|s| s.name == "init_records");
        assert!(init_records.is_some());
        assert_eq!(init_records.unwrap().kind, SymbolKind::Function);

        let find_record = symbols.iter().find(|s| s.name == "find_record");
        assert!(find_record.is_some());
        assert_eq!(find_record.unwrap().kind, SymbolKind::Function);

        // External linkage override
        let external_inline_function = symbols
            .iter()
            .find(|s| s.name == "external_inline_function");
        assert!(external_inline_function.is_some());
        assert_eq!(external_inline_function.unwrap().kind, SymbolKind::Function);

        // Static inline functions
        let static_inline_helper = symbols.iter().find(|s| s.name == "static_inline_helper");
        assert!(static_inline_helper.is_some());
        assert!(
            static_inline_helper
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("static inline int static_inline_helper(int a, int b)")
        );

        // Function with mixed linkage
        let public_function = symbols.iter().find(|s| s.name == "public_function");
        assert!(public_function.is_some());
        assert!(
            public_function
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("int public_function(int x)")
        );
    }

    #[test]
    fn test_extract_complex_typedef_chains() {
        let code = r#"
    // Basic typedefs
    typedef int Integer;
    typedef Integer Number;

    // Pointer typedef chains
    typedef char* String;
    typedef String* StringArray;
    typedef StringArray* StringMatrix;

    // Function pointer typedefs
    typedef int (*CompareFunc)(const void*, const void*);
    typedef void (*Callback)(void* context);
    typedef CompareFunc* CompareFuncPtr;

    // Struct typedef chains
    typedef struct {
        int x, y;
    } Point;

    typedef struct {
        Point start;
        Point end;
    } Line;

    typedef struct {
        Line* lines;
        size_t count;
    } Polygon;

    typedef Polygon* PolygonPtr;
    typedef PolygonPtr* PolygonArray;

    // Enum typedef chains
    typedef enum {
        RED, GREEN, BLUE
    } Color;

    typedef enum {
        CIRCLE, SQUARE, TRIANGLE
    } ShapeType;

    typedef struct {
        ShapeType type;
        Color color;
        union {
            struct { Point center; int radius; } circle;
            struct { Point corner; int width, height; } rectangle;
            struct { Point vertices[3]; } triangle;
        } data;
    } Shape;

    typedef Shape* ShapePtr;
    typedef ShapePtr* ShapeArray;

    // Complex function typedefs
    typedef int (*BinaryOp)(int, int);
    typedef BinaryOp* BinaryOpPtr;
    typedef BinaryOpPtr* BinaryOpArray;

    typedef struct {
        const char* name;
        BinaryOp operation;
    } Operation;

    typedef Operation* OperationPtr;
    typedef OperationPtr (*OperationFactory)(void);

    // Callback system typedefs
    typedef struct Event Event;
    typedef void (*EventHandler)(Event* event, void* context);
    typedef EventHandler* EventHandlerPtr;

    struct Event {
        int type;
        void* data;
        EventHandlerPtr handlers;
        size_t handler_count;
    };

    typedef struct {
        Event* events;
        size_t event_count;
        void* context;
    } EventSystem;

    typedef EventSystem* EventSystemPtr;

    // Generic container typedefs
    typedef void* GenericData;
    typedef size_t (*HashFunc)(GenericData);
    typedef int (*CompareFuncGeneric)(GenericData, GenericData);
    typedef void (*DestroyFunc)(GenericData);

    typedef struct {
        GenericData* items;
        size_t count;
        size_t capacity;
        HashFunc hash;
        CompareFuncGeneric compare;
        DestroyFunc destroy;
    } GenericSet;

    typedef GenericSet* GenericSetPtr;
    typedef GenericSetPtr* GenericSetArray;

    // Usage examples
    Integer num = 42;
    Number value = num;

    String str = "hello";
    StringArray strings = &str;
    StringMatrix matrix = &strings;

    Point p = {1, 2};
    Line l = {p, {3, 4}};
    Polygon poly = {&l, 1};
    PolygonPtr poly_ptr = &poly;
    PolygonArray poly_array = &poly_ptr;

    Shape circle = {CIRCLE, RED, {.circle = {{0, 0}, 5}}};
    ShapePtr shape_ptr = &circle;
    ShapeArray shape_array = &shape_ptr;

    int add(int a, int b) { return a + b; }
    int multiply(int a, int b) { return a * b; }

    BinaryOp ops[2] = {add, multiply};
    BinaryOpPtr op_ptr = &ops[0];
    BinaryOpArray op_array = &op_ptr;

    Operation op = {"add", add};
    OperationPtr op_ptr2 = &op;

    Event event = {1, NULL, NULL, 0};
    EventSystem sys = {&event, 1, NULL};
    EventSystemPtr sys_ptr = &sys;
    "#;
        let (mut extractor, tree) = parse_c(code, "typedefs.c");
        let symbols = extractor.extract_symbols(&tree);

        // Basic typedef chains
        let integer_typedef = symbols.iter().find(|s| s.name == "Integer");
        assert!(integer_typedef.is_some());

        let number_typedef = symbols.iter().find(|s| s.name == "Number");
        assert!(number_typedef.is_some());

        // Pointer typedef chains
        let string_typedef = symbols.iter().find(|s| s.name == "String");
        assert!(string_typedef.is_some());

        let string_array_typedef = symbols.iter().find(|s| s.name == "StringArray");
        assert!(string_array_typedef.is_some());

        let string_matrix_typedef = symbols.iter().find(|s| s.name == "StringMatrix");
        assert!(string_matrix_typedef.is_some());

        // Struct typedef chains
        let point_typedef = symbols.iter().find(|s| s.name == "Point");
        assert!(point_typedef.is_some());

        let line_typedef = symbols.iter().find(|s| s.name == "Line");
        assert!(line_typedef.is_some());

        let polygon_typedef = symbols.iter().find(|s| s.name == "Polygon");
        assert!(polygon_typedef.is_some());

        let polygon_ptr_typedef = symbols.iter().find(|s| s.name == "PolygonPtr");
        assert!(polygon_ptr_typedef.is_some());

        let polygon_array_typedef = symbols.iter().find(|s| s.name == "PolygonArray");
        assert!(polygon_array_typedef.is_some());

        // Enum typedef chains
        let color_typedef = symbols.iter().find(|s| s.name == "Color");
        assert!(color_typedef.is_some());

        let shape_type_typedef = symbols.iter().find(|s| s.name == "ShapeType");
        assert!(shape_type_typedef.is_some());

        let shape_typedef = symbols.iter().find(|s| s.name == "Shape");
        assert!(shape_typedef.is_some());

        let shape_ptr_typedef = symbols.iter().find(|s| s.name == "ShapePtr");
        assert!(shape_ptr_typedef.is_some());

        let shape_array_typedef = symbols.iter().find(|s| s.name == "ShapeArray");
        assert!(shape_array_typedef.is_some());
    }
}
