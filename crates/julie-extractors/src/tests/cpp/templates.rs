use super::{RelationshipKind, SymbolKind, parse_cpp};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_template_classes_and_functions() {
        let cpp_code = r#"
    template<typename T>
    class Vector {
    public:
        Vector(size_t size);
        void push_back(const T& item);
        T& operator[](size_t index);

    private:
        T* data_;
        size_t size_;
        size_t capacity_;
    };

    template<typename T, size_t N>
    class Array {
    public:
        T& at(size_t index) { return data_[index]; }

    private:
        T data_[N];
    };

    template<typename T>
    T max(const T& a, const T& b) {
        return (a > b) ? a : b;
    }

    template<typename T, typename U>
    auto add(T a, U b) -> decltype(a + b) {
        return a + b;
    }

    // Template specialization
    template<>
    class Vector<bool> {
    public:
        void flip();
    private:
        std::vector<uint8_t> data_;
    };
    "#;

        let (mut extractor, tree) = parse_cpp(cpp_code);
        let symbols = extractor.extract_symbols(&tree);

        let vector = symbols.iter().find(|s| s.name == "Vector");
        assert!(vector.is_some());
        assert!(
            vector
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("template<typename T>")
        );
        assert!(
            vector
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("class Vector")
        );

        let array = symbols.iter().find(|s| s.name == "Array");
        assert!(array.is_some());
        assert!(
            array
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("template<typename T, size_t N>")
        );

        let max_func = symbols.iter().find(|s| s.name == "max");
        assert!(max_func.is_some());
        assert_eq!(max_func.unwrap().kind, SymbolKind::Function);
        assert!(
            max_func
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("template<typename T>")
        );

        let add_func = symbols.iter().find(|s| s.name == "add");
        assert!(add_func.is_some());
        assert!(
            add_func
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("auto add(T a, U b) -> decltype(a + b)")
        );

        let vector_bool = symbols
            .iter()
            .find(|s| s.name == "Vector" && s.signature.as_ref().unwrap().contains("<bool>"));
        assert!(vector_bool.is_some());
    }

    #[test]
    fn test_extract_inheritance_and_template_relationships() {
        let cpp_code = r#"
    class Base {
    public:
        virtual void method() {}
    };

    class Derived : public Base {
    public:
        void method() override {}
    };

    template<typename T>
    class Container : public Base {
    public:
        void add(T item) {}
    };

    class MultipleInheritance : public Base, public Container<int> {
    public:
        void complexMethod() {}
    };
    "#;

        let (mut extractor, tree) = parse_cpp(cpp_code);
        let symbols = extractor.extract_symbols(&tree);
        let relationships = extractor.extract_relationships(&tree, &symbols);

        // Check inheritance relationships
        let derived_extends_base = relationships.iter().any(|r| {
            r.kind == RelationshipKind::Extends
                && symbols
                    .iter()
                    .any(|s| s.id == r.from_symbol_id && s.name == "Derived")
                && symbols
                    .iter()
                    .any(|s| s.id == r.to_symbol_id && s.name == "Base")
        });
        assert!(derived_extends_base);

        let container_extends_base = relationships.iter().any(|r| {
            r.kind == RelationshipKind::Extends
                && symbols
                    .iter()
                    .any(|s| s.id == r.from_symbol_id && s.name == "Container")
                && symbols
                    .iter()
                    .any(|s| s.id == r.to_symbol_id && s.name == "Base")
        });
        assert!(container_extends_base);
    }

    #[test]
    fn test_extract_variadic_templates_sfinae_and_template_metaprogramming() {
        let cpp_code = r#"
    #include <type_traits>
    #include <utility>

    // Variadic templates
    template<typename... Args>
    class VariadicTemplate {
    public:
        template<typename T, typename... Rest>
        void process(T&& first, Rest&&... rest) {
            // Process first, then recursively process rest
            if constexpr (sizeof...(rest) > 0) {
                process(std::forward<Rest>(rest)...);
            }
        }
    };

    // SFINAE with enable_if
    template<typename T>
    typename std::enable_if<std::is_integral<T>::value, T>::type
    increment(T value) {
        return value + 1;
    }

    template<typename T>
    typename std::enable_if<std::is_floating_point<T>::value, T>::type
    increment(T value) {
        return value + 1.0;
    }

    // Type traits
    template<typename T>
    struct is_pointer : std::false_type {};

    template<typename T>
    struct is_pointer<T*> : std::true_type {};

    // Concepts simulation (pre-C++20)
    template<typename T>
    using HasBegin = decltype(std::declval<T>().begin());

    template<typename Container>
    class ContainerWrapper {
        static_assert(std::is_same_v<HasBegin<Container>, void>, "Container must have begin()");
    };

    // Perfect forwarding
    template<typename T>
    decltype(auto) perfect_forward(T&& value) {
        return std::forward<T>(value);
    }
    "#;

        let (mut extractor, tree) = parse_cpp(cpp_code);
        let symbols = extractor.extract_symbols(&tree);

        let variadic_template = symbols.iter().find(|s| s.name == "VariadicTemplate");
        assert!(variadic_template.is_some());
        assert!(
            variadic_template
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("typename... Args")
        );

        let process_method = symbols.iter().find(|s| s.name == "process");
        assert!(process_method.is_some());

        let increment_funcs = symbols.iter().filter(|s| s.name == "increment").count();
        assert_eq!(increment_funcs, 2); // Two overloads with SFINAE

        let is_pointer_trait = symbols.iter().find(|s| s.name == "is_pointer");
        assert!(is_pointer_trait.is_some());

        let perfect_forward = symbols.iter().find(|s| s.name == "perfect_forward");
        assert!(perfect_forward.is_some());
        assert!(
            perfect_forward
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("decltype(auto)")
        );
    }

    #[test]
    fn test_handle_large_cpp_codebases_with_complex_templates_efficiently() {
        let cpp_code = r#"
    // Complex template metaprogramming
    template<int N>
    struct Factorial {
        static constexpr int value = N * Factorial<N - 1>::value;
    };

    template<>
    struct Factorial<0> {
        static constexpr int value = 1;
    };

    template<template<typename> class Container, typename... Types>
    class MultiContainer {
    public:
        template<std::size_t Index>
        using TypeAt = typename std::tuple_element<Index, std::tuple<Types...>>::type;

        template<std::size_t Index>
        Container<TypeAt<Index>>& get() {
            return std::get<Index>(containers_);
        }

    private:
        std::tuple<Container<Types>...> containers_;
    };

    // Complex inheritance hierarchy
    template<typename Derived>
    class CRTP_Base {
    public:
        void interface() {
            static_cast<Derived*>(this)->implementation();
        }
    };

    class Implementation1 : public CRTP_Base<Implementation1> {
    public:
        void implementation() { /* impl 1 */ }
    };

    class Implementation2 : public CRTP_Base<Implementation2> {
    public:
        void implementation() { /* impl 2 */ }
    };

    // Template template parameters
    template<template<typename, typename> class Container,
             typename Key,
             typename Value,
             template<typename> class Allocator = std::allocator>
    class GenericMap {
    public:
        using ContainerType = Container<Key, Value>;
        using AllocatorType = Allocator<std::pair<const Key, Value>>;

        void insert(const Key& k, const Value& v);
        Value& operator[](const Key& k);

    private:
        ContainerType data_;
        AllocatorType alloc_;
    };

    // Variadic template with perfect forwarding
    template<typename F, typename... Args>
    auto invoke_later(F&& f, Args&&... args)
        -> std::future<std::invoke_result_t<F, Args...>> {
        using return_type = std::invoke_result_t<F, Args...>;

        auto task = std::make_shared<std::packaged_task<return_type()>>(
            std::bind(std::forward<F>(f), std::forward<Args>(args)...)
        );

        std::future<return_type> result = task->get_future();

        std::thread([task](){ (*task)(); }).detach();

        return result;
    }

    // Complex SFINAE
    template<typename T, typename = void>
    struct has_size : std::false_type {};

    template<typename T>
    struct has_size<T, std::void_t<decltype(std::declval<T>().size())>>
        : std::true_type {};

    template<typename Container>
    std::enable_if_t<has_size<Container>::value, std::size_t>
    get_size(const Container& c) {
        return c.size();
    }

    template<typename Container>
    std::enable_if_t<!has_size<Container>::value, std::size_t>
    get_size(const Container& c) {
        return std::distance(std::begin(c), std::end(c));
    }
    "#;

        let (mut extractor, tree) = parse_cpp(cpp_code);
        let symbols = extractor.extract_symbols(&tree);

        let factorial = symbols.iter().find(|s| s.name == "Factorial");
        assert!(factorial.is_some());
        assert!(
            factorial
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("template<int N>")
        );

        let multi_container = symbols.iter().find(|s| s.name == "MultiContainer");
        assert!(multi_container.is_some());
        assert!(
            multi_container
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("template<template<typename> class Container")
        );

        let crtp_base = symbols.iter().find(|s| s.name == "CRTP_Base");
        assert!(crtp_base.is_some());

        let implementation1 = symbols.iter().find(|s| s.name == "Implementation1");
        assert!(implementation1.is_some());

        let generic_map = symbols.iter().find(|s| s.name == "GenericMap");
        assert!(generic_map.is_some());

        let invoke_later = symbols.iter().find(|s| s.name == "invoke_later");
        assert!(invoke_later.is_some());

        let has_size_trait = symbols.iter().find(|s| s.name == "has_size");
        assert!(has_size_trait.is_some());

        let get_size_funcs = symbols.iter().filter(|s| s.name == "get_size").count();
        assert_eq!(get_size_funcs, 2); // Two SFINAE overloads

        // Performance check - should handle complex templates without timeout
        assert!(symbols.len() >= 10); // Should extract significant number of symbols
    }
}
