use super::{SymbolKind, extract_symbols};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_doxygen_from_class() {
        let code = r#"
            /**
             * UserService manages user authentication
             * Provides login and logout functionality
             */
            class UserService {
            public:
                void authenticate();
            };
        "#;

        let symbols = extract_symbols(code);
        let class_symbol = symbols
            .iter()
            .find(|s| s.name == "UserService")
            .expect("Class not found");

        assert!(
            class_symbol.doc_comment.is_some(),
            "Class should have doc comment"
        );
        let doc = class_symbol.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("manages user authentication"),
            "Doc should contain main description"
        );
        assert!(
            doc.contains("Provides login and logout"),
            "Doc should contain full comment"
        );
    }

    #[test]
    fn test_extract_doxygen_from_function() {
        let code = r#"
            /**
             * Validates user credentials
             * @param username The username to validate
             * @return true if valid
             */
            bool validateCredentials(const std::string& username) {
                return true;
            }
        "#;

        let symbols = extract_symbols(code);
        let func = symbols
            .iter()
            .find(|s| s.name == "validateCredentials")
            .expect("Function not found");

        assert!(
            func.doc_comment.is_some(),
            "Function should have doc comment"
        );
        let doc = func.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("Validates user credentials"),
            "Doc should contain main description"
        );
        assert!(
            doc.contains("@param username"),
            "Doc should contain parameter info"
        );
        assert!(doc.contains("@return"), "Doc should contain return info");
    }

    #[test]
    fn test_extract_doxygen_triple_slash_from_method() {
        let code = r#"
            class Calculator {
            public:
                /// Adds two numbers together
                /// @param a First number
                /// @param b Second number
                /// @return Sum of a and b
                int add(int a, int b) {
                    return a + b;
                }
            };
        "#;

        let symbols = extract_symbols(code);
        let method = symbols
            .iter()
            .find(|s| s.name == "add")
            .expect("Method not found");

        assert!(
            method.doc_comment.is_some(),
            "Method should have doc comment"
        );
        let doc = method.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("Adds two numbers"),
            "Doc should contain main description"
        );
        assert!(
            doc.contains("@param a"),
            "Doc should contain first parameter info"
        );
        assert!(
            doc.contains("@param b"),
            "Doc should contain second parameter info"
        );
        assert!(doc.contains("@return"), "Doc should contain return info");
    }

    #[test]
    fn test_extract_doxygen_from_struct() {
        let code = r#"
            /**
             * Point represents a 2D coordinate
             * @brief Used for geometric calculations
             */
            struct Point {
                double x;
                double y;
            };
        "#;

        let symbols = extract_symbols(code);
        let struct_symbol = symbols
            .iter()
            .find(|s| s.name == "Point")
            .expect("Struct not found");

        assert!(
            struct_symbol.doc_comment.is_some(),
            "Struct should have doc comment"
        );
        let doc = struct_symbol.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("Point represents a 2D coordinate"),
            "Doc should contain description"
        );
        assert!(doc.contains("@brief"), "Doc should contain @brief tag");
    }

    #[test]
    fn test_extract_doxygen_from_enum() {
        let code = r#"
            /**
             * @brief Status codes for operations
             * Represents the result of async operations
             */
            enum class Status {
                Success,
                Error,
                Pending
            };
        "#;

        let symbols = extract_symbols(code);
        let enum_symbol = symbols
            .iter()
            .find(|s| s.name == "Status")
            .expect("Enum not found");

        assert!(
            enum_symbol.doc_comment.is_some(),
            "Enum should have doc comment"
        );
        let doc = enum_symbol.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("Status codes for operations"),
            "Doc should contain description"
        );
        assert!(doc.contains("@brief"), "Doc should contain @brief tag");
    }

    #[test]
    fn test_extract_doxygen_from_constructor() {
        let code = r#"
            class MyClass {
            public:
                /**
                 * Constructs MyClass with initialization
                 * @param value Initial value for the object
                 */
                MyClass(int value);
            };
        "#;

        let symbols = extract_symbols(code);
        let ctor = symbols
            .iter()
            .find(|s| s.name == "MyClass" && s.kind == SymbolKind::Constructor)
            .expect("Constructor not found");

        assert!(
            ctor.doc_comment.is_some(),
            "Constructor should have doc comment"
        );
        let doc = ctor.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("Constructs MyClass"),
            "Doc should contain constructor description"
        );
        assert!(
            doc.contains("@param value"),
            "Doc should contain parameter info"
        );
    }

    #[test]
    fn test_extract_doxygen_from_destructor() {
        let code = r#"
            class Resource {
            public:
                /// Cleans up resource and releases memory
                ~Resource();
            };
        "#;

        let symbols = extract_symbols(code);
        let dtor = symbols
            .iter()
            .find(|s| s.name == "~Resource" && s.kind == SymbolKind::Destructor)
            .expect("Destructor not found");

        assert!(
            dtor.doc_comment.is_some(),
            "Destructor should have doc comment"
        );
        let doc = dtor.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("Cleans up resource"),
            "Doc should contain destructor description"
        );
    }

    #[test]
    fn test_extract_doxygen_with_template_parameters() {
        let code = r#"
            /**
             * Template function for computing maximum value
             * @tparam T Type of elements to compare
             * @param a First value
             * @param b Second value
             * @return The maximum of a and b
             */
            template<typename T>
            T max(T a, T b) {
                return (a > b) ? a : b;
            }
        "#;

        let symbols = extract_symbols(code);
        let func = symbols
            .iter()
            .find(|s| s.name == "max")
            .expect("Template function not found");

        assert!(
            func.doc_comment.is_some(),
            "Template function should have doc comment"
        );
        let doc = func.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("Template function for computing maximum"),
            "Doc should contain description"
        );
        assert!(
            doc.contains("@tparam T"),
            "Doc should contain template parameter info"
        );
        assert!(
            doc.contains("@param a"),
            "Doc should contain parameter info"
        );
    }

    #[test]
    fn test_extract_doxygen_from_namespace() {
        let code = r#"
            /**
             * @brief Utilities namespace
             * Contains common utility functions and classes
             */
            namespace Utils {
                class Helper {};
            }
        "#;

        let symbols = extract_symbols(code);
        let namespace = symbols
            .iter()
            .find(|s| s.name == "Utils")
            .expect("Namespace not found");

        assert!(
            namespace.doc_comment.is_some(),
            "Namespace should have doc comment"
        );
        let doc = namespace.doc_comment.as_ref().unwrap();
        assert!(doc.contains("@brief"), "Doc should contain @brief tag");
        assert!(
            doc.contains("Contains common utility"),
            "Doc should contain description"
        );
    }

    #[test]
    fn test_extract_doxygen_from_union() {
        let code = r#"
            /**
             * Data union that holds either an integer or a float
             * @brief Flexible data storage
             */
            union Data {
                int i;
                float f;
            };
        "#;

        let symbols = extract_symbols(code);
        let union_symbol = symbols
            .iter()
            .find(|s| s.name == "Data" && s.kind == SymbolKind::Union)
            .expect("Union not found");

        assert!(
            union_symbol.doc_comment.is_some(),
            "Union should have doc comment"
        );
        let doc = union_symbol.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("holds either an integer or a float"),
            "Doc should contain description"
        );
        assert!(doc.contains("@brief"), "Doc should contain @brief tag");
    }

    #[test]
    fn test_extract_multiple_doc_comment_lines() {
        let code = r#"
            /**
             * Processes data with comprehensive validation
             *
             * This function validates input before processing
             * and returns appropriate error codes.
             *
             * @param input Input data pointer
             * @param output Output buffer pointer
             * @return Status code indicating success or failure
             */
            Status processData(const Data* input, Data* output);
        "#;

        let symbols = extract_symbols(code);
        let func = symbols
            .iter()
            .find(|s| s.name == "processData")
            .expect("Function not found");

        assert!(
            func.doc_comment.is_some(),
            "Function should have doc comment"
        );
        let doc = func.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("Processes data with comprehensive validation"),
            "Doc should contain first line"
        );
        assert!(
            doc.contains("This function validates input"),
            "Doc should contain detailed description"
        );
        assert!(
            doc.contains("@param input"),
            "Doc should contain parameter documentation"
        );
    }

    #[test]
    fn test_no_doc_comment_when_missing() {
        let code = r#"
            int simpleFunction() {
                return 42;
            }
        "#;

        let symbols = extract_symbols(code);
        let func = symbols
            .iter()
            .find(|s| s.name == "simpleFunction")
            .expect("Function not found");

        assert!(
            func.doc_comment.is_none(),
            "Function without doc comment should have None"
        );
    }

    #[test]
    fn test_extract_doxygen_from_operator_overload() {
        let code = r#"
            class Vector {
            public:
                /**
                 * Adds two vectors together
                 * @param other The vector to add
                 * @return New vector containing the sum
                 */
                Vector operator+(const Vector& other) const;
            };
        "#;

        let symbols = extract_symbols(code);
        let op = symbols
            .iter()
            .find(|s| s.name == "operator+" && s.kind == SymbolKind::Operator)
            .expect("Operator overload not found");

        assert!(op.doc_comment.is_some(), "Operator should have doc comment");
        let doc = op.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("Adds two vectors"),
            "Doc should contain operator description"
        );
        assert!(
            doc.contains("@param other"),
            "Doc should contain parameter info"
        );
    }
}
