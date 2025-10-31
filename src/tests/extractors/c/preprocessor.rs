use super::{parse_c, SymbolKind};

#[cfg(test)]
mod tests {
    use super::*;

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
        let (mut extractor, tree) = parse_c(code, "preprocessor.c");
        let symbols = extractor.extract_symbols(&tree);

        // Compiler detection macros - Tests compiler feature detection
        let compiler_gcc = symbols.iter().find(|s| s.name == "COMPILER_GCC");
        assert!(compiler_gcc.is_some());
        assert_eq!(compiler_gcc.unwrap().kind, SymbolKind::Constant);

        let likely_macro = symbols.iter().find(|s| s.name == "LIKELY");
        assert!(likely_macro.is_some());
        assert!(likely_macro
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("__builtin_expect"));

        let force_inline_macro = symbols.iter().find(|s| s.name == "FORCE_INLINE");
        assert!(force_inline_macro.is_some());

        let packed_macro = symbols.iter().find(|s| s.name == "PACKED");
        assert!(packed_macro.is_some());

        // Platform detection - Tests platform-specific macros
        let platform_windows = symbols.iter().find(|s| s.name == "PLATFORM_WINDOWS");
        assert!(platform_windows.is_some());

        let file_handle_typedef = symbols.iter().find(|s| s.name == "FileHandle");
        assert!(file_handle_typedef.is_some());

        let invalid_file_handle = symbols.iter().find(|s| s.name == "INVALID_FILE_HANDLE");
        assert!(invalid_file_handle.is_some());

        // Architecture detection - Tests arch-specific features
        let arch_x64 = symbols.iter().find(|s| s.name == "ARCH_X64");
        assert!(arch_x64.is_some());

        let cache_line_size = symbols.iter().find(|s| s.name == "CACHE_LINE_SIZE");
        assert!(cache_line_size.is_some());

        // Debug/Release configuration - Tests build configuration
        let build_debug = symbols.iter().find(|s| s.name == "BUILD_DEBUG");
        assert!(build_debug.is_some());

        let debug_print = symbols.iter().find(|s| s.name == "DEBUG_PRINT");
        assert!(debug_print.is_some());

        let assert_macro = symbols.iter().find(|s| s.name == "ASSERT");
        assert!(assert_macro.is_some());

        let debug_only = symbols.iter().find(|s| s.name == "DEBUG_ONLY");
        assert!(debug_only.is_some());

        // Version information - Tests version macros
        let version_major = symbols.iter().find(|s| s.name == "VERSION_MAJOR");
        assert!(version_major.is_some());

        let version_string = symbols.iter().find(|s| s.name == "VERSION_STRING");
        assert!(version_string.is_some());

        // Feature toggles - Tests conditional features
        let feature_networking = symbols.iter().find(|s| s.name == "FEATURE_NETWORKING");
        assert!(feature_networking.is_some());

        let feature_graphics = symbols.iter().find(|s| s.name == "FEATURE_GRAPHICS");
        assert!(feature_graphics.is_some());

        // Conditional structs - Tests conditional compilation
        let network_connection = symbols.iter().find(|s| s.name == "NetworkConnection");
        assert!(network_connection.is_some());

        let audio_buffer = symbols.iter().find(|s| s.name == "AudioBuffer");
        assert!(audio_buffer.is_some());

        // Conditional functions - Tests conditional function definitions
        let network_init = symbols.iter().find(|s| s.name == "network_init");
        assert!(network_init.is_some());

        let audio_init = symbols.iter().find(|s| s.name == "audio_init");
        assert!(audio_init.is_some());

        // Pragma-affected structures - Tests pragma handling
        let network_header = symbols.iter().find(|s| s.name == "NetworkHeader");
        assert!(network_header.is_some());
        assert!(network_header
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("PACKED"));

        // Alignment directives - Tests alignment features
        let align_macro = symbols.iter().find(|s| s.name == "ALIGN");
        assert!(align_macro.is_some());

        let atomic_counter = symbols.iter().find(|s| s.name == "AtomicCounter");
        assert!(atomic_counter.is_some());

        assert!(atomic_counter
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("ALIGN(CACHE_LINE_SIZE)"));

        // Inline assembly functions - Tests inline assembly
        let rdtsc_function = symbols.iter().find(|s| s.name == "rdtsc");
        assert!(rdtsc_function.is_some());
        assert!(rdtsc_function
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("static inline uint64_t rdtsc(void)"));

        let cpu_pause_function = symbols.iter().find(|s| s.name == "cpu_pause");
        assert!(cpu_pause_function.is_some());

        // Advanced macro patterns - Tests sophisticated macro techniques
        let get_macro = symbols.iter().find(|s| s.name == "GET_MACRO");
        assert!(get_macro.is_some());

        let variadic_macro = symbols.iter().find(|s| s.name == "VARIADIC_MACRO");
        assert!(variadic_macro.is_some());

        let macro1 = symbols.iter().find(|s| s.name == "MACRO1");
        assert!(macro1.is_some());

        // X-Macro pattern - Tests X-macro techniques
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

        // Generated getter/setter - Tests macro generation
        let declare_getter_setter = symbols.iter().find(|s| s.name == "DECLARE_GETTER_SETTER");
        assert!(declare_getter_setter.is_some());

        let global_value = symbols.iter().find(|s| s.name == "global_value");
        assert!(global_value.is_some());

        // C11 specific features - Tests C standard features
        let atomic_int = symbols.iter().find(|s| s.name == "AtomicInt");
        assert!(atomic_int.is_some());

        let atomic_increment = symbols.iter().find(|s| s.name == "atomic_increment");
        assert!(atomic_increment.is_some());

        // Compiler optimization attributes - Tests optimization directives
        let optimize_size = symbols.iter().find(|s| s.name == "OPTIMIZE_SIZE");
        assert!(optimize_size.is_some());

        let optimize_speed = symbols.iter().find(|s| s.name == "OPTIMIZE_SPEED");
        assert!(optimize_speed.is_some());

        let no_optimize = symbols.iter().find(|s| s.name == "NO_OPTIMIZE");
        assert!(no_optimize.is_some());

        // Optimization-specific functions - Tests optimization annotations
        let fast_function = symbols.iter().find(|s| s.name == "fast_function");
        assert!(fast_function.is_some());

        let small_function = symbols.iter().find(|s| s.name == "small_function");
        assert!(small_function.is_some());

        let debug_function = symbols.iter().find(|s| s.name == "debug_function");
        assert!(debug_function.is_some());

        // Standard library includes for conditional features - Tests conditional includes
        let socket_include = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains("#include <sys/socket.h>"))
        });
        assert!(socket_include.is_some());

        let stdatomic_include = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains("#include <stdatomic.h>"))
        });
        assert!(stdatomic_include.is_some());

        let threads_include = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains("#include <threads.h>"))
        });
        assert!(threads_include.is_some());
    }
}
