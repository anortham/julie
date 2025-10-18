use super::{parse_cpp, SymbolKind};

#[cfg(test)]
mod tests {
    use super::*;

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

        let (mut extractor, tree) = parse_cpp(cpp_code);
        let symbols = extractor.extract_symbols(&tree);

        let shape = symbols.iter().find(|s| s.name == "Shape");
        assert!(shape.is_some());
        assert_eq!(shape.unwrap().kind, SymbolKind::Class);
        assert!(shape
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("class Shape"));

        let circle = symbols.iter().find(|s| s.name == "Circle");
        assert!(circle.is_some());
        assert!(circle
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("public Shape"));

        let colored_circle = symbols.iter().find(|s| s.name == "ColoredCircle");
        assert!(colored_circle.is_some());
        assert!(colored_circle
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("public Circle, public Drawable"));

        // Check methods
        let destructor = symbols.iter().find(|s| s.name == "~Shape");
        assert!(destructor.is_some());
        assert_eq!(destructor.unwrap().kind, SymbolKind::Destructor);

        let area = symbols.iter().find(|s| s.name == "area");
        assert!(area.is_some());
        assert_eq!(area.unwrap().kind, SymbolKind::Method);
        assert!(area
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("virtual"));

        let get_instance_count = symbols.iter().find(|s| s.name == "getInstanceCount");
        assert!(get_instance_count.is_some());
        assert!(get_instance_count
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("static"));
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

        let (mut extractor, tree) = parse_cpp(cpp_code);
        let symbols = extractor.extract_symbols(&tree);

        let point = symbols.iter().find(|s| s.name == "Point");
        assert!(point.is_some());
        assert_eq!(point.unwrap().kind, SymbolKind::Struct);
        assert!(point
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("struct Point"));

        let aligned_data = symbols.iter().find(|s| s.name == "AlignedData");
        assert!(aligned_data.is_some());
        assert!(aligned_data
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("alignas(16)"));

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

        let (mut extractor, tree) = parse_cpp(cpp_code);
        let symbols = extractor.extract_symbols(&tree);

        let default_ctor = symbols
            .iter()
            .find(|s| s.name == "Resource" && s.signature.as_ref().unwrap().contains("Resource()"));
        assert!(default_ctor.is_some());
        assert_eq!(default_ctor.unwrap().kind, SymbolKind::Constructor);

        let param_ctor = symbols.iter().find(|s| {
            s.name == "Resource"
                && s.signature
                    .as_ref()
                    .unwrap()
                    .contains("const std::string& name")
        });
        assert!(param_ctor.is_some());
        assert_eq!(param_ctor.unwrap().kind, SymbolKind::Constructor);

        let destructor = symbols.iter().find(|s| s.name == "~Resource");
        assert!(destructor.is_some());
        assert_eq!(destructor.unwrap().kind, SymbolKind::Destructor);
        assert!(destructor
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("virtual"));

        let move_ctor = symbols.iter().find(|s| {
            s.name == "Resource" && s.signature.as_ref().unwrap().contains("Resource&& other")
        });
        assert!(move_ctor.is_some());
        assert!(move_ctor
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("noexcept"));

        let deleted_ctor = symbols
            .iter()
            .find(|s| s.name == "Resource" && s.signature.as_ref().unwrap().contains("= delete"));
        assert!(deleted_ctor.is_some());

        let container_template_ctor = symbols
            .iter()
            .find(|s| s.name == "Container" && s.signature.as_ref().unwrap().contains("explicit"));
        assert!(container_template_ctor.is_some());
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

        let (mut extractor, tree) = parse_cpp(cpp_code);
        let symbols = extractor.extract_symbols(&tree);

        let dot_func = symbols.iter().find(|s| s.name == "dot");
        assert!(dot_func.is_some());
        assert_eq!(dot_func.unwrap().kind, SymbolKind::Function);
        assert!(dot_func
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("friend"));

        let plus_op = symbols.iter().find(|s| s.name == "operator+");
        assert!(plus_op.is_some());
        assert_eq!(plus_op.unwrap().kind, SymbolKind::Operator);
        assert!(plus_op
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("friend"));

        let vector_class = symbols.iter().find(|s| s.name == "Vector");
        assert!(vector_class.is_some());
        assert_eq!(vector_class.unwrap().kind, SymbolKind::Class);

        // Check access specifier handling

        // Private fields (lines 307-308)
        let data_field = symbols.iter().find(|s| s.name == "data");
        assert!(data_field.is_some(), "Should extract 'data' field");
        assert_eq!(
            data_field.unwrap().visibility,
            Some(crate::extractors::base::Visibility::Private),
            "Field 'data' should be Private (declared in private: section)"
        );

        let size_field = symbols.iter().find(|s| s.name == "size");
        assert!(size_field.is_some(), "Should extract 'size' field");
        assert_eq!(
            size_field.unwrap().visibility,
            Some(crate::extractors::base::Visibility::Private),
            "Field 'size' should be Private (declared in private: section)"
        );

        // Public constructor (line 311)
        let ctor = symbols
            .iter()
            .find(|s| s.name == "Vector" && s.kind == SymbolKind::Constructor);
        assert!(ctor.is_some(), "Should extract Vector constructor");
        assert_eq!(
            ctor.unwrap().visibility,
            Some(crate::extractors::base::Visibility::Public),
            "Constructor should be Public (declared in public: section)"
        );

        // Public destructor (line 312)
        let dtor = symbols.iter().find(|s| s.name == "~Vector");
        assert!(dtor.is_some(), "Should extract Vector destructor");
        assert_eq!(
            dtor.unwrap().visibility,
            Some(crate::extractors::base::Visibility::Public),
            "Destructor should be Public (declared in public: section)"
        );

        // Protected method (line 324)
        let resize_method = symbols.iter().find(|s| s.name == "resize");
        assert!(resize_method.is_some(), "Should extract 'resize' method");
        assert_eq!(
            resize_method.unwrap().visibility,
            Some(crate::extractors::base::Visibility::Protected),
            "Method 'resize' should be Protected (declared in protected: section)"
        );

        // Private method (line 327)
        let cleanup_method = symbols.iter().find(|s| s.name == "cleanup");
        assert!(cleanup_method.is_some(), "Should extract 'cleanup' method");
        assert_eq!(
            cleanup_method.unwrap().visibility,
            Some(crate::extractors::base::Visibility::Private),
            "Method 'cleanup' should be Private (declared in private: section)"
        );
    }
}
