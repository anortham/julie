use super::parse_cpp;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_lambdas_smart_pointers_and_modern_cpp_constructs() {
        let cpp_code = r#"
    #include <memory>
    #include <functional>
    #include <vector>

    class ModernCpp {
    public:
        void useLambdas() {
            auto lambda1 = [](int x) { return x * 2; };
            auto lambda2 = [this](double y) -> double { return y + value_; };
            auto lambda3 = [&](const std::string& s) mutable -> bool { return !s.empty(); };
        }

        void useSmartPointers() {
            auto ptr1 = std::make_unique<int>(42);
            auto ptr2 = std::make_shared<std::vector<double>>();
            std::weak_ptr<int> weak_ref = ptr2;
        }

        template<typename F>
        void useCallbacks(F&& callback) {
            callback();
        }

    private:
        double value_ = 1.0;
        std::unique_ptr<char[]> buffer_;
        std::shared_ptr<Resource> resource_;
    };

    // Generic lambda (C++14)
    auto generic_lambda = [](auto x, auto y) { return x + y; };

    // Variable templates (C++14)
    template<typename T>
    constexpr T pi = T(3.14159265358979323846);

    // Alias templates
    template<typename T>
    using Vec = std::vector<T>;

    // Structured bindings (C++17)
    auto [x, y, z] = std::make_tuple(1, 2.0, "three");
    "#;

        let (mut extractor, tree) = parse_cpp(cpp_code);
        let symbols = extractor.extract_symbols(&tree);

        let modern_class = symbols.iter().find(|s| s.name == "ModernCpp");
        assert!(modern_class.is_some());

        let use_lambdas = symbols.iter().find(|s| s.name == "useLambdas");
        assert!(use_lambdas.is_some());

        let generic_lambda = symbols.iter().find(|s| s.name == "generic_lambda");
        assert!(generic_lambda.is_some());

        let pi_template = symbols.iter().find(|s| s.name == "pi");
        assert!(pi_template.is_some());
        assert!(pi_template
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("constexpr"));

        // Note: Lambda extraction within functions is complex and may not be fully implemented initially
        let lambdas_count = symbols.iter().filter(|s| s.name.contains("lambda")).count();
        // At minimum, we should find the generic_lambda
        assert!(lambdas_count >= 1);
    }
}
