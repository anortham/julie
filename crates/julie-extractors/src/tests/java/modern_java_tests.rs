// Modern Java Features Tests
//
// Tests for Java 8+ features including:
// - Lambda expressions
// - Method references
// - Streams API
// - Records (Java 14+)
// - Text blocks (Java 13+)

use crate::base::{SymbolKind, Visibility};
use crate::java::JavaExtractor;
use crate::tests::test_utils::init_parser;
use std::path::PathBuf;

#[cfg(test)]
mod modern_java_tests {
    use super::*;

    #[test]
    fn test_extract_lambda_expressions() {
        let workspace_root = PathBuf::from("/tmp/test");
        let code = r#"
import java.util.*;
import java.util.function.*;

public class LambdaExample {
    public void process() {
        List<String> names = Arrays.asList("Alice", "Bob", "Charlie");

        // Lambda in forEach
        names.forEach(name -> System.out.println(name));

        // Lambda with multiple statements
        names.stream()
            .filter(name -> {
                System.out.println("Filtering: " + name);
                return name.length() > 3;
            })
            .forEach(System.out::println);

        // Lambda as function parameter
        Predicate<String> startsWithA = s -> s.startsWith("A");
        Function<String, Integer> length = String::length;
        Consumer<String> printer = System.out::println;
    }
}
"#;

        let tree = init_parser(code, "java");

        let mut extractor = JavaExtractor::new(
            "java".to_string(),
            "test.java".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        // Verify the class and methods are extracted
        let lambda_example = symbols.iter().find(|s| s.name == "LambdaExample");
        assert!(lambda_example.is_some());

        let process_method = symbols.iter().find(|s| s.name == "process");
        assert!(process_method.is_some());
        assert_eq!(process_method.unwrap().kind, SymbolKind::Method);
    }

    #[test]
    fn test_extract_records() {
        let workspace_root = PathBuf::from("/tmp/test");
        let code = r#"
public record Person(String name, int age) {
    public Person {
        if (age < 0) {
            throw new IllegalArgumentException("Age cannot be negative");
        }
    }

    public static Person create(String name, int age) {
        return new Person(name, age);
    }
}

record Point(int x, int y) implements Comparable<Point> {
    @Override
    public int compareTo(Point other) {
        return Integer.compare(this.x, other.x);
    }
}
"#;

        let tree = init_parser(code, "java");

        let mut extractor = JavaExtractor::new(
            "java".to_string(),
            "test.java".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        let person_record = symbols.iter().find(|s| s.name == "Person");
        assert!(person_record.is_some());
        assert_eq!(person_record.unwrap().kind, SymbolKind::Class); // Records are classes
        assert!(
            person_record
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("public record Person(String name, int age)")
        );

        let point_record = symbols.iter().find(|s| s.name == "Point");
        assert!(point_record.is_some());
        assert!(
            point_record
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("record Point(int x, int y)")
        );

        // Check for record methods
        let create_method = symbols.iter().find(|s| s.name == "create");
        assert!(create_method.is_some());
        assert_eq!(create_method.unwrap().kind, SymbolKind::Method);

        let compare_to_method = symbols.iter().find(|s| s.name == "compareTo");
        assert!(compare_to_method.is_some());
        assert_eq!(compare_to_method.unwrap().kind, SymbolKind::Method);
    }

    #[test]
    fn test_extract_text_blocks() {
        let workspace_root = PathBuf::from("/tmp/test");
        let code = r#"
public class TextBlockExample {
    public void demonstrate() {
        String html = """
            <html>
                <body>
                    <h1>Hello, World!</h1>
                </body>
            </html>
            """;

        String json = """
            {
                "name": "John",
                "age": 30
            }
            """;

        String sql = """
            SELECT *
            FROM users
            WHERE active = true
            """;
    }
}
"#;

        let tree = init_parser(code, "java");

        let mut extractor = JavaExtractor::new(
            "java".to_string(),
            "test.java".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        let text_block_example = symbols.iter().find(|s| s.name == "TextBlockExample");
        assert!(text_block_example.is_some());

        let demonstrate_method = symbols.iter().find(|s| s.name == "demonstrate");
        assert!(demonstrate_method.is_some());
        assert_eq!(demonstrate_method.unwrap().kind, SymbolKind::Method);
    }

    #[test]
    fn test_extract_streams_and_optionals() {
        let workspace_root = PathBuf::from("/tmp/test");
        let code = r#"
import java.util.*;
import java.util.stream.*;

public class StreamExample {
    public void processData(List<String> data) {
        Optional<String> result = data.stream()
            .filter(s -> s.length() > 5)
            .map(String::toUpperCase)
            .findFirst()
            .orElse("DEFAULT");

        List<Integer> lengths = data.stream()
            .mapToInt(String::length)
            .boxed()
            .collect(Collectors.toList());
    }
}
"#;

        let tree = init_parser(code, "java");

        let mut extractor = JavaExtractor::new(
            "java".to_string(),
            "test.java".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        let stream_example = symbols.iter().find(|s| s.name == "StreamExample");
        assert!(stream_example.is_some());

        let process_data_method = symbols.iter().find(|s| s.name == "processData");
        assert!(process_data_method.is_some());
        assert_eq!(process_data_method.unwrap().kind, SymbolKind::Method);
        assert!(
            process_data_method
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("public void processData(List<String> data)")
        );
    }
}
