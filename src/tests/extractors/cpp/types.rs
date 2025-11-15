use super::{SymbolKind, parse_cpp};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_enums_and_scoped_enums() {
        let cpp_code = r#"
    enum Color {
        RED,
        GREEN,
        BLUE,
        ALPHA = 255
    };

    enum class Status : uint8_t {
        Pending = 1,
        Active = 2,
        Inactive = 3,
        Error = 0xFF
    };

    enum Direction { NORTH, SOUTH, EAST, WEST };

    // Anonymous enum
    enum {
        MAX_BUFFER_SIZE = 1024,
        DEFAULT_TIMEOUT = 30
    };
    "#;

        let (mut extractor, tree) = parse_cpp(cpp_code);
        let symbols = extractor.extract_symbols(&tree);

        let color = symbols.iter().find(|s| s.name == "Color");
        assert!(color.is_some());
        assert_eq!(color.unwrap().kind, SymbolKind::Enum);
        assert!(
            color
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("enum Color")
        );

        let status = symbols.iter().find(|s| s.name == "Status");
        assert!(status.is_some());
        assert!(
            status
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("enum class Status : uint8_t")
        );

        let red = symbols.iter().find(|s| s.name == "RED");
        assert!(red.is_some());
        assert_eq!(red.unwrap().kind, SymbolKind::EnumMember);

        let alpha = symbols.iter().find(|s| s.name == "ALPHA");
        assert!(alpha.is_some());
        assert!(alpha.unwrap().signature.as_ref().unwrap().contains("= 255"));

        let max_buffer_size = symbols.iter().find(|s| s.name == "MAX_BUFFER_SIZE");
        assert!(max_buffer_size.is_some());
        assert_eq!(max_buffer_size.unwrap().kind, SymbolKind::Constant);
    }

    #[test]
    fn test_extract_variables_and_constants_with_various_storage_classes() {
        let cpp_code = r#"
    // Global variables
    int global_counter = 0;
    const double PI = 3.14159;
    constexpr int MAX_SIZE = 1000;

    // Static variables
    static std::string app_name = "MyApp";
    static const int VERSION = 1;

    // External declarations
    extern int external_var;
    extern "C" void c_function();

    class Config {
    public:
        static const int DEFAULT_PORT = 8080;
        static constexpr double TIMEOUT = 30.0;
        mutable int cache_hits;

    private:
        std::string filename_;
        static inline int instance_count_ = 0;
        thread_local static int thread_id_;
    };

    namespace Settings {
        inline constexpr bool DEBUG_MODE = true;
        volatile sig_atomic_t signal_flag = 0;
    }
    "#;

        let (mut extractor, tree) = parse_cpp(cpp_code);
        let symbols = extractor.extract_symbols(&tree);

        let global_counter = symbols.iter().find(|s| s.name == "global_counter");
        assert!(global_counter.is_some());
        assert_eq!(global_counter.unwrap().kind, SymbolKind::Variable);

        let pi = symbols.iter().find(|s| s.name == "PI");
        assert!(pi.is_some());
        assert_eq!(pi.unwrap().kind, SymbolKind::Constant);
        assert!(pi.unwrap().signature.as_ref().unwrap().contains("const"));

        let max_size = symbols.iter().find(|s| s.name == "MAX_SIZE");
        assert!(max_size.is_some());
        assert_eq!(max_size.unwrap().kind, SymbolKind::Constant);
        assert!(
            max_size
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("constexpr")
        );

        let app_name = symbols.iter().find(|s| s.name == "app_name");
        assert!(app_name.is_some());
        assert!(
            app_name
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("static")
        );

        let default_port = symbols.iter().find(|s| s.name == "DEFAULT_PORT");
        assert!(default_port.is_some());
        assert_eq!(default_port.unwrap().kind, SymbolKind::Constant);

        let debug_mode = symbols.iter().find(|s| s.name == "DEBUG_MODE");
        assert!(debug_mode.is_some());
        assert!(
            debug_mode
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("inline constexpr")
        );
    }

    #[test]
    fn test_infer_types_from_cpp_type_annotations_and_auto() {
        let cpp_code = r#"
    auto getValue() -> int { return 42; }
    auto getConstant() { return 3.14; }

    template<typename T>
    auto process(T&& value) -> decltype(std::forward<T>(value)) {
        return std::forward<T>(value);
    }

    class TypeInference {
    public:
        auto method1() const -> std::string { return name_; }
        auto method2() -> double& { return value_; }

    private:
        std::string name_;
        double value_;
    };
    "#;

        let (mut extractor, tree) = parse_cpp(cpp_code);
        let symbols = extractor.extract_symbols(&tree);
        let types = extractor.infer_types(&symbols);

        let get_value = symbols.iter().find(|s| s.name == "getValue");
        assert!(get_value.is_some());

        assert!(
            get_value
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("-> int")
        );

        let process_func = symbols.iter().find(|s| s.name == "process");
        assert!(process_func.is_some());
        assert!(
            process_func
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("auto process")
        );

        // Type inference should work
        assert!(!types.is_empty());
    }
}
