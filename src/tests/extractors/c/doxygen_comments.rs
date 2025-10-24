use super::extract_symbols;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_doxygen_from_function_definition() {
        let code = r#"
            /**
             * Validates user credentials
             * @param username The username to validate
             * @return 1 if valid, 0 otherwise
             */
            int validate_credentials(const char* username) {
                return 1;
            }
        "#;

        let symbols = extract_symbols(code);
        let func = symbols
            .iter()
            .find(|s| s.name == "validate_credentials")
            .expect("Function not found");

        assert!(
            func.doc_comment.is_some(),
            "Function should have doc comment"
        );
        let doc = func.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("Validates user credentials"),
            "Doc should contain description"
        );
        assert!(
            doc.contains("@param username"),
            "Doc should contain parameter info"
        );
        assert!(doc.contains("@return"), "Doc should contain return info");
    }

    #[test]
    fn test_extract_doxygen_from_function_declaration() {
        let code = r#"
            /**
             * Calculates the sum of two numbers
             * @param a First number
             * @param b Second number
             * @return The sum of a and b
             */
            int add(int a, int b);
        "#;

        let symbols = extract_symbols(code);
        let func = symbols
            .iter()
            .find(|s| s.name == "add")
            .expect("Function not found");

        assert!(
            func.doc_comment.is_some(),
            "Function should have doc comment"
        );
        let doc = func.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("Calculates the sum"),
            "Doc should contain description"
        );
        assert!(
            doc.contains("@param a"),
            "Doc should contain first parameter info"
        );
        assert!(
            doc.contains("@param b"),
            "Doc should contain second parameter info"
        );
    }

    #[test]
    fn test_extract_doxygen_triple_slash_from_struct() {
        let code = r#"
            /// User service structure
            /// Manages authentication state
            struct UserService {
                int is_authenticated;
            };
        "#;

        let symbols = extract_symbols(code);
        let struct_symbol = symbols
            .iter()
            .find(|s| s.name == "UserService")
            .expect("Struct not found");

        assert!(
            struct_symbol.doc_comment.is_some(),
            "Struct should have doc comment"
        );
        let doc = struct_symbol.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("User service structure"),
            "Doc should contain description"
        );
        assert!(
            doc.contains("Manages authentication state"),
            "Doc should contain full comment"
        );
    }

    #[test]
    fn test_extract_doxygen_from_typedef() {
        let code = r#"
            /**
             * Error code type
             * @brief Standard error codes for the system
             */
            typedef int ErrorCode;
        "#;

        let symbols = extract_symbols(code);
        let typedef = symbols
            .iter()
            .find(|s| s.name == "ErrorCode")
            .expect("Typedef not found");

        assert!(
            typedef.doc_comment.is_some(),
            "Typedef should have doc comment"
        );
        let doc = typedef.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("Error code type"),
            "Doc should contain description"
        );
        assert!(
            doc.contains("@brief"),
            "Doc should contain brief annotation"
        );
    }

    #[test]
    fn test_extract_doxygen_from_enum() {
        let code = r#"
            /**
             * @brief Status codes for operations
             * Represents the result of operations
             */
            enum Status {
                STATUS_SUCCESS,
                STATUS_ERROR,
                STATUS_PENDING
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
            doc.contains("@brief"),
            "Doc should contain brief annotation"
        );
        assert!(
            doc.contains("Status codes for operations"),
            "Doc should contain description"
        );
    }

    #[test]
    fn test_extract_doxygen_from_global_variable() {
        let code = r#"
            /**
             * Global counter for operations
             * @see increment_counter()
             */
            int global_counter = 0;
        "#;

        let symbols = extract_symbols(code);
        let var = symbols
            .iter()
            .find(|s| s.name == "global_counter")
            .expect("Variable not found");

        assert!(
            var.doc_comment.is_some(),
            "Variable should have doc comment"
        );
        let doc = var.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("Global counter"),
            "Doc should contain description"
        );
        assert!(doc.contains("@see"), "Doc should contain see reference");
    }

    #[test]
    fn test_extract_multiple_doc_comment_lines() {
        let code = r#"
            /**
             * Processes data with validation
             *
             * This function validates input before processing
             * and returns appropriate error codes.
             *
             * @param input Input data pointer
             * @param output Output buffer pointer
             * @return ErrorCode indicating success or failure
             */
            int process_data(const char* input, char* output);
        "#;

        let symbols = extract_symbols(code);
        let func = symbols
            .iter()
            .find(|s| s.name == "process_data")
            .expect("Function not found");

        assert!(
            func.doc_comment.is_some(),
            "Function should have doc comment"
        );
        let doc = func.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("Processes data with validation"),
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
            int simple_function() {
                return 42;
            }
        "#;

        let symbols = extract_symbols(code);
        let func = symbols
            .iter()
            .find(|s| s.name == "simple_function")
            .expect("Function not found");

        assert!(
            func.doc_comment.is_none(),
            "Function without doc comment should have None"
        );
    }

    #[test]
    fn test_extract_doxygen_from_struct_with_fields() {
        let code = r#"
            /**
             * @brief Point in 2D space
             */
            struct Point {
                /// X coordinate
                double x;
                /// Y coordinate
                double y;
            };
        "#;

        let symbols = extract_symbols(code);
        let point_struct = symbols
            .iter()
            .find(|s| s.name == "Point")
            .expect("Point struct not found");

        assert!(
            point_struct.doc_comment.is_some(),
            "Struct should have doc comment"
        );
        let doc = point_struct.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("@brief"),
            "Doc should contain brief annotation"
        );
        assert!(
            doc.contains("Point in 2D space"),
            "Doc should contain description"
        );
    }
}
