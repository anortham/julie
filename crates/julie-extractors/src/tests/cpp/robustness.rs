use super::parse_cpp;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_malformed_code_complex_nesting_and_preprocessor_directives() {
        let cpp_code = r#"
    #define MACRO_FUNCTION(name, type) \
        type get_##name() const { return name##_; } \
        void set_##name(const type& value) { name##_ = value; }

    #ifdef DEBUG
        #define LOG(msg) std::cout << msg << std::endl
    #else
        #define LOG(msg)
    #endif

    class ComplexClass {
    public:
        MACRO_FUNCTION(value, int)
        MACRO_FUNCTION(name, std::string)

        #if defined(FEATURE_A) && defined(FEATURE_B)
        void feature_ab_method() {
            LOG("Feature A+B enabled");
        }
        #endif

        // Complex template with ERROR nodes in some parsers
        template<typename T, typename = std::enable_if_t<std::is_arithmetic_v<T>>>
        class NestedTemplate {
        public:
            template<typename U = T, typename = std::enable_if_t<std::is_integral_v<U>>>
            void integral_method() {}

            template<typename U = T, typename = std::enable_if_t<std::is_floating_point_v<U>>>
            void floating_method() {}
        };

        // Malformed syntax that might cause parsing issues
        #ifdef BROKEN_FEATURE
        template<typename... Args
        class BrokenTemplate {
            // Missing closing > bracket intentionally
        #endif

    private:
        int value_;
        std::string name_;
    };

    // Preprocessor conditionals with complex nesting
    #ifdef PLATFORM_WINDOWS
        #ifdef COMPILER_MSVC
            class WindowsMSVCSpecific {
            public:
                void platform_method() {}
            };
        #elif defined(COMPILER_CLANG)
            class WindowsClangSpecific {
            public:
                void platform_method() {}
            };
        #endif
    #elif defined(PLATFORM_LINUX)
        class LinuxSpecific {
        public:
            void platform_method() {}
        };
    #else
        class DefaultPlatform {
        public:
            void platform_method() {}
        };
    #endif

    // Function with complex template constraints (may parse as ERROR)
    template<typename Container>
    requires requires(Container c) {
        { c.begin() } -> std::same_as<typename Container::iterator>;
        { c.end() } -> std::same_as<typename Container::iterator>;
    }
    void process_container(Container&& c) {
        // C++20 concepts may not be fully supported by tree-sitter-cpp
    }

    // Attribute specifiers
    [[nodiscard]] int compute_value();
    [[deprecated("Use compute_value_v2 instead")]] int compute_value_old();

    class [[final]] FinalClass {
    public:
        [[noreturn]] void terminate_program();
        void regular_method() [[cold]];
    };
    "#;

        let (mut extractor, tree) = parse_cpp(cpp_code);
        let symbols = extractor.extract_symbols(&tree);

        let complex_class = symbols.iter().find(|s| s.name == "ComplexClass");
        assert!(complex_class.is_some());

        let nested_template = symbols.iter().find(|s| s.name == "NestedTemplate");
        assert!(nested_template.is_some());

        // Should handle malformed syntax gracefully
        // The extractor should not crash and should extract what it can

        // Check for platform-specific classes (at least one should be found)
        let platform_classes = symbols
            .iter()
            .filter(|s| {
                s.name.contains("Windows") || s.name.contains("Linux") || s.name.contains("Default")
            })
            .count();
        assert!(platform_classes >= 1);

        let final_class = symbols.iter().find(|s| s.name == "FinalClass");
        assert!(final_class.is_some());

        let compute_value = symbols.iter().find(|s| s.name == "compute_value");
        assert!(compute_value.is_some());

        // Should handle attributes gracefully
        let terminate_program = symbols.iter().find(|s| s.name == "terminate_program");
        assert!(terminate_program.is_some());

        // The extractor should be resilient and not crash on malformed input
        assert!(symbols.len() >= 5); // Should extract at least some symbols despite malformed parts
    }
}
