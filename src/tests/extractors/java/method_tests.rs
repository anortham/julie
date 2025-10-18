// Method and Constructor Extraction Tests
//
// Tests for Java method and constructor extraction including:
// - Method definitions with various modifiers
// - Constructor definitions
// - Method parameters and return types
// - Overloaded methods

use super::*;

#[cfg(test)]
mod method_tests {
    use super::*;

    #[test]
    fn test_extract_method_definitions() {
        let code = r#"
public class Calculator {
    public int add(int a, int b) {
        return a + b;
    }

    private static double multiply(double x, double y) {
        return x * y;
    }

    protected void process() {
        // Process logic
    }

    final String format(String pattern, Object... args) {
        return String.format(pattern, args);
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = JavaExtractor::new(
            "java".to_string(),
            "test.java".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        let add_method = symbols.iter().find(|s| s.name == "add");
        assert!(add_method.is_some());
        assert_eq!(add_method.unwrap().kind, SymbolKind::Method);
        assert!(add_method
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("public int add(int a, int b)"));

        let multiply_method = symbols.iter().find(|s| s.name == "multiply");
        assert!(multiply_method.is_some());
        assert!(multiply_method
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("private static double multiply"));

        let format_method = symbols.iter().find(|s| s.name == "format");
        assert!(format_method.is_some());
        assert!(format_method
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("final String format(String pattern, Object... args)"));
    }

    #[test]
    fn test_extract_constructor_definitions() {
        let code = r#"
public class Person {
    private String name;
    private int age;

    public Person() {
        this("Unknown", 0);
    }

    public Person(String name) {
        this(name, 0);
    }

    public Person(String name, int age) {
        this.name = name;
        this.age = age;
    }

    private Person(Person other) {
        this(other.name, other.age);
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = JavaExtractor::new(
            "java".to_string(),
            "test.java".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Constructors should be extracted as constructors with the class name
        let constructors: Vec<_> = symbols
            .iter()
            .filter(|s| s.name == "Person" && s.kind == SymbolKind::Constructor)
            .collect();

        assert_eq!(constructors.len(), 4, "Should extract all 4 constructors");

        // Check signatures contain constructor patterns
        let signatures: Vec<_> = constructors
            .iter()
            .filter_map(|s| s.signature.as_ref())
            .collect();

        assert!(signatures.iter().any(|s| s.contains("public Person()")));
        assert!(signatures
            .iter()
            .any(|s| s.contains("public Person(String name)")));
        assert!(signatures
            .iter()
            .any(|s| s.contains("public Person(String name, int age)")));
        assert!(signatures
            .iter()
            .any(|s| s.contains("private Person(Person other)")));
    }

    #[test]
    fn test_extract_overloaded_methods() {
        let code = r#"
public class OverloadedExample {
    public void process() {
        // No parameters
    }

    public void process(int value) {
        // Single int parameter
    }

    public void process(String text) {
        // Single string parameter
    }

    public void process(int value, String text) {
        // Two parameters
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = JavaExtractor::new(
            "java".to_string(),
            "test.java".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        let process_methods: Vec<_> = symbols
            .iter()
            .filter(|s| s.name == "process" && s.kind == SymbolKind::Method)
            .collect();

        assert_eq!(
            process_methods.len(),
            4,
            "Should extract all 4 overloaded process methods"
        );

        let signatures: Vec<_> = process_methods
            .iter()
            .filter_map(|s| s.signature.as_ref())
            .collect();

        assert!(signatures
            .iter()
            .any(|s| s.contains("public void process()")));
        assert!(signatures
            .iter()
            .any(|s| s.contains("public void process(int value)")));
        assert!(signatures
            .iter()
            .any(|s| s.contains("public void process(String text)")));
        assert!(signatures
            .iter()
            .any(|s| s.contains("public void process(int value, String text)")));
    }

    #[test]
    fn test_extract_abstract_methods() {
        let code = r#"
public abstract class AbstractProcessor {
    public abstract void process();

    public abstract String transform(String input);

    protected abstract int calculate(int a, int b);
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = JavaExtractor::new(
            "java".to_string(),
            "test.java".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        let process_method = symbols.iter().find(|s| s.name == "process");
        assert!(process_method.is_some());
        assert!(process_method
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("public abstract void process()"));

        let transform_method = symbols.iter().find(|s| s.name == "transform");
        assert!(transform_method.is_some());
        assert!(transform_method
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("public abstract String transform(String input)"));
    }
}
