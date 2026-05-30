use super::{SymbolKind, parse_cpp};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_functions_and_operator_overloads() {
        let cpp_code = r#"
    int factorial(int n);

    inline double square(double x) {
        return x * x;
    }

    class Complex {
    public:
        Complex(double real = 0, double imag = 0);

        // Arithmetic operators
        Complex operator+(const Complex& other) const;
        Complex operator-(const Complex& other) const;
        Complex& operator+=(const Complex& other);

        // Comparison operators
        bool operator==(const Complex& other) const;
        bool operator!=(const Complex& other) const;

        // Stream operators
        friend std::ostream& operator<<(std::ostream& os, const Complex& c);
        friend std::istream& operator>>(std::istream& is, Complex& c);

        // Conversion operators
        operator double() const;
        explicit operator bool() const;

        // Function call operator
        double operator()(double x) const;

        // Subscript operator
        double& operator[](int index);

    private:
        double real_, imag_;
    };

    // Global operators
    Complex operator*(const Complex& a, const Complex& b);
    Complex operator/(const Complex& a, const Complex& b);
    "#;

        let (mut extractor, tree) = parse_cpp(cpp_code);
        let symbols = extractor.extract_symbols(&tree);

        let factorial = symbols.iter().find(|s| s.name == "factorial");
        assert!(factorial.is_some());
        assert_eq!(factorial.unwrap().kind, SymbolKind::Function);

        let square = symbols.iter().find(|s| s.name == "square");
        assert!(square.is_some());
        assert!(
            square
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("inline")
        );

        let plus_op = symbols.iter().find(|s| s.name == "operator+");
        assert!(plus_op.is_some());
        assert_eq!(plus_op.unwrap().kind, SymbolKind::Operator);
        assert!(
            plus_op
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("operator+")
        );

        let stream_op = symbols.iter().find(|s| s.name == "operator<<");
        assert!(stream_op.is_some());
        assert!(
            stream_op
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("friend")
        );

        let conversion_op = symbols.iter().find(|s| s.name == "operator double");
        assert!(conversion_op.is_some());

        let call_op = symbols.iter().find(|s| s.name == "operator()");
        assert!(call_op.is_some());
    }

    #[test]
    fn test_pointer_and_reference_return_free_functions_extracted() {
        // Regression (Phase 3b adjacent defect): a free function whose return
        // type makes the declarator a `pointer_declarator` / `reference_declarator`
        // wrapper (`const char *load()`, `char **grid()`, `int& ref()`) was never
        // extracted as a symbol because extract_function only unwrapped a direct
        // `function_declarator`. The C extractor handles this; C++ must too.
        let cpp_code = r#"
    const char *load() {
        return "x";
    }

    char **grid() {
        return 0;
    }

    int& ref(int x) {
        return x;
    }
    "#;

        let (mut extractor, tree) = parse_cpp(cpp_code);
        let symbols = extractor.extract_symbols(&tree);

        // EXACTLY ONE `load` symbol: before the fix the function_definition path
        // returned None and the bare `function_declarator` fallback produced a
        // single declarator-only symbol; a naive fix that only patches
        // extract_function would then double-extract (definition + declarator).
        let loads: Vec<_> = symbols.iter().filter(|s| s.name == "load").collect();
        assert_eq!(
            loads.len(),
            1,
            "exactly one `load` symbol (no duplicate from the declarator fallback), got {loads:?}"
        );
        let load = loads[0];
        assert_eq!(
            load.kind,
            SymbolKind::Function,
            "pointer-return free function is a Function"
        );
        // The symbol must span the FULL definition (including the `{ ... }` body),
        // not just the `load()` declarator — otherwise a literal inside the body
        // cannot anchor to it (the original Phase 3b breakage). The body lives on
        // lines after the signature, so end_line must be strictly greater.
        assert!(
            load.end_line > load.start_line,
            "symbol must span the full definition incl. body (start {}, end {})",
            load.start_line,
            load.end_line
        );

        let grid = symbols
            .iter()
            .find(|s| s.name == "grid")
            .expect("double-pointer-return free function `grid` must be extracted");
        assert_eq!(grid.kind, SymbolKind::Function);
        assert!(
            grid.end_line > grid.start_line,
            "grid must span its full definition body"
        );

        let r#ref = symbols
            .iter()
            .find(|s| s.name == "ref")
            .expect("reference-return free function `ref` must be extracted");
        assert_eq!(r#ref.kind, SymbolKind::Function);
        assert!(
            r#ref.end_line > r#ref.start_line,
            "ref must span its full definition body"
        );
    }

    #[test]
    fn test_cpp_standard_attributes_persist_and_expand() {
        let cpp_code = r#"
    [[nodiscard, maybe_unused]]
    int compute_value() {
        return 42;
    }
    "#;

        let (mut extractor, tree) = parse_cpp(cpp_code);
        let symbols = extractor.extract_symbols(&tree);

        let function = symbols
            .iter()
            .find(|s| s.name == "compute_value")
            .expect("Should find attributed function");

        let annotation_keys = function
            .annotations
            .iter()
            .map(|annotation| annotation.annotation_key.as_str())
            .collect::<Vec<_>>();
        assert_eq!(annotation_keys, vec!["nodiscard", "maybe_unused"]);

        let raw_texts = function
            .annotations
            .iter()
            .map(|annotation| annotation.raw_text.as_deref())
            .collect::<Vec<_>>();
        assert_eq!(raw_texts, vec![Some("nodiscard"), Some("maybe_unused")]);
    }
}
