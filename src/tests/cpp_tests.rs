// Port of Miller's comprehensive C++ extractor tests
// Following TDD pattern: RED phase - tests should compile but fail

use crate::extractors::base::{SymbolKind, RelationshipKind};
use tree_sitter::Tree;

#[cfg(test)]
mod cpp_extractor_tests {
    use super::*;

    fn debug_tree_node(node: tree_sitter::Node, depth: usize) {
        let indent = "  ".repeat(depth);
        println!("{}Node: {} [{}..{}]",
            indent,
            node.kind(),
            node.start_position().row,
            node.end_position().row
        );

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            debug_tree_node(child, depth + 1);
        }
    }

    // Helper function to create a CppExtractor and parse C++ code
    fn create_extractor_and_parse(code: &str) -> (crate::extractors::cpp::CppExtractor, Tree) {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_cpp::LANGUAGE.into()).unwrap();
        let tree = parser.parse(code, None).unwrap();
        let extractor = crate::extractors::cpp::CppExtractor::new("test.cpp".to_string(), code.to_string());
        (extractor, tree)
    }


    #[test]
    fn test_extract_namespace_declarations_and_include_statements() {
        let cpp_code = r#"
#include <iostream>
#include <vector>
#include "custom_header.h"

using namespace std;
using std::string;

namespace MyCompany {
    namespace Utils {
        // Nested namespace content
    }
}

namespace MyProject = MyCompany::Utils;  // Namespace alias
"#;

        let (mut extractor, tree) = create_extractor_and_parse(cpp_code);
        let symbols = extractor.extract_symbols(&tree);

        let std_namespace = symbols.iter().find(|s| s.name == "std");
        assert!(std_namespace.is_some());
        assert_eq!(std_namespace.unwrap().kind, SymbolKind::Import);

        let my_company = symbols.iter().find(|s| s.name == "MyCompany");
        assert!(my_company.is_some());
        assert_eq!(my_company.unwrap().kind, SymbolKind::Namespace);

        let utils = symbols.iter().find(|s| s.name == "Utils");
        assert!(utils.is_some());
        assert_eq!(utils.unwrap().kind, SymbolKind::Namespace);

        let alias = symbols.iter().find(|s| s.name == "MyProject");
        assert!(alias.is_some());
        assert!(alias.unwrap().signature.as_ref().unwrap().contains("MyCompany::Utils"));
    }


    #[test]
    fn test_extract_class_declarations_with_inheritance_and_access_specifiers() {
        let cpp_code = r#"
namespace Geometry {
    class Shape {
    public:
        virtual ~Shape() = default;
        virtual double area() const = 0;
        virtual void draw() const;

    protected:
        std::string name_;

    private:
        int id_;
    };

    class Circle : public Shape {
    public:
        Circle(double radius);
        double area() const override;

        static int getInstanceCount();

    private:
        double radius_;
        static int instance_count_;
    };

    class Rectangle : public Shape {
    public:
        Rectangle(double width, double height) : width_(width), height_(height) {}
        double area() const override { return width_ * height_; }

    private:
        double width_, height_;
    };

    // Multiple inheritance
    class Drawable {
    public:
        virtual void render() = 0;
    };

    class ColoredCircle : public Circle, public Drawable {
    public:
        ColoredCircle(double radius, const std::string& color);
        void render() override;

    private:
        std::string color_;
    };
}
"#;

        let (mut extractor, tree) = create_extractor_and_parse(cpp_code);
        let symbols = extractor.extract_symbols(&tree);

        let shape = symbols.iter().find(|s| s.name == "Shape");
        assert!(shape.is_some());
        assert_eq!(shape.unwrap().kind, SymbolKind::Class);
        assert!(shape.unwrap().signature.as_ref().unwrap().contains("class Shape"));

        let circle = symbols.iter().find(|s| s.name == "Circle");
        assert!(circle.is_some());
        assert!(circle.unwrap().signature.as_ref().unwrap().contains("public Shape"));

        let colored_circle = symbols.iter().find(|s| s.name == "ColoredCircle");
        assert!(colored_circle.is_some());
        assert!(colored_circle.unwrap().signature.as_ref().unwrap().contains("public Circle, public Drawable"));

        // Check methods
        let destructor = symbols.iter().find(|s| s.name == "~Shape");
        assert!(destructor.is_some());
        assert_eq!(destructor.unwrap().kind, SymbolKind::Destructor);

        let area = symbols.iter().find(|s| s.name == "area");
        assert!(area.is_some());
        assert_eq!(area.unwrap().kind, SymbolKind::Method);
        assert!(area.unwrap().signature.as_ref().unwrap().contains("virtual"));

        let get_instance_count = symbols.iter().find(|s| s.name == "getInstanceCount");
        assert!(get_instance_count.is_some());
        assert!(get_instance_count.unwrap().signature.as_ref().unwrap().contains("static"));
    }

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

        let (mut extractor, tree) = create_extractor_and_parse(cpp_code);
        let symbols = extractor.extract_symbols(&tree);

        let vector = symbols.iter().find(|s| s.name == "Vector");
        assert!(vector.is_some());
        assert!(vector.unwrap().signature.as_ref().unwrap().contains("template<typename T>"));
        assert!(vector.unwrap().signature.as_ref().unwrap().contains("class Vector"));

        let array = symbols.iter().find(|s| s.name == "Array");
        assert!(array.is_some());
        assert!(array.unwrap().signature.as_ref().unwrap().contains("template<typename T, size_t N>"));

        let max_func = symbols.iter().find(|s| s.name == "max");
        assert!(max_func.is_some());
        assert_eq!(max_func.unwrap().kind, SymbolKind::Function);
        assert!(max_func.unwrap().signature.as_ref().unwrap().contains("template<typename T>"));

        let add_func = symbols.iter().find(|s| s.name == "add");
        assert!(add_func.is_some());
        assert!(add_func.unwrap().signature.as_ref().unwrap().contains("auto add(T a, U b) -> decltype(a + b)"));

        let vector_bool = symbols.iter().find(|s| s.name == "Vector" && s.signature.as_ref().unwrap().contains("<bool>"));
        assert!(vector_bool.is_some());
    }


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

        let (mut extractor, tree) = create_extractor_and_parse(cpp_code);
        let symbols = extractor.extract_symbols(&tree);

        let factorial = symbols.iter().find(|s| s.name == "factorial");
        assert!(factorial.is_some());
        assert_eq!(factorial.unwrap().kind, SymbolKind::Function);

        let square = symbols.iter().find(|s| s.name == "square");
        assert!(square.is_some());
        assert!(square.unwrap().signature.as_ref().unwrap().contains("inline"));

        let plus_op = symbols.iter().find(|s| s.name == "operator+");
        assert!(plus_op.is_some());
        assert_eq!(plus_op.unwrap().kind, SymbolKind::Operator);
        assert!(plus_op.unwrap().signature.as_ref().unwrap().contains("operator+"));

        let stream_op = symbols.iter().find(|s| s.name == "operator<<");
        assert!(stream_op.is_some());
        assert!(stream_op.unwrap().signature.as_ref().unwrap().contains("friend"));

        let conversion_op = symbols.iter().find(|s| s.name == "operator double");
        assert!(conversion_op.is_some());

        let call_op = symbols.iter().find(|s| s.name == "operator()");
        assert!(call_op.is_some());
    }

    #[test]
    fn test_extract_struct_and_union_declarations() {
        let cpp_code = r#"
struct Point {
    double x, y;

    Point(double x = 0, double y = 0) : x(x), y(y) {}

    double distance() const {
        return sqrt(x * x + y * y);
    }
};

struct alignas(16) AlignedData {
    float data[4];
};

union Value {
    int i;
    float f;
    double d;
    char c[8];

    Value() : i(0) {}
    Value(int val) : i(val) {}
    Value(float val) : f(val) {}
};

// Anonymous union
struct Variant {
    enum Type { INT, FLOAT, STRING } type;

    union {
        int int_val;
        float float_val;
        std::string* string_val;
    };

    Variant(int val) : type(INT), int_val(val) {}
};
"#;

        let (mut extractor, tree) = create_extractor_and_parse(cpp_code);
        let symbols = extractor.extract_symbols(&tree);

        let point = symbols.iter().find(|s| s.name == "Point");
        assert!(point.is_some());
        assert_eq!(point.unwrap().kind, SymbolKind::Struct);
        assert!(point.unwrap().signature.as_ref().unwrap().contains("struct Point"));

        let aligned_data = symbols.iter().find(|s| s.name == "AlignedData");
        assert!(aligned_data.is_some());
        assert!(aligned_data.unwrap().signature.as_ref().unwrap().contains("alignas(16)"));

        let value = symbols.iter().find(|s| s.name == "Value");
        assert!(value.is_some());
        assert_eq!(value.unwrap().kind, SymbolKind::Union);

        let distance = symbols.iter().find(|s| s.name == "distance");
        assert!(distance.is_some());
        assert_eq!(distance.unwrap().kind, SymbolKind::Method);

        let variant = symbols.iter().find(|s| s.name == "Variant");
        assert!(variant.is_some());
        assert_eq!(variant.unwrap().kind, SymbolKind::Struct);
    }

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

        let (mut extractor, tree) = create_extractor_and_parse(cpp_code);
        let symbols = extractor.extract_symbols(&tree);

        let color = symbols.iter().find(|s| s.name == "Color");
        assert!(color.is_some());
        assert_eq!(color.unwrap().kind, SymbolKind::Enum);
        assert!(color.unwrap().signature.as_ref().unwrap().contains("enum Color"));

        let status = symbols.iter().find(|s| s.name == "Status");
        assert!(status.is_some());
        assert!(status.unwrap().signature.as_ref().unwrap().contains("enum class Status : uint8_t"));

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
    fn test_extract_constructors_and_destructors_with_various_patterns() {
        let cpp_code = r#"
class Resource {
public:
    // Default constructor
    Resource();

    // Parameterized constructor
    Resource(const std::string& name, size_t size);

    // Copy constructor
    Resource(const Resource& other);

    // Move constructor
    Resource(Resource&& other) noexcept;

    // Copy assignment operator
    Resource& operator=(const Resource& other);

    // Move assignment operator
    Resource& operator=(Resource&& other) noexcept;

    // Destructor
    virtual ~Resource();

    // Deleted functions
    Resource(const Resource& other, int) = delete;

    // Defaulted functions
    Resource(int) = default;

private:
    std::string name_;
    std::unique_ptr<char[]> data_;
    size_t size_;
};

template<typename T>
class Container {
public:
    Container();
    explicit Container(size_t capacity);
    ~Container();

    template<typename U>
    Container(const Container<U>& other);

private:
    T* data_;
    size_t size_;
};
"#;

        let (mut extractor, tree) = create_extractor_and_parse(cpp_code);
        let symbols = extractor.extract_symbols(&tree);

        let default_ctor = symbols.iter().find(|s| s.name == "Resource" && s.signature.as_ref().unwrap().contains("Resource()"));
        assert!(default_ctor.is_some());
        assert_eq!(default_ctor.unwrap().kind, SymbolKind::Constructor);

        let param_ctor = symbols.iter().find(|s| s.name == "Resource" && s.signature.as_ref().unwrap().contains("const std::string& name"));
        assert!(param_ctor.is_some());
        assert_eq!(param_ctor.unwrap().kind, SymbolKind::Constructor);

        let destructor = symbols.iter().find(|s| s.name == "~Resource");
        assert!(destructor.is_some());
        assert_eq!(destructor.unwrap().kind, SymbolKind::Destructor);
        assert!(destructor.unwrap().signature.as_ref().unwrap().contains("virtual"));

        let move_ctor = symbols.iter().find(|s| s.name == "Resource" && s.signature.as_ref().unwrap().contains("Resource&& other"));
        assert!(move_ctor.is_some());
        assert!(move_ctor.unwrap().signature.as_ref().unwrap().contains("noexcept"));

        let deleted_ctor = symbols.iter().find(|s| s.name == "Resource" && s.signature.as_ref().unwrap().contains("= delete"));
        assert!(deleted_ctor.is_some());

        let container_template_ctor = symbols.iter().find(|s| s.name == "Container" && s.signature.as_ref().unwrap().contains("explicit"));
        assert!(container_template_ctor.is_some());
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

        let (mut extractor, tree) = create_extractor_and_parse(cpp_code);
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
        assert!(max_size.unwrap().signature.as_ref().unwrap().contains("constexpr"));

        let app_name = symbols.iter().find(|s| s.name == "app_name");
        assert!(app_name.is_some());
        assert!(app_name.unwrap().signature.as_ref().unwrap().contains("static"));

        let default_port = symbols.iter().find(|s| s.name == "DEFAULT_PORT");
        assert!(default_port.is_some());
        assert_eq!(default_port.unwrap().kind, SymbolKind::Constant);

        let debug_mode = symbols.iter().find(|s| s.name == "DEBUG_MODE");
        assert!(debug_mode.is_some());
        assert!(debug_mode.unwrap().signature.as_ref().unwrap().contains("inline constexpr"));
    }

    #[test]
    fn test_handle_friend_declarations_and_access_specifiers() {
        let cpp_code = r#"
class Matrix;  // Forward declaration

class Vector {
private:
    double* data;
    size_t size;

public:
    Vector(size_t n);
    ~Vector();

    // Friend function declarations
    friend double dot(const Vector& a, const Vector& b);
    friend Vector operator+(const Vector& a, const Vector& b);
    friend class Matrix;

    // Friend template
    template<typename T>
    friend class SmartPtr;

protected:
    void resize(size_t new_size);

private:
    void cleanup();
};

class Matrix {
public:
    Matrix(size_t rows, size_t cols);

    // Can access private members of Vector because of friend declaration
    Vector multiply(const Vector& v) const;

private:
    double** data;
    size_t rows, cols;
};
"#;

        let (mut extractor, tree) = create_extractor_and_parse(cpp_code);
        let symbols = extractor.extract_symbols(&tree);

        let dot_func = symbols.iter().find(|s| s.name == "dot");
        assert!(dot_func.is_some());
        assert_eq!(dot_func.unwrap().kind, SymbolKind::Function);
        assert!(dot_func.unwrap().signature.as_ref().unwrap().contains("friend"));

        let plus_op = symbols.iter().find(|s| s.name == "operator+");
        assert!(plus_op.is_some());
        assert_eq!(plus_op.unwrap().kind, SymbolKind::Operator);
        assert!(plus_op.unwrap().signature.as_ref().unwrap().contains("friend"));

        let vector_class = symbols.iter().find(|s| s.name == "Vector");
        assert!(vector_class.is_some());
        assert_eq!(vector_class.unwrap().kind, SymbolKind::Class);

        // Check access specifier handling
        let resize_method = symbols.iter().find(|s| s.name == "resize");
        assert!(resize_method.is_some());
        // Note: Visibility extraction will be tested in the implementation
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

        let (mut extractor, tree) = create_extractor_and_parse(cpp_code);
        let symbols = extractor.extract_symbols(&tree);
        let types = extractor.infer_types(&symbols);

        let get_value = symbols.iter().find(|s| s.name == "getValue");
        assert!(get_value.is_some());

        assert!(get_value.unwrap().signature.as_ref().unwrap().contains("-> int"));

        let process_func = symbols.iter().find(|s| s.name == "process");
        assert!(process_func.is_some());
        assert!(process_func.unwrap().signature.as_ref().unwrap().contains("auto process"));

        // Type inference should work
        assert!(!types.is_empty());
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

        let (mut extractor, tree) = create_extractor_and_parse(cpp_code);
        let symbols = extractor.extract_symbols(&tree);
        let relationships = extractor.extract_relationships(&tree, &symbols);

        // Check inheritance relationships
        let derived_extends_base = relationships.iter().any(|r| {
            r.kind == RelationshipKind::Extends &&
            symbols.iter().any(|s| s.id == r.from_symbol_id && s.name == "Derived") &&
            symbols.iter().any(|s| s.id == r.to_symbol_id && s.name == "Base")
        });
        assert!(derived_extends_base);

        let container_extends_base = relationships.iter().any(|r| {
            r.kind == RelationshipKind::Extends &&
            symbols.iter().any(|s| s.id == r.from_symbol_id && s.name == "Container") &&
            symbols.iter().any(|s| s.id == r.to_symbol_id && s.name == "Base")
        });
        assert!(container_extends_base);
    }

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

        let (mut extractor, tree) = create_extractor_and_parse(cpp_code);
        let symbols = extractor.extract_symbols(&tree);

        let modern_class = symbols.iter().find(|s| s.name == "ModernCpp");
        assert!(modern_class.is_some());

        let use_lambdas = symbols.iter().find(|s| s.name == "useLambdas");
        assert!(use_lambdas.is_some());

        let generic_lambda = symbols.iter().find(|s| s.name == "generic_lambda");
        assert!(generic_lambda.is_some());

        let pi_template = symbols.iter().find(|s| s.name == "pi");
        assert!(pi_template.is_some());
        assert!(pi_template.unwrap().signature.as_ref().unwrap().contains("constexpr"));

        // Note: Lambda extraction within functions is complex and may not be fully implemented initially
        let lambdas_count = symbols.iter().filter(|s| s.name.contains("lambda")).count();
        // At minimum, we should find the generic_lambda
        assert!(lambdas_count >= 1);
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

        let (mut extractor, tree) = create_extractor_and_parse(cpp_code);
        let symbols = extractor.extract_symbols(&tree);

        let variadic_template = symbols.iter().find(|s| s.name == "VariadicTemplate");
        assert!(variadic_template.is_some());
        assert!(variadic_template.unwrap().signature.as_ref().unwrap().contains("typename... Args"));

        let process_method = symbols.iter().find(|s| s.name == "process");
        assert!(process_method.is_some());

        let increment_funcs = symbols.iter().filter(|s| s.name == "increment").count();
        assert_eq!(increment_funcs, 2); // Two overloads with SFINAE

        let is_pointer_trait = symbols.iter().find(|s| s.name == "is_pointer");
        assert!(is_pointer_trait.is_some());

        let perfect_forward = symbols.iter().find(|s| s.name == "perfect_forward");
        assert!(perfect_forward.is_some());
        assert!(perfect_forward.unwrap().signature.as_ref().unwrap().contains("decltype(auto)"));
    }

    #[test]
    fn test_extract_threading_async_and_synchronization_primitives() {
        let cpp_code = r#"
#include <thread>
#include <mutex>
#include <condition_variable>
#include <future>
#include <atomic>

class ThreadSafeCounter {
public:
    void increment() {
        std::lock_guard<std::mutex> lock(mutex_);
        ++count_;
    }

    int get() const {
        std::lock_guard<std::mutex> lock(mutex_);
        return count_;
    }

    void wait_for_condition() {
        std::unique_lock<std::mutex> lock(mutex_);
        cv_.wait(lock, [this] { return count_ > 10; });
    }

private:
    mutable std::mutex mutex_;
    std::condition_variable cv_;
    int count_ = 0;
};

class AtomicOperations {
public:
    void atomic_ops() {
        counter_.store(42, std::memory_order_release);
        int value = counter_.load(std::memory_order_acquire);
        counter_.fetch_add(1, std::memory_order_acq_rel);
    }

private:
    std::atomic<int> counter_{0};
    std::atomic_flag flag_ = ATOMIC_FLAG_INIT;
};

class AsyncOperations {
public:
    std::future<int> async_computation() {
        return std::async(std::launch::async, []() {
            std::this_thread::sleep_for(std::chrono::milliseconds(100));
            return 42;
        });
    }

    void promise_example() {
        std::promise<std::string> promise;
        auto future = promise.get_future();

        std::thread worker([&promise]() {
            promise.set_value("Hello from thread!");
        });

        worker.join();
    }
};

// Thread-local storage
thread_local int tls_counter = 0;

// Memory ordering
std::atomic<bool> ready{false};
std::atomic<int> data{0};

void producer() {
    data.store(42, std::memory_order_relaxed);
    ready.store(true, std::memory_order_release);
}
"#;

        let (mut extractor, tree) = create_extractor_and_parse(cpp_code);
        let symbols = extractor.extract_symbols(&tree);

        let thread_safe_counter = symbols.iter().find(|s| s.name == "ThreadSafeCounter");
        assert!(thread_safe_counter.is_some());

        let increment_method = symbols.iter().find(|s| s.name == "increment");
        assert!(increment_method.is_some());

        let atomic_ops_class = symbols.iter().find(|s| s.name == "AtomicOperations");
        assert!(atomic_ops_class.is_some());

        let async_computation = symbols.iter().find(|s| s.name == "async_computation");
        assert!(async_computation.is_some());
        assert!(async_computation.unwrap().signature.as_ref().unwrap().contains("std::future<int>"));

        let tls_counter = symbols.iter().find(|s| s.name == "tls_counter");
        assert!(tls_counter.is_some());
        assert!(tls_counter.unwrap().signature.as_ref().unwrap().contains("thread_local"));

        let producer = symbols.iter().find(|s| s.name == "producer");
        assert!(producer.is_some());
    }

    #[test]
    fn test_extract_exception_handling_and_raii_patterns() {
        let cpp_code = r#"
#include <exception>
#include <stdexcept>
#include <memory>

// Custom exception class
class DatabaseException : public std::runtime_error {
public:
    explicit DatabaseException(const std::string& msg)
        : std::runtime_error(msg), error_code_(0) {}

    DatabaseException(const std::string& msg, int code)
        : std::runtime_error(msg), error_code_(code) {}

    int error_code() const noexcept { return error_code_; }

private:
    int error_code_;
};

// RAII wrapper for file handling
class FileGuard {
public:
    explicit FileGuard(const std::string& filename)
        : file_(std::fopen(filename.c_str(), "r")) {
        if (!file_) {
            throw std::runtime_error("Failed to open file: " + filename);
        }
    }

    ~FileGuard() noexcept {
        if (file_) {
            std::fclose(file_);
        }
    }

    // Non-copyable
    FileGuard(const FileGuard&) = delete;
    FileGuard& operator=(const FileGuard&) = delete;

    // Movable
    FileGuard(FileGuard&& other) noexcept : file_(other.file_) {
        other.file_ = nullptr;
    }

    FileGuard& operator=(FileGuard&& other) noexcept {
        if (this != &other) {
            if (file_) std::fclose(file_);
            file_ = other.file_;
            other.file_ = nullptr;
        }
        return *this;
    }

    FILE* get() const noexcept { return file_; }

private:
    FILE* file_;
};

class ExceptionSafetyDemo {
public:
    void strong_guarantee() try {
        // All operations succeed or all fail
        auto backup = data_;
        data_.clear();
        data_ = process_data();
    } catch (...) {
        // Restore state on exception
        throw;
    }

    void no_throw_swap(ExceptionSafetyDemo& other) noexcept {
        using std::swap;
        swap(data_, other.data_);
    }

private:
    std::vector<int> data_;

    std::vector<int> process_data() {
        // Simulate processing that might throw
        if (data_.empty()) {
            throw DatabaseException("No data to process");
        }
        return data_;
    }
};

// Exception specification (deprecated but still used)
void legacy_function() throw(std::bad_alloc, DatabaseException);

// Modern exception specification
void modern_function() noexcept;
void maybe_throws() noexcept(false);
"#;

        let (mut extractor, tree) = create_extractor_and_parse(cpp_code);
        let symbols = extractor.extract_symbols(&tree);

        let db_exception = symbols.iter().find(|s| s.name == "DatabaseException");
        assert!(db_exception.is_some());
        assert!(db_exception.unwrap().signature.as_ref().unwrap().contains("public std::runtime_error"));

        let file_guard = symbols.iter().find(|s| s.name == "FileGuard");
        assert!(file_guard.is_some());

        let file_guard_ctor = symbols.iter().find(|s| s.name == "FileGuard" && s.kind == SymbolKind::Constructor);
        assert!(file_guard_ctor.is_some());

        let file_guard_dtor = symbols.iter().find(|s| s.name == "~FileGuard");
        assert!(file_guard_dtor.is_some());
        assert!(file_guard_dtor.unwrap().signature.as_ref().unwrap().contains("noexcept"));

        let strong_guarantee = symbols.iter().find(|s| s.name == "strong_guarantee");
        assert!(strong_guarantee.is_some());

        let modern_function = symbols.iter().find(|s| s.name == "modern_function");
        assert!(modern_function.is_some());
        assert!(modern_function.unwrap().signature.as_ref().unwrap().contains("noexcept"));
    }

    #[test]
    fn test_extract_google_test_catch2_and_boost_test_patterns() {
        let cpp_code = r#"
#include <gtest/gtest.h>
#include <catch2/catch.hpp>
#include <boost/test/unit_test.hpp>

// Google Test patterns
TEST(MathTest, Addition) {
    EXPECT_EQ(2 + 2, 4);
    ASSERT_TRUE(true);
}

TEST_F(DatabaseTest, Connection) {
    EXPECT_NO_THROW(db_.connect());
}

class DatabaseTest : public ::testing::Test {
protected:
    void SetUp() override {
        db_.initialize();
    }

    void TearDown() override {
        db_.cleanup();
    }

    Database db_;
};

// Parameterized test
class ParameterizedMathTest : public ::testing::TestWithParam<int> {};

TEST_P(ParameterizedMathTest, Square) {
    int value = GetParam();
    EXPECT_GT(value * value, 0);
}

// Catch2 patterns
TEST_CASE("Vector operations", "[vector]") {
    std::vector<int> v{1, 2, 3};

    SECTION("push_back increases size") {
        v.push_back(4);
        REQUIRE(v.size() == 4);
    }

    SECTION("clear empties vector") {
        v.clear();
        CHECK(v.empty());
    }
}

SCENARIO("User authentication", "[auth]") {
    GIVEN("A user with valid credentials") {
        User user("john", "password123");

        WHEN("they attempt to login") {
            bool result = user.authenticate();

            THEN("authentication succeeds") {
                REQUIRE(result == true);
            }
        }
    }
}

// Boost.Test patterns
BOOST_AUTO_TEST_SUITE(StringTests)

BOOST_AUTO_TEST_CASE(StringLength) {
    std::string str = "hello";
    BOOST_CHECK_EQUAL(str.length(), 5);
}

BOOST_AUTO_TEST_CASE(StringConcatenation) {
    std::string a = "hello";
    std::string b = "world";
    BOOST_REQUIRE_EQUAL(a + b, "helloworld");
}

BOOST_AUTO_TEST_SUITE_END()

// Fixture class
class FixtureTest {
public:
    FixtureTest() : value_(42) {}

protected:
    int value_;
};

BOOST_FIXTURE_TEST_CASE(FixtureUsage, FixtureTest) {
    BOOST_CHECK_EQUAL(value_, 42);
}

// Custom matchers and assertions
MATCHER_P(IsMultipleOf, n, "") {
    return (arg % n) == 0;
}

TEST(CustomMatchers, MultipleTest) {
    EXPECT_THAT(15, IsMultipleOf(3));
}
"#;

        let (mut extractor, tree) = create_extractor_and_parse(cpp_code);
        let symbols = extractor.extract_symbols(&tree);

        // Google Test macros create functions
        let math_test_addition = symbols.iter().find(|s| s.name.contains("MathTest") || s.name.contains("Addition"));
        // Note: TEST macro expansion may not be fully parsed by tree-sitter

        let database_test = symbols.iter().find(|s| s.name == "DatabaseTest");
        assert!(database_test.is_some());
        assert_eq!(database_test.unwrap().kind, SymbolKind::Class);

        let setup_method = symbols.iter().find(|s| s.name == "SetUp");
        assert!(setup_method.is_some());

        let parameterized_test = symbols.iter().find(|s| s.name == "ParameterizedMathTest");
        assert!(parameterized_test.is_some());

        let fixture_test = symbols.iter().find(|s| s.name == "FixtureTest");
        assert!(fixture_test.is_some());

        // Note: Macro-generated test functions may not be captured perfectly
        // This depends on tree-sitter's ability to parse macro expansions
        let total_test_classes = symbols.iter().filter(|s| {
            s.name.contains("Test") && s.kind == SymbolKind::Class
        }).count();
        assert!(total_test_classes >= 2); // At least DatabaseTest and ParameterizedMathTest
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

        let (mut extractor, tree) = create_extractor_and_parse(cpp_code);
        let symbols = extractor.extract_symbols(&tree);

        let factorial = symbols.iter().find(|s| s.name == "Factorial");
        assert!(factorial.is_some());
        assert!(factorial.unwrap().signature.as_ref().unwrap().contains("template<int N>"));

        let multi_container = symbols.iter().find(|s| s.name == "MultiContainer");
        assert!(multi_container.is_some());
        assert!(multi_container.unwrap().signature.as_ref().unwrap().contains("template<template<typename> class Container"));

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

        let (mut extractor, tree) = create_extractor_and_parse(cpp_code);
        let symbols = extractor.extract_symbols(&tree);

        let complex_class = symbols.iter().find(|s| s.name == "ComplexClass");
        assert!(complex_class.is_some());

        let nested_template = symbols.iter().find(|s| s.name == "NestedTemplate");
        assert!(nested_template.is_some());

        // Should handle malformed syntax gracefully
        // The extractor should not crash and should extract what it can

        // Check for platform-specific classes (at least one should be found)
        let platform_classes = symbols.iter().filter(|s| {
            s.name.contains("Windows") || s.name.contains("Linux") || s.name.contains("Default")
        }).count();
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