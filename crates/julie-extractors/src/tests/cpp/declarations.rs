use super::{extract_symbols, SymbolKind};

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Template variable extraction tests
    // ========================================================================

    #[test]
    fn test_template_variable_constexpr() {
        let code = r#"template<class T> constexpr T pi = T(3.14159);"#;
        let symbols = extract_symbols(code);
        let pi = symbols.iter().find(|s| s.name == "pi");
        assert!(pi.is_some(), "Template variable 'pi' should be extracted");
        let pi = pi.unwrap();
        assert_eq!(pi.kind, SymbolKind::Constant);
        // Signature should include template parameters
        let sig = pi.signature.as_ref().unwrap();
        assert!(
            sig.contains("template<class T>"),
            "Signature should contain template params, got: {}",
            sig
        );
        assert!(
            sig.contains("pi"),
            "Signature should contain variable name, got: {}",
            sig
        );
    }

    #[test]
    fn test_template_variable_non_constexpr() {
        let code = r#"template<typename T> T default_value = T{};"#;
        let symbols = extract_symbols(code);
        let dv = symbols.iter().find(|s| s.name == "default_value");
        assert!(
            dv.is_some(),
            "Template variable 'default_value' should be extracted"
        );
        let dv = dv.unwrap();
        // Non-constexpr template variable should be Variable kind
        assert_eq!(dv.kind, SymbolKind::Variable);
        let sig = dv.signature.as_ref().unwrap();
        assert!(
            sig.contains("template<typename T>"),
            "Signature should contain template params, got: {}",
            sig
        );
    }

    #[test]
    fn test_template_variable_no_duplicate() {
        // Make sure we don't get duplicate symbols (one from extract_template, one from walk_children)
        let code = r#"template<class T> constexpr T pi = T(3.14159);"#;
        let symbols = extract_symbols(code);
        let pi_count = symbols.iter().filter(|s| s.name == "pi").count();
        assert_eq!(
            pi_count, 1,
            "Template variable 'pi' should appear exactly once, got {}",
            pi_count
        );
    }

    #[test]
    fn test_template_class_still_works() {
        // Ensure template classes still work after our changes
        let code = r#"
template<typename T>
class Container {
public:
    void add(T item);
};
"#;
        let symbols = extract_symbols(code);
        let container = symbols.iter().find(|s| s.name == "Container");
        assert!(
            container.is_some(),
            "Template class should still be extracted"
        );
        assert_eq!(container.unwrap().kind, SymbolKind::Class);
        let sig = container.unwrap().signature.as_ref().unwrap();
        assert!(
            sig.contains("template<typename T>"),
            "Template class should still have template params in signature"
        );
    }

    #[test]
    fn test_template_function_still_works() {
        // Ensure template functions still work after our changes
        let code = r#"
template<typename T>
T max_val(const T& a, const T& b) {
    return (a > b) ? a : b;
}
"#;
        let symbols = extract_symbols(code);
        let max_fn = symbols.iter().find(|s| s.name == "max_val");
        assert!(
            max_fn.is_some(),
            "Template function should still be extracted"
        );
        assert_eq!(max_fn.unwrap().kind, SymbolKind::Function);
    }

    // ========================================================================
    // Typedef extraction tests
    // ========================================================================

    #[test]
    fn test_typedef_simple() {
        let code = r#"typedef int ErrorCode;"#;
        let symbols = extract_symbols(code);
        let typedef = symbols.iter().find(|s| s.name == "ErrorCode");
        assert!(
            typedef.is_some(),
            "Typedef 'ErrorCode' should be extracted"
        );
        let typedef = typedef.unwrap();
        assert_eq!(typedef.kind, SymbolKind::Type);
        let sig = typedef.signature.as_ref().unwrap();
        assert!(
            sig.contains("typedef"),
            "Signature should include 'typedef', got: {}",
            sig
        );
    }

    #[test]
    fn test_typedef_function_pointer() {
        let code = r#"typedef void (*callback_t)(int, float);"#;
        let symbols = extract_symbols(code);
        let typedef = symbols.iter().find(|s| s.name == "callback_t");
        assert!(
            typedef.is_some(),
            "Function pointer typedef 'callback_t' should be extracted"
        );
        let typedef = typedef.unwrap();
        assert_eq!(typedef.kind, SymbolKind::Type);
        let sig = typedef.signature.as_ref().unwrap();
        assert!(
            sig.contains("typedef"),
            "Signature should include 'typedef', got: {}",
            sig
        );
    }

    #[test]
    fn test_typedef_pointer() {
        let code = r#"typedef int* IntPtr;"#;
        let symbols = extract_symbols(code);
        let typedef = symbols.iter().find(|s| s.name == "IntPtr");
        assert!(
            typedef.is_some(),
            "Typedef 'IntPtr' should be extracted"
        );
        assert_eq!(typedef.unwrap().kind, SymbolKind::Type);
    }

    #[test]
    fn test_typedef_in_namespace() {
        let code = r#"
namespace my_lib {
    typedef int ErrorCode;
    typedef void (*Handler)(int);
}
"#;
        let symbols = extract_symbols(code);
        let error_code = symbols.iter().find(|s| s.name == "ErrorCode");
        assert!(
            error_code.is_some(),
            "Typedef 'ErrorCode' in namespace should be extracted"
        );
        assert_eq!(error_code.unwrap().kind, SymbolKind::Type);

        let handler = symbols.iter().find(|s| s.name == "Handler");
        assert!(
            handler.is_some(),
            "Typedef 'Handler' in namespace should be extracted"
        );
        assert_eq!(handler.unwrap().kind, SymbolKind::Type);
    }

    #[test]
    fn test_typedef_struct() {
        // Common C pattern: typedef struct
        let code = r#"
typedef struct {
    int x;
    int y;
} Point;
"#;
        let symbols = extract_symbols(code);
        let point = symbols.iter().find(|s| s.name == "Point");
        assert!(
            point.is_some(),
            "Typedef struct 'Point' should be extracted"
        );
        assert_eq!(point.unwrap().kind, SymbolKind::Type);
    }
}
