use super::{parse_cpp, SymbolKind};

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
        assert!(square
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("inline"));

        let plus_op = symbols.iter().find(|s| s.name == "operator+");
        assert!(plus_op.is_some());
        assert_eq!(plus_op.unwrap().kind, SymbolKind::Operator);
        assert!(plus_op
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("operator+"));

        let stream_op = symbols.iter().find(|s| s.name == "operator<<");
        assert!(stream_op.is_some());
        assert!(stream_op
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("friend"));

        let conversion_op = symbols.iter().find(|s| s.name == "operator double");
        assert!(conversion_op.is_some());

        let call_op = symbols.iter().find(|s| s.name == "operator()");
        assert!(call_op.is_some());
    }
}
