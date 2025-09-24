// Java Extractor Tests
//
// Direct port of Miller's Java extractor tests (TDD RED phase)
// Original: /Users/murphy/Source/miller/src/__tests__/parser/java-extractor.test.ts
//
// Comprehensive test suite covering:
// - Packages, imports, classes, interfaces, enums, annotations
// - Modern Java features (records, sealed classes, pattern matching)
// - Exception handling, testing patterns, generics, nested classes
// - Performance and edge case handling

use crate::extractors::base::{Symbol, SymbolKind};
use crate::extractors::java::JavaExtractor;
use tree_sitter::Parser;

/// Initialize Java parser
fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_java::LANGUAGE.into()).expect("Error loading Java grammar");
    parser
}

#[cfg(test)]
mod java_extractor_tests {
    use super::*;

    // Package and Import Extraction Tests
    #[test]
    fn test_extract_package_declarations() {
        let code = r#"
package com.example.app;

package com.acme.utils;
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = JavaExtractor::new(
            "java".to_string(),
            "test.java".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        let app_package = symbols.iter().find(|s| s.name == "com.example.app");
        assert!(app_package.is_some());
        assert_eq!(app_package.unwrap().kind, SymbolKind::Namespace);
        assert!(app_package.unwrap().signature.as_ref().unwrap().contains("package com.example.app"));
        assert_eq!(app_package.unwrap().visibility.as_ref().unwrap(), "public");
    }

    #[test]
    fn test_extract_import_statements() {
        let code = r#"
package com.example;

import java.util.List;
import java.util.ArrayList;
import java.util.Map;
import static java.lang.Math.PI;
import static java.util.Collections.*;
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = JavaExtractor::new(
            "java".to_string(),
            "test.java".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        let list_import = symbols.iter().find(|s| s.name == "List");
        assert!(list_import.is_some());
        assert_eq!(list_import.unwrap().kind, SymbolKind::Import);
        assert!(list_import.unwrap().signature.as_ref().unwrap().contains("import java.util.List"));

        let pi_import = symbols.iter().find(|s| s.name == "PI");
        assert!(pi_import.is_some());
        assert!(pi_import.unwrap().signature.as_ref().unwrap().contains("import static java.lang.Math.PI"));

        let collections_import = symbols.iter().find(|s| s.name == "Collections");
        assert!(collections_import.is_some());
        assert!(collections_import.unwrap().signature.as_ref().unwrap().contains("import static java.util.Collections.*"));
    }

    // Class Extraction Tests
    #[test]
    fn test_extract_class_definitions_with_modifiers() {
        let code = r#"
package com.example;

public class User {
    private String name;
    public int age;
}

abstract class Animal {
    abstract void makeSound();
}

final class Constants {
    public static final String VERSION = "1.0";
}

class DefaultClass {
    // package-private class
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

        let user_class = symbols.iter().find(|s| s.name == "User");
        assert!(user_class.is_some());
        assert_eq!(user_class.unwrap().kind, SymbolKind::Class);
        assert!(user_class.unwrap().signature.as_ref().unwrap().contains("public class User"));
        assert_eq!(user_class.unwrap().visibility.as_ref().unwrap(), "public");

        let animal_class = symbols.iter().find(|s| s.name == "Animal");
        assert!(animal_class.is_some());
        assert!(animal_class.unwrap().signature.as_ref().unwrap().contains("abstract class Animal"));
        assert_eq!(animal_class.unwrap().visibility.as_ref().unwrap(), "package");

        let constants_class = symbols.iter().find(|s| s.name == "Constants");
        assert!(constants_class.is_some());
        assert!(constants_class.unwrap().signature.as_ref().unwrap().contains("final class Constants"));

        let default_class = symbols.iter().find(|s| s.name == "DefaultClass");
        assert!(default_class.is_some());
        assert_eq!(default_class.unwrap().visibility.as_ref().unwrap(), "package");
    }

    #[test]
    fn test_extract_class_inheritance_and_implementations() {
        let code = r#"
package com.example;

public class Dog extends Animal implements Runnable, Serializable {
    public void run() {}
}

public class Cat extends Animal {
    public void meow() {}
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

        let dog_class = symbols.iter().find(|s| s.name == "Dog");
        assert!(dog_class.is_some());
        assert!(dog_class.unwrap().signature.as_ref().unwrap().contains("extends Animal"));
        assert!(dog_class.unwrap().signature.as_ref().unwrap().contains("implements Runnable, Serializable"));

        let cat_class = symbols.iter().find(|s| s.name == "Cat");
        assert!(cat_class.is_some());
        assert!(cat_class.unwrap().signature.as_ref().unwrap().contains("extends Animal"));
    }

    // Interface Extraction Tests
    #[test]
    fn test_extract_interface_definitions() {
        let code = r#"
package com.example;

public interface Drawable {
    void draw();
    default void render() {
        draw();
    }
}

interface Serializable extends Cloneable {
    // marker interface
}

@FunctionalInterface
public interface Consumer<T> {
    void accept(T t);
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

        let drawable = symbols.iter().find(|s| s.name == "Drawable");
        assert!(drawable.is_some());
        assert_eq!(drawable.unwrap().kind, SymbolKind::Interface);
        assert!(drawable.unwrap().signature.as_ref().unwrap().contains("public interface Drawable"));
        assert_eq!(drawable.unwrap().visibility.as_ref().unwrap(), "public");

        let serializable = symbols.iter().find(|s| s.name == "Serializable");
        assert!(serializable.is_some());
        assert!(serializable.unwrap().signature.as_ref().unwrap().contains("extends Cloneable"));

        let consumer = symbols.iter().find(|s| s.name == "Consumer");
        assert!(consumer.is_some());
        assert!(consumer.unwrap().signature.as_ref().unwrap().contains("Consumer<T>"));
    }

    // Method Extraction Tests
    #[test]
    fn test_extract_method_definitions_with_parameters_and_return_types() {
        let code = r#"
package com.example;

public class Calculator {
    public int add(int a, int b) {
        return a + b;
    }

    private void reset() {
        // private method
    }

    protected static String format(double value) {
        return String.valueOf(value);
    }

    public abstract void process();

    public final boolean validate(String input) {
        return input != null;
    }

    @Override
    public String toString() {
        return "Calculator";
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
        assert!(add_method.unwrap().signature.as_ref().unwrap().contains("public int add(int a, int b)"));
        assert_eq!(add_method.unwrap().visibility.as_ref().unwrap(), "public");

        let reset_method = symbols.iter().find(|s| s.name == "reset");
        assert!(reset_method.is_some());
        assert_eq!(reset_method.unwrap().visibility.as_ref().unwrap(), "private");

        let format_method = symbols.iter().find(|s| s.name == "format");
        assert!(format_method.is_some());
        assert!(format_method.unwrap().signature.as_ref().unwrap().contains("protected static String format"));
        assert_eq!(format_method.unwrap().visibility.as_ref().unwrap(), "protected");

        let process_method = symbols.iter().find(|s| s.name == "process");
        assert!(process_method.is_some());
        assert!(process_method.unwrap().signature.as_ref().unwrap().contains("abstract"));

        let validate_method = symbols.iter().find(|s| s.name == "validate");
        assert!(validate_method.is_some());
        assert!(validate_method.unwrap().signature.as_ref().unwrap().contains("final boolean validate"));

        let to_string_method = symbols.iter().find(|s| s.name == "toString");
        assert!(to_string_method.is_some());
        assert!(to_string_method.unwrap().signature.as_ref().unwrap().contains("@Override"));
    }

    #[test]
    fn test_extract_constructors() {
        let code = r#"
package com.example;

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

    private Person(boolean dummy) {
        // private constructor
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

        let constructors: Vec<&Symbol> = symbols.iter().filter(|s| s.kind == SymbolKind::Constructor).collect();
        assert_eq!(constructors.len(), 4);

        let default_constructor = constructors.iter().find(|s| s.signature.as_ref().unwrap().contains("Person()"));
        assert!(default_constructor.is_some());
        assert_eq!(default_constructor.unwrap().visibility.as_ref().unwrap(), "public");

        let name_constructor = constructors.iter().find(|s| s.signature.as_ref().unwrap().contains("Person(String name)"));
        assert!(name_constructor.is_some());

        let full_constructor = constructors.iter().find(|s| s.signature.as_ref().unwrap().contains("Person(String name, int age)"));
        assert!(full_constructor.is_some());

        let private_constructor = constructors.iter().find(|s| s.signature.as_ref().unwrap().contains("private") && s.signature.as_ref().unwrap().contains("boolean"));
        assert!(private_constructor.is_some());
        assert_eq!(private_constructor.unwrap().visibility.as_ref().unwrap(), "private");
    }

    // Field Extraction Tests
    #[test]
    fn test_extract_field_declarations_with_modifiers() {
        let code = r#"
package com.example;

public class Config {
    public static final String VERSION = "1.0.0";
    private String apiKey;
    protected int maxRetries = 3;
    boolean debugMode;
    public final long timestamp = System.currentTimeMillis();

    private static final Logger LOGGER = LoggerFactory.getLogger(Config.class);
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

        let version = symbols.iter().find(|s| s.name == "VERSION");
        assert!(version.is_some());
        assert_eq!(version.unwrap().kind, SymbolKind::Constant); // static final = constant
        assert!(version.unwrap().signature.as_ref().unwrap().contains("public static final String VERSION"));
        assert_eq!(version.unwrap().visibility.as_ref().unwrap(), "public");

        let api_key = symbols.iter().find(|s| s.name == "apiKey");
        assert!(api_key.is_some());
        assert_eq!(api_key.unwrap().kind, SymbolKind::Property);
        assert_eq!(api_key.unwrap().visibility.as_ref().unwrap(), "private");

        let max_retries = symbols.iter().find(|s| s.name == "maxRetries");
        assert!(max_retries.is_some());
        assert_eq!(max_retries.unwrap().visibility.as_ref().unwrap(), "protected");

        let debug_mode = symbols.iter().find(|s| s.name == "debugMode");
        assert!(debug_mode.is_some());
        assert_eq!(debug_mode.unwrap().visibility.as_ref().unwrap(), "package");

        let timestamp = symbols.iter().find(|s| s.name == "timestamp");
        assert!(timestamp.is_some());
        assert!(timestamp.unwrap().signature.as_ref().unwrap().contains("final"));

        let logger = symbols.iter().find(|s| s.name == "LOGGER");
        assert!(logger.is_some());
        assert_eq!(logger.unwrap().kind, SymbolKind::Constant);
        assert_eq!(logger.unwrap().visibility.as_ref().unwrap(), "private");
    }

    // Enum Extraction Tests
    #[test]
    fn test_extract_enum_definitions_and_values() {
        let code = r#"
package com.example;

public enum Color {
    RED, GREEN, BLUE
}

public enum Status {
    PENDING("pending"),
    ACTIVE("active"),
    INACTIVE("inactive");

    private final String value;

    Status(String value) {
        this.value = value;
    }

    public String getValue() {
        return value;
    }
}

enum Priority {
    LOW(1), MEDIUM(2), HIGH(3);

    private final int level;

    Priority(int level) {
        this.level = level;
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

        let color_enum = symbols.iter().find(|s| s.name == "Color");
        assert!(color_enum.is_some());
        assert_eq!(color_enum.unwrap().kind, SymbolKind::Enum);
        assert!(color_enum.unwrap().signature.as_ref().unwrap().contains("public enum Color"));
        assert_eq!(color_enum.unwrap().visibility.as_ref().unwrap(), "public");

        let red = symbols.iter().find(|s| s.name == "RED");
        assert!(red.is_some());
        assert_eq!(red.unwrap().kind, SymbolKind::EnumMember);

        let status_enum = symbols.iter().find(|s| s.name == "Status");
        assert!(status_enum.is_some());
        assert_eq!(status_enum.unwrap().kind, SymbolKind::Enum);

        let pending = symbols.iter().find(|s| s.name == "PENDING");
        assert!(pending.is_some());
        assert!(pending.unwrap().signature.as_ref().unwrap().contains("PENDING(\"pending\")"));

        let priority_enum = symbols.iter().find(|s| s.name == "Priority");
        assert!(priority_enum.is_some());
        assert_eq!(priority_enum.unwrap().visibility.as_ref().unwrap(), "package"); // no modifier = package
    }

    // Annotation Extraction Tests
    #[test]
    fn test_extract_annotation_definitions() {
        let code = r#"
package com.example;

import java.lang.annotation.*;

@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.METHOD)
public @interface Test {
    String value() default "";
    int timeout() default 0;
}

@interface Internal {
    // marker annotation
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

        let test_annotation = symbols.iter().find(|s| s.name == "Test");
        assert!(test_annotation.is_some());
        assert_eq!(test_annotation.unwrap().kind, SymbolKind::Interface); // annotations are special interfaces
        assert!(test_annotation.unwrap().signature.as_ref().unwrap().contains("@interface Test"));
        assert_eq!(test_annotation.unwrap().visibility.as_ref().unwrap(), "public");

        let internal_annotation = symbols.iter().find(|s| s.name == "Internal");
        assert!(internal_annotation.is_some());
        assert_eq!(internal_annotation.unwrap().visibility.as_ref().unwrap(), "package");
    }

    // Generic Types Tests
    #[test]
    fn test_extract_generic_class_and_method_definitions() {
        let code = r#"
package com.example;

public class Container<T> {
    private T value;

    public Container(T value) {
        this.value = value;
    }

    public T getValue() {
        return value;
    }

    public <U> U transform(Function<T, U> mapper) {
        return mapper.apply(value);
    }
}

public class Pair<K, V> extends Container<V> {
    private K key;

    public Pair(K key, V value) {
        super(value);
        this.key = key;
    }
}

public interface Repository<T, ID> {
    T findById(ID id);
    List<T> findAll();
    <S extends T> S save(S entity);
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

        let container = symbols.iter().find(|s| s.name == "Container");
        assert!(container.is_some());
        assert!(container.unwrap().signature.as_ref().unwrap().contains("Container<T>"));

        let transform = symbols.iter().find(|s| s.name == "transform");
        assert!(transform.is_some());
        assert!(transform.unwrap().signature.as_ref().unwrap().contains("<U>"));

        let pair = symbols.iter().find(|s| s.name == "Pair");
        assert!(pair.is_some());
        assert!(pair.unwrap().signature.as_ref().unwrap().contains("Pair<K, V>"));
        assert!(pair.unwrap().signature.as_ref().unwrap().contains("extends Container<V>"));

        let repository = symbols.iter().find(|s| s.name == "Repository");
        assert!(repository.is_some());
        assert!(repository.unwrap().signature.as_ref().unwrap().contains("Repository<T, ID>"));

        let save = symbols.iter().find(|s| s.name == "save");
        assert!(save.is_some());
        assert!(save.unwrap().signature.as_ref().unwrap().contains("<S extends T>"));
    }

    // Nested Classes Tests
    #[test]
    fn test_extract_nested_and_inner_classes() {
        let code = r#"
package com.example;

public class Outer {
    private String name;

    public static class StaticNested {
        public void doSomething() {}
    }

    public class Inner {
        public void accessOuter() {
            System.out.println(name);
        }
    }

    private class PrivateInner {
        private void helper() {}
    }

    public void localClassExample() {
        class LocalClass {
            void localMethod() {}
        }

        LocalClass local = new LocalClass();
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

        let outer = symbols.iter().find(|s| s.name == "Outer");
        assert!(outer.is_some());

        let static_nested = symbols.iter().find(|s| s.name == "StaticNested");
        assert!(static_nested.is_some());
        assert!(static_nested.unwrap().signature.as_ref().unwrap().contains("static class StaticNested"));
        assert_eq!(static_nested.unwrap().visibility.as_ref().unwrap(), "public");

        let inner = symbols.iter().find(|s| s.name == "Inner");
        assert!(inner.is_some());
        assert_eq!(inner.unwrap().visibility.as_ref().unwrap(), "public");

        let private_inner = symbols.iter().find(|s| s.name == "PrivateInner");
        assert!(private_inner.is_some());
        assert_eq!(private_inner.unwrap().visibility.as_ref().unwrap(), "private");

        // Local classes might be harder to extract, but let's test for them
        let local_class = symbols.iter().find(|s| s.name == "LocalClass");
        assert!(local_class.is_some());
    }

    // Modern Java Features Tests
    #[test]
    fn test_extract_lambda_expressions_and_method_references() {
        let code = r#"
package com.example;

import java.util.*;
import java.util.function.*;
import java.util.stream.*;

public class ModernJavaFeatures {

    // Lambda expressions
    private final Comparator<String> comparator = (s1, s2) -> s1.compareToIgnoreCase(s2);
    private final Function<String, Integer> stringLength = s -> s.length();
    private final BiFunction<Integer, Integer, Integer> sum = (a, b) -> a + b;
    private final Runnable task = () -> System.out.println("Task executed");

    // Method references
    private final Function<String, String> toUpperCase = String::toUpperCase;
    private final Supplier<List<String>> listSupplier = ArrayList::new;
    private final Consumer<String> printer = System.out::println;
    private final BinaryOperator<Integer> max = Integer::max;

    public void streamOperations() {
        List<String> names = Arrays.asList("John", "Jane", "Bob", "Alice");

        // Stream with lambda expressions
        List<String> upperCaseNames = names.stream()
            .filter(name -> name.length() > 3)
            .map(String::toUpperCase)
            .sorted((s1, s2) -> s1.compareTo(s2))
            .collect(Collectors.toList());

        // Parallel stream processing
        Optional<String> longest = names.parallelStream()
            .max(Comparator.comparing(String::length));

        // Complex stream operations
        Map<Integer, List<String>> groupedByLength = names.stream()
            .collect(Collectors.groupingBy(String::length));

        // Stream with reduce operations
        int totalLength = names.stream()
            .mapToInt(String::length)
            .reduce(0, Integer::sum);
    }

    // Optional usage patterns
    public Optional<User> findUser(String name) {
        return Optional.ofNullable(getUserByName(name))
            .filter(user -> user.isActive())
            .map(this::enrichUser);
    }

    public String getUserDisplayName(String userId) {
        return findUser(userId)
            .map(User::getName)
            .map(String::toUpperCase)
            .orElse("Unknown User");
    }

    // Functional interface usage
    @FunctionalInterface
    public interface UserProcessor {
        User process(User user);

        default User processWithLogging(User user) {
            System.out.println("Processing user: " + user.getName());
            return process(user);
        }
    }

    // Higher-order functions
    public <T, R> List<R> transformList(List<T> list, Function<T, R> transformer) {
        return list.stream()
            .map(transformer)
            .collect(Collectors.toList());
    }

    public <T> Optional<T> findFirst(List<T> list, Predicate<T> predicate) {
        return list.stream()
            .filter(predicate)
            .findFirst();
    }

    // CompletableFuture patterns
    public CompletableFuture<String> asyncOperation() {
        return CompletableFuture
            .supplyAsync(() -> fetchDataFromService())
            .thenApply(String::toUpperCase)
            .thenCompose(this::validateData)
            .exceptionally(throwable -> {
                log.error("Error in async operation", throwable);
                return "Default Value";
            });
    }

    public CompletableFuture<Void> combinedAsyncOperations() {
        CompletableFuture<String> future1 = CompletableFuture.supplyAsync(() -> "Data1");
        CompletableFuture<String> future2 = CompletableFuture.supplyAsync(() -> "Data2");

        return CompletableFuture.allOf(future1, future2)
            .thenRun(() -> {
                String result1 = future1.join();
                String result2 = future2.join();
                processResults(result1, result2);
            });
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

        let modern_features = symbols.iter().find(|s| s.name == "ModernJavaFeatures");
        assert!(modern_features.is_some());
        assert_eq!(modern_features.unwrap().kind, SymbolKind::Class);

        // Lambda expression fields
        let comparator = symbols.iter().find(|s| s.name == "comparator");
        assert!(comparator.is_some());
        assert!(comparator.unwrap().signature.as_ref().unwrap().contains("Comparator<String>"));

        let string_length = symbols.iter().find(|s| s.name == "stringLength");
        assert!(string_length.is_some());
        assert!(string_length.unwrap().signature.as_ref().unwrap().contains("Function<String, Integer>"));

        // Method reference fields
        let to_upper_case = symbols.iter().find(|s| s.name == "toUpperCase");
        assert!(to_upper_case.is_some());
        assert!(to_upper_case.unwrap().signature.as_ref().unwrap().contains("Function<String, String>"));

        let printer = symbols.iter().find(|s| s.name == "printer");
        assert!(printer.is_some());
        assert!(printer.unwrap().signature.as_ref().unwrap().contains("Consumer<String>"));

        // Stream operations method
        let stream_operations = symbols.iter().find(|s| s.name == "streamOperations");
        assert!(stream_operations.is_some());
        assert_eq!(stream_operations.unwrap().kind, SymbolKind::Method);

        // Optional methods
        let find_user = symbols.iter().find(|s| s.name == "findUser");
        assert!(find_user.is_some());
        assert!(find_user.unwrap().signature.as_ref().unwrap().contains("Optional<User>"));

        let get_user_display_name = symbols.iter().find(|s| s.name == "getUserDisplayName");
        assert!(get_user_display_name.is_some());
        assert!(get_user_display_name.unwrap().signature.as_ref().unwrap().contains("String getUserDisplayName"));

        // Functional interface
        let user_processor = symbols.iter().find(|s| s.name == "UserProcessor");
        assert!(user_processor.is_some());
        assert_eq!(user_processor.unwrap().kind, SymbolKind::Interface);
        assert!(user_processor.unwrap().signature.as_ref().unwrap().contains("@FunctionalInterface"));

        // Generic methods
        let transform_list = symbols.iter().find(|s| s.name == "transformList");
        assert!(transform_list.is_some());
        assert!(transform_list.unwrap().signature.as_ref().unwrap().contains("<T, R>"));

        let async_operation = symbols.iter().find(|s| s.name == "asyncOperation");
        assert!(async_operation.is_some());
        assert!(async_operation.unwrap().signature.as_ref().unwrap().contains("CompletableFuture<String>"));
    }

    // Advanced Language Features Tests
    #[test]
    fn test_extract_records_sealed_classes_and_pattern_matching() {
        let code = r#"
package com.example;

import java.util.*;

// Record types (Java 14+)
public record Person(String name, int age, String email) {
    // Compact constructor
    public Person {
        if (age < 0) {
            throw new IllegalArgumentException("Age cannot be negative");
        }
        if (name == null || name.isBlank()) {
            throw new IllegalArgumentException("Name cannot be null or blank");
        }
    }

    // Custom constructor
    public Person(String name, int age) {
        this(name, age, name.toLowerCase() + "@example.com");
    }

    // Instance methods in records
    public boolean isAdult() {
        return age >= 18;
    }

    public String getDisplayName() {
        return name + " (" + age + ")";
    }
}

// Sealed classes (Java 17+)
public sealed class Shape
    permits Circle, Rectangle, Triangle {

    public abstract double area();
    public abstract double perimeter();
}

public final class Circle extends Shape {
    private final double radius;

    public Circle(double radius) {
        this.radius = radius;
    }

    @Override
    public double area() {
        return Math.PI * radius * radius;
    }

    @Override
    public double perimeter() {
        return 2 * Math.PI * radius;
    }

    public double radius() {
        return radius;
    }
}

public final class Rectangle extends Shape {
    private final double width;
    private final double height;

    public Rectangle(double width, double height) {
        this.width = width;
        this.height = height;
    }

    @Override
    public double area() {
        return width * height;
    }

    @Override
    public double perimeter() {
        return 2 * (width + height);
    }

    public double width() { return width; }
    public double height() { return height; }
}

public non-sealed class Triangle extends Shape {
    private final double a, b, c;

    public Triangle(double a, double b, double c) {
        this.a = a;
        this.b = b;
        this.c = c;
    }

    @Override
    public double area() {
        double s = (a + b + c) / 2;
        return Math.sqrt(s * (s - a) * (s - b) * (s - c));
    }

    @Override
    public double perimeter() {
        return a + b + c;
    }
}

// Pattern matching and switch expressions
public class PatternMatching {

    // Switch expressions (Java 14+)
    public String describeShape(Shape shape) {
        return switch (shape) {
            case Circle c -> "Circle with radius " + c.radius();
            case Rectangle r -> "Rectangle " + r.width() + "x" + r.height();
            case Triangle t -> "Triangle with perimeter " + t.perimeter();
        };
    }

    // Pattern matching with instanceof (Java 16+)
    public double calculateShapeArea(Object obj) {
        if (obj instanceof Circle c) {
            return c.area();
        } else if (obj instanceof Rectangle r) {
            return r.area();
        } else if (obj instanceof Triangle t) {
            return t.area();
        } else {
            throw new IllegalArgumentException("Unknown shape type");
        }
    }

    // Text blocks (Java 13+)
    public String getJsonTemplate() {
        return """
            {
                "name": "%s",
                "age": %d,
                "email": "%s",
                "active": %b
            }
            """;
    }

    public String getSqlQuery() {
        return """
            SELECT p.name, p.age, p.email
            FROM person p
            WHERE p.age >= 18
              AND p.active = true
            ORDER BY p.name
            """;
    }

    // Switch with multiple cases
    public String categorizeAge(int age) {
        return switch (age) {
            case 0, 1, 2 -> "Baby";
            case 3, 4, 5 -> "Toddler";
            case 6, 7, 8, 9, 10, 11, 12 -> "Child";
            case 13, 14, 15, 16, 17 -> "Teenager";
            default -> {
                if (age >= 18 && age < 65) {
                    yield "Adult";
                } else if (age >= 65) {
                    yield "Senior";
                } else {
                    yield "Invalid age";
                }
            }
        };
    }

    // Enhanced instanceof with pattern variables
    public void processObject(Object obj) {
        if (obj instanceof String str && str.length() > 5) {
            System.out.println("Long string: " + str.toUpperCase());
        } else if (obj instanceof Integer num && num > 0) {
            System.out.println("Positive number: " + num);
        } else if (obj instanceof List<?> list && !list.isEmpty()) {
            System.out.println("Non-empty list with " + list.size() + " elements");
        }
    }
}

// Record with generics
public record Result<T, E>(T value, E error, boolean isSuccess) {

    public static <T, E> Result<T, E> success(T value) {
        return new Result<>(value, null, true);
    }

    public static <T, E> Result<T, E> failure(E error) {
        return new Result<>(null, error, false);
    }

    public Optional<T> getValue() {
        return isSuccess ? Optional.of(value) : Optional.empty();
    }

    public Optional<E> getError() {
        return !isSuccess ? Optional.of(error) : Optional.empty();
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

        // Record type
        let person = symbols.iter().find(|s| s.name == "Person");
        assert!(person.is_some());
        assert_eq!(person.unwrap().kind, SymbolKind::Class);
        assert!(person.unwrap().signature.as_ref().unwrap().contains("record Person"));

        let person_compact_constructor = symbols.iter().find(|s| s.name == "Person" && s.kind == SymbolKind::Constructor);
        assert!(person_compact_constructor.is_some());

        let is_adult = symbols.iter().find(|s| s.name == "isAdult");
        assert!(is_adult.is_some());
        assert!(is_adult.unwrap().signature.as_ref().unwrap().contains("boolean isAdult()"));

        // Sealed class
        let shape = symbols.iter().find(|s| s.name == "Shape");
        assert!(shape.is_some());
        assert!(shape.unwrap().signature.as_ref().unwrap().contains("sealed class Shape"));
        assert!(shape.unwrap().signature.as_ref().unwrap().contains("permits Circle, Rectangle, Triangle"));

        let circle = symbols.iter().find(|s| s.name == "Circle");
        assert!(circle.is_some());
        assert!(circle.unwrap().signature.as_ref().unwrap().contains("final class Circle extends Shape"));

        let rectangle = symbols.iter().find(|s| s.name == "Rectangle");
        assert!(rectangle.is_some());
        assert!(rectangle.unwrap().signature.as_ref().unwrap().contains("final class Rectangle extends Shape"));

        let triangle = symbols.iter().find(|s| s.name == "Triangle");
        assert!(triangle.is_some());
        assert!(triangle.unwrap().signature.as_ref().unwrap().contains("non-sealed class Triangle extends Shape"));

        // Pattern matching class
        let pattern_matching = symbols.iter().find(|s| s.name == "PatternMatching");
        assert!(pattern_matching.is_some());

        let describe_shape = symbols.iter().find(|s| s.name == "describeShape");
        assert!(describe_shape.is_some());
        assert!(describe_shape.unwrap().signature.as_ref().unwrap().contains("String describeShape(Shape shape)"));

        let calculate_shape_area = symbols.iter().find(|s| s.name == "calculateShapeArea");
        assert!(calculate_shape_area.is_some());
        assert!(calculate_shape_area.unwrap().signature.as_ref().unwrap().contains("double calculateShapeArea(Object obj)"));

        let get_json_template = symbols.iter().find(|s| s.name == "getJsonTemplate");
        assert!(get_json_template.is_some());
        assert!(get_json_template.unwrap().signature.as_ref().unwrap().contains("String getJsonTemplate()"));

        let categorize_age = symbols.iter().find(|s| s.name == "categorizeAge");
        assert!(categorize_age.is_some());
        assert!(categorize_age.unwrap().signature.as_ref().unwrap().contains("String categorizeAge(int age)"));

        // Generic record
        let result_record = symbols.iter().find(|s| s.name == "Result");
        assert!(result_record.is_some());
        assert!(result_record.unwrap().signature.as_ref().unwrap().contains("record Result<T, E>"));

        let success_method = symbols.iter().find(|s| s.name == "success");
        assert!(success_method.is_some());
        assert!(success_method.unwrap().signature.as_ref().unwrap().contains("static <T, E> Result<T, E> success"));
    }

    // Exception Handling and Resource Management Tests
    #[test]
    fn test_extract_exception_classes_and_try_with_resources() {
        let code = r#"
package com.example;

import java.io.*;
import java.util.*;
import java.sql.*;

// Custom exception hierarchy
public class BusinessException extends Exception {
    private final String errorCode;
    private final Map<String, Object> context;

    public BusinessException(String message, String errorCode) {
        super(message);
        this.errorCode = errorCode;
        this.context = new HashMap<>();
    }

    public BusinessException(String message, String errorCode, Throwable cause) {
        super(message, cause);
        this.errorCode = errorCode;
        this.context = new HashMap<>();
    }

    public BusinessException addContext(String key, Object value) {
        this.context.put(key, value);
        return this;
    }

    public String getErrorCode() {
        return errorCode;
    }

    public Map<String, Object> getContext() {
        return Collections.unmodifiableMap(context);
    }
}

public class ValidationException extends BusinessException {
    private final List<String> violations;

    public ValidationException(String message, List<String> violations) {
        super(message, "VALIDATION_ERROR");
        this.violations = new ArrayList<>(violations);
    }

    public List<String> getViolations() {
        return Collections.unmodifiableList(violations);
    }
}

public class DataAccessException extends BusinessException {
    public DataAccessException(String message, Throwable cause) {
        super(message, "DATA_ACCESS_ERROR", cause);
    }
}

// Resource management with try-with-resources
public class ResourceManager {

    // Single resource
    public String readFile(String fileName) throws IOException {
        try (BufferedReader reader = Files.newBufferedReader(Paths.get(fileName))) {
            return reader.lines()
                .collect(Collectors.joining("\n"));
        }
    }

    // Multiple resources
    public void copyFile(String source, String destination) throws IOException {
        try (InputStream input = Files.newInputStream(Paths.get(source));
             OutputStream output = Files.newOutputStream(Paths.get(destination))) {

            byte[] buffer = new byte[8192];
            int bytesRead;
            while ((bytesRead = input.read(buffer)) != -1) {
                output.write(buffer, 0, bytesRead);
            }
        }
    }

    // Database operations with try-with-resources
    public List<User> getUsersFromDatabase(String connectionUrl) throws DataAccessException {
        try (Connection connection = DriverManager.getConnection(connectionUrl);
             PreparedStatement statement = connection.prepareStatement(
                 "SELECT id, name, email FROM users WHERE active = ?");
        ) {
            statement.setBoolean(1, true);

            try (ResultSet resultSet = statement.executeQuery()) {
                List<User> users = new ArrayList<>();
                while (resultSet.next()) {
                    User user = new User(
                        resultSet.getLong("id"),
                        resultSet.getString("name"),
                        resultSet.getString("email")
                    );
                    users.add(user);
                }
                return users;
            }
        } catch (SQLException e) {
            throw new DataAccessException("Failed to fetch users from database", e);
        }
    }

    // Multi-catch exception handling
    public void processData(String data) throws BusinessException {
        try {
            validateInput(data);
            parseData(data);
            persistData(data);
        } catch (IllegalArgumentException | NumberFormatException e) {
            throw new ValidationException("Invalid data format",
                Arrays.asList(e.getMessage()));
        } catch (IOException | SQLException e) {
            throw new DataAccessException("Failed to process data", e);
        } catch (Exception e) {
            throw new BusinessException("Unexpected error during data processing",
                "PROCESSING_ERROR", e);
        }
    }

    // Exception handling with suppressed exceptions
    public void closeResources(AutoCloseable... resources) {
        Exception primaryException = null;

        for (AutoCloseable resource : resources) {
            try {
                if (resource != null) {
                    resource.close();
                }
            } catch (Exception e) {
                if (primaryException == null) {
                    primaryException = e;
                } else {
                    primaryException.addSuppressed(e);
                }
            }
        }

        if (primaryException != null) {
            if (primaryException instanceof RuntimeException) {
                throw (RuntimeException) primaryException;
            } else {
                throw new RuntimeException(primaryException);
            }
        }
    }

    // Exception chaining and wrapping
    public void chainedExceptionExample() throws BusinessException {
        try {
            riskyOperation();
        } catch (IOException e) {
            DataAccessException dae = new DataAccessException("Database operation failed", e);
            dae.addContext("operation", "chainedExceptionExample");
            dae.addContext("timestamp", Instant.now());
            throw dae;
        }
    }

    // Custom resource implementation
    public static class ManagedResource implements AutoCloseable {
        private final String resourceName;
        private boolean closed = false;

        public ManagedResource(String resourceName) {
            this.resourceName = resourceName;
            System.out.println("Opening resource: " + resourceName);
        }

        public void doWork() throws IOException {
            if (closed) {
                throw new IllegalStateException("Resource is closed");
            }
            // Simulate work
            if (Math.random() < 0.1) {
                throw new IOException("Random failure in " + resourceName);
            }
        }

        @Override
        public void close() throws IOException {
            if (!closed) {
                System.out.println("Closing resource: " + resourceName);
                closed = true;
                // Simulate close failure
                if (Math.random() < 0.05) {
                    throw new IOException("Failed to close " + resourceName);
                }
            }
        }
    }

    // Complex try-with-resources with custom resource
    public void complexResourceManagement() throws BusinessException {
        try (ManagedResource resource1 = new ManagedResource("Database");
             ManagedResource resource2 = new ManagedResource("FileSystem");
             ManagedResource resource3 = new ManagedResource("Network")) {

            resource1.doWork();
            resource2.doWork();
            resource3.doWork();

        } catch (IOException e) {
            throw new BusinessException("Resource operation failed",
                "RESOURCE_ERROR", e)
                .addContext("resources", Arrays.asList("Database", "FileSystem", "Network"));
        }
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

        // Custom exceptions
        let business_exception = symbols.iter().find(|s| s.name == "BusinessException");
        assert!(business_exception.is_some());
        assert_eq!(business_exception.unwrap().kind, SymbolKind::Class);
        assert!(business_exception.unwrap().signature.as_ref().unwrap().contains("class BusinessException extends Exception"));

        let validation_exception = symbols.iter().find(|s| s.name == "ValidationException");
        assert!(validation_exception.is_some());
        assert!(validation_exception.unwrap().signature.as_ref().unwrap().contains("class ValidationException extends BusinessException"));

        let data_access_exception = symbols.iter().find(|s| s.name == "DataAccessException");
        assert!(data_access_exception.is_some());
        assert!(data_access_exception.unwrap().signature.as_ref().unwrap().contains("class DataAccessException extends BusinessException"));

        // Exception constructors
        let business_exception_constructor = symbols.iter().find(|s|
            s.name == "BusinessException" &&
            s.kind == SymbolKind::Constructor &&
            s.signature.as_ref().unwrap().contains("String message, String errorCode, Throwable cause")
        );
        assert!(business_exception_constructor.is_some());

        // Exception methods
        let add_context = symbols.iter().find(|s| s.name == "addContext");
        assert!(add_context.is_some());
        assert!(add_context.unwrap().signature.as_ref().unwrap().contains("BusinessException addContext(String key, Object value)"));

        let get_error_code = symbols.iter().find(|s| s.name == "getErrorCode");
        assert!(get_error_code.is_some());
        assert!(get_error_code.unwrap().signature.as_ref().unwrap().contains("String getErrorCode()"));

        // Resource manager
        let resource_manager = symbols.iter().find(|s| s.name == "ResourceManager");
        assert!(resource_manager.is_some());
        assert_eq!(resource_manager.unwrap().kind, SymbolKind::Class);

        let read_file = symbols.iter().find(|s| s.name == "readFile");
        assert!(read_file.is_some());
        assert!(read_file.unwrap().signature.as_ref().unwrap().contains("String readFile(String fileName) throws IOException"));

        let copy_file = symbols.iter().find(|s| s.name == "copyFile");
        assert!(copy_file.is_some());
        assert!(copy_file.unwrap().signature.as_ref().unwrap().contains("void copyFile(String source, String destination) throws IOException"));

        let get_users_from_database = symbols.iter().find(|s| s.name == "getUsersFromDatabase");
        assert!(get_users_from_database.is_some());
        assert!(get_users_from_database.unwrap().signature.as_ref().unwrap().contains("List<User> getUsersFromDatabase(String connectionUrl) throws DataAccessException"));

        let process_data = symbols.iter().find(|s| s.name == "processData");
        assert!(process_data.is_some());
        assert!(process_data.unwrap().signature.as_ref().unwrap().contains("void processData(String data) throws BusinessException"));

        let close_resources = symbols.iter().find(|s| s.name == "closeResources");
        assert!(close_resources.is_some());
        assert!(close_resources.unwrap().signature.as_ref().unwrap().contains("void closeResources(AutoCloseable... resources)"));

        // Custom resource
        let managed_resource = symbols.iter().find(|s| s.name == "ManagedResource");
        assert!(managed_resource.is_some());
        assert!(managed_resource.unwrap().signature.as_ref().unwrap().contains("static class ManagedResource implements AutoCloseable"));

        let do_work = symbols.iter().find(|s| s.name == "doWork");
        assert!(do_work.is_some());
        assert!(do_work.unwrap().signature.as_ref().unwrap().contains("void doWork() throws IOException"));

        let close = symbols.iter().find(|s| s.name == "close");
        assert!(close.is_some());
        assert!(close.unwrap().signature.as_ref().unwrap().contains("@Override"));
        assert!(close.unwrap().signature.as_ref().unwrap().contains("void close() throws IOException"));
    }

    // Testing Patterns and Annotations Tests
    #[test]
    fn test_extract_junit_and_testing_framework_patterns() {
        let code = r#"
package com.example.test;

import org.junit.jupiter.api.*;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.*;
import org.mockito.*;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.test.context.TestPropertySource;

import static org.junit.jupiter.api.Assertions.*;
import static org.mockito.Mockito.*;

@SpringBootTest
@TestPropertySource(locations = "classpath:test.properties")
@TestMethodOrder(OrderAnnotation.class)
public class UserServiceTest {

    @Mock
    private UserRepository userRepository;

    @InjectMocks
    private UserService userService;

    @Captor
    private ArgumentCaptor<User> userCaptor;

    private static TestDataBuilder testDataBuilder;

    @BeforeAll
    static void setUpClass() {
        testDataBuilder = new TestDataBuilder();
        System.out.println("Setting up test class");
    }

    @AfterAll
    static void tearDownClass() {
        testDataBuilder = null;
        System.out.println("Tearing down test class");
    }

    @BeforeEach
    void setUp() {
        MockitoAnnotations.openMocks(this);
        System.out.println("Setting up test method");
    }

    @AfterEach
    void tearDown() {
        reset(userRepository);
        System.out.println("Tearing down test method");
    }

    @Test
    @DisplayName("Should create user successfully")
    @Order(1)
    void shouldCreateUserSuccessfully() {
        // Given
        User newUser = testDataBuilder.createUser("John Doe", "john@example.com");
        when(userRepository.save(any(User.class))).thenReturn(newUser);

        // When
        User createdUser = userService.createUser(newUser);

        // Then
        assertNotNull(createdUser);
        assertEquals("John Doe", createdUser.getName());
        assertEquals("john@example.com", createdUser.getEmail());

        verify(userRepository).save(userCaptor.capture());
        User capturedUser = userCaptor.getValue();
        assertEquals(newUser.getName(), capturedUser.getName());
    }

    @Test
    @DisplayName("Should throw exception when user is null")
    @Order(2)
    void shouldThrowExceptionWhenUserIsNull() {
        // When & Then
        assertThrows(IllegalArgumentException.class, () -> {
            userService.createUser(null);
        });

        verifyNoInteractions(userRepository);
    }

    @ParameterizedTest
    @DisplayName("Should validate email format")
    @ValueSource(strings = {"invalid", "@invalid.com", "invalid@", ""})
    void shouldValidateEmailFormat(String invalidEmail) {
        // Given
        User user = testDataBuilder.createUser("John Doe", invalidEmail);

        // When & Then
        assertThrows(ValidationException.class, () -> {
            userService.createUser(user);
        });
    }

    @ParameterizedTest
    @DisplayName("Should accept valid email formats")
    @ValueSource(strings = {
        "test@example.com",
        "user.name@domain.co.uk",
        "first.last+tag@example.org"
    })
    void shouldAcceptValidEmailFormats(String validEmail) {
        // Given
        User user = testDataBuilder.createUser("John Doe", validEmail);
        when(userRepository.save(any(User.class))).thenReturn(user);

        // When & Then
        assertDoesNotThrow(() -> {
            userService.createUser(user);
        });
    }

    @ParameterizedTest
    @DisplayName("Should handle different user scenarios")
    @MethodSource("userTestCases")
    void shouldHandleDifferentUserScenarios(UserTestCase testCase) {
        // Given
        when(userRepository.save(any(User.class))).thenReturn(testCase.expectedUser());

        // When
        User result = userService.createUser(testCase.inputUser());

        // Then
        assertEquals(testCase.expectedUser().getName(), result.getName());
        assertEquals(testCase.expectedUser().getEmail(), result.getEmail());
    }

    static Stream<UserTestCase> userTestCases() {
        return Stream.of(
            new UserTestCase(
                testDataBuilder.createUser("Alice", "alice@example.com"),
                testDataBuilder.createUser("Alice", "alice@example.com")
            ),
            new UserTestCase(
                testDataBuilder.createUser("Bob", "bob@test.org"),
                testDataBuilder.createUser("Bob", "bob@test.org")
            )
        );
    }

    @Test
    @DisplayName("Should find user by id")
    @Timeout(value = 2, unit = TimeUnit.SECONDS)
    void shouldFindUserById() {
        // Given
        Long userId = 1L;
        User expectedUser = testDataBuilder.createUser("John", "john@example.com");
        when(userRepository.findById(userId)).thenReturn(Optional.of(expectedUser));

        // When
        Optional<User> result = userService.findById(userId);

        // Then
        assertTrue(result.isPresent());
        assertEquals(expectedUser.getName(), result.get().getName());
    }

    @RepeatedTest(value = 5, name = "Execution {currentRepetition} of {totalRepetitions}")
    @DisplayName("Should handle concurrent user creation")
    void shouldHandleConcurrentUserCreation(RepetitionInfo repetitionInfo) {
        // Given
        String userName = "User" + repetitionInfo.getCurrentRepetition();
        User user = testDataBuilder.createUser(userName, userName.toLowerCase() + "@example.com");
        when(userRepository.save(any(User.class))).thenReturn(user);

        // When
        User result = userService.createUser(user);

        // Then
        assertNotNull(result);
        assertEquals(userName, result.getName());
    }

    @Test
    @DisplayName("Should handle database exceptions gracefully")
    @Tag("integration")
    void shouldHandleDatabaseExceptionsGracefully() {
        // Given
        User user = testDataBuilder.createUser("John", "john@example.com");
        when(userRepository.save(any(User.class)))
            .thenThrow(new DataAccessException("Database connection failed"));

        // When & Then
        assertThrows(ServiceException.class, () -> {
            userService.createUser(user);
        });
    }

    @Nested
    @DisplayName("User validation tests")
    class UserValidationTests {

        @Test
        @DisplayName("Should validate required fields")
        void shouldValidateRequiredFields() {
            // Test implementation
            assertAll(
                () -> assertThrows(ValidationException.class,
                    () -> userService.createUser(new User(null, "test@example.com"))),
                () -> assertThrows(ValidationException.class,
                    () -> userService.createUser(new User("John", null))),
                () -> assertThrows(ValidationException.class,
                    () -> userService.createUser(new User("", "test@example.com")))
            );
        }

        @Test
        @DisplayName("Should validate business rules")
        void shouldValidateBusinessRules() {
            // Test implementation for business rule validation
            User user = testDataBuilder.createUser("ValidUser", "valid@example.com");

            assertDoesNotThrow(() -> {
                userService.validateBusinessRules(user);
            });
        }
    }

    // Helper classes and records
    public record UserTestCase(User inputUser, User expectedUser) {}

    @TestConfiguration
    static class TestConfig {

        @Bean
        @Primary
        public Clock testClock() {
            return Clock.fixed(Instant.parse("2023-01-01T00:00:00Z"), ZoneOffset.UTC);
        }
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

        // Test class
        let user_service_test = symbols.iter().find(|s| s.name == "UserServiceTest");
        assert!(user_service_test.is_some());
        assert_eq!(user_service_test.unwrap().kind, SymbolKind::Class);
        assert!(user_service_test.unwrap().signature.as_ref().unwrap().contains("@SpringBootTest"));
        assert!(user_service_test.unwrap().signature.as_ref().unwrap().contains("@TestPropertySource"));
        assert!(user_service_test.unwrap().signature.as_ref().unwrap().contains("@TestMethodOrder"));

        // Mock fields
        let user_repository = symbols.iter().find(|s| s.name == "userRepository");
        assert!(user_repository.is_some());
        assert!(user_repository.unwrap().signature.as_ref().unwrap().contains("@Mock"));

        let user_service = symbols.iter().find(|s| s.name == "userService");
        assert!(user_service.is_some());
        assert!(user_service.unwrap().signature.as_ref().unwrap().contains("@InjectMocks"));

        let user_captor = symbols.iter().find(|s| s.name == "userCaptor");
        assert!(user_captor.is_some());
        assert!(user_captor.unwrap().signature.as_ref().unwrap().contains("@Captor"));

        // Lifecycle methods
        let set_up_class = symbols.iter().find(|s| s.name == "setUpClass");
        assert!(set_up_class.is_some());
        assert!(set_up_class.unwrap().signature.as_ref().unwrap().contains("@BeforeAll"));
        assert!(set_up_class.unwrap().signature.as_ref().unwrap().contains("static void setUpClass()"));

        let tear_down_class = symbols.iter().find(|s| s.name == "tearDownClass");
        assert!(tear_down_class.is_some());
        assert!(tear_down_class.unwrap().signature.as_ref().unwrap().contains("@AfterAll"));

        let set_up = symbols.iter().find(|s| s.name == "setUp");
        assert!(set_up.is_some());
        assert!(set_up.unwrap().signature.as_ref().unwrap().contains("@BeforeEach"));

        let tear_down = symbols.iter().find(|s| s.name == "tearDown");
        assert!(tear_down.is_some());
        assert!(tear_down.unwrap().signature.as_ref().unwrap().contains("@AfterEach"));

        // Test methods
        let should_create_user_successfully = symbols.iter().find(|s| s.name == "shouldCreateUserSuccessfully");
        assert!(should_create_user_successfully.is_some());
        assert!(should_create_user_successfully.unwrap().signature.as_ref().unwrap().contains("@Test"));
        assert!(should_create_user_successfully.unwrap().signature.as_ref().unwrap().contains("@DisplayName(\"Should create user successfully\")"));
        assert!(should_create_user_successfully.unwrap().signature.as_ref().unwrap().contains("@Order(1)"));

        let should_throw_exception_when_user_is_null = symbols.iter().find(|s| s.name == "shouldThrowExceptionWhenUserIsNull");
        assert!(should_throw_exception_when_user_is_null.is_some());
        assert!(should_throw_exception_when_user_is_null.unwrap().signature.as_ref().unwrap().contains("@Test"));

        // Parameterized tests
        let should_validate_email_format = symbols.iter().find(|s| s.name == "shouldValidateEmailFormat");
        assert!(should_validate_email_format.is_some());
        assert!(should_validate_email_format.unwrap().signature.as_ref().unwrap().contains("@ParameterizedTest"));
        assert!(should_validate_email_format.unwrap().signature.as_ref().unwrap().contains("@ValueSource"));

        let should_accept_valid_email_formats = symbols.iter().find(|s| s.name == "shouldAcceptValidEmailFormats");
        assert!(should_accept_valid_email_formats.is_some());
        assert!(should_accept_valid_email_formats.unwrap().signature.as_ref().unwrap().contains("@ParameterizedTest"));

        let should_handle_different_user_scenarios = symbols.iter().find(|s| s.name == "shouldHandleDifferentUserScenarios");
        assert!(should_handle_different_user_scenarios.is_some());
        assert!(should_handle_different_user_scenarios.unwrap().signature.as_ref().unwrap().contains("@MethodSource(\"userTestCases\")"));

        // Test data methods
        let user_test_cases = symbols.iter().find(|s| s.name == "userTestCases");
        assert!(user_test_cases.is_some());
        assert!(user_test_cases.unwrap().signature.as_ref().unwrap().contains("static Stream<UserTestCase> userTestCases()"));

        // Timeout test
        let should_find_user_by_id = symbols.iter().find(|s| s.name == "shouldFindUserById");
        assert!(should_find_user_by_id.is_some());
        assert!(should_find_user_by_id.unwrap().signature.as_ref().unwrap().contains("@Timeout"));

        // Repeated test
        let should_handle_concurrent_user_creation = symbols.iter().find(|s| s.name == "shouldHandleConcurrentUserCreation");
        assert!(should_handle_concurrent_user_creation.is_some());
        assert!(should_handle_concurrent_user_creation.unwrap().signature.as_ref().unwrap().contains("@RepeatedTest"));

        // Tagged test
        let should_handle_database_exceptions_gracefully = symbols.iter().find(|s| s.name == "shouldHandleDatabaseExceptionsGracefully");
        assert!(should_handle_database_exceptions_gracefully.is_some());
        assert!(should_handle_database_exceptions_gracefully.unwrap().signature.as_ref().unwrap().contains("@Tag(\"integration\")"));

        // Nested test class
        let user_validation_tests = symbols.iter().find(|s| s.name == "UserValidationTests");
        assert!(user_validation_tests.is_some());
        assert!(user_validation_tests.unwrap().signature.as_ref().unwrap().contains("@Nested"));
        assert!(user_validation_tests.unwrap().signature.as_ref().unwrap().contains("@DisplayName(\"User validation tests\")"));

        // Record for test data
        let user_test_case = symbols.iter().find(|s| s.name == "UserTestCase");
        assert!(user_test_case.is_some());
        assert!(user_test_case.unwrap().signature.as_ref().unwrap().contains("record UserTestCase"));

        // Test configuration
        let test_config = symbols.iter().find(|s| s.name == "TestConfig");
        assert!(test_config.is_some());
        assert!(test_config.unwrap().signature.as_ref().unwrap().contains("@TestConfiguration"));
        assert!(test_config.unwrap().signature.as_ref().unwrap().contains("static class TestConfig"));
    }

    // Java-specific Features Tests
    #[test]
    fn test_handle_comprehensive_java_code() {
        let code = r#"
package com.example.service;

import java.util.*;
import java.util.concurrent.CompletableFuture;
import static java.util.stream.Collectors.*;

/**
 * User service for managing user operations
 */
@Service
@Transactional
public class UserService implements CrudService<User, Long> {

    @Autowired
    private UserRepository repository;

    @Value("${app.default.timeout}")
    private int timeout;

    public static final String DEFAULT_ROLE = "USER";

    @Override
    public User findById(Long id) {
        return repository.findById(id)
            .orElseThrow(() -> new UserNotFoundException("User not found: " + id));
    }

    @Async
    public CompletableFuture<List<User>> findActiveUsers() {
        return CompletableFuture.supplyAsync(() ->
            repository.findAll().stream()
                .filter(User::isActive)
                .collect(toList())
        );
    }

    @PreAuthorize("hasRole('ADMIN')")
    public void deleteUser(Long id) {
        repository.deleteById(id);
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

        // Check we extracted all major symbols
        assert!(symbols.iter().any(|s| s.name == "com.example.service"));
        assert!(symbols.iter().any(|s| s.name == "UserService"));
        assert!(symbols.iter().any(|s| s.name == "repository"));
        assert!(symbols.iter().any(|s| s.name == "timeout"));
        assert!(symbols.iter().any(|s| s.name == "DEFAULT_ROLE"));
        assert!(symbols.iter().any(|s| s.name == "findById"));
        assert!(symbols.iter().any(|s| s.name == "findActiveUsers"));
        assert!(symbols.iter().any(|s| s.name == "deleteUser"));

        // Check specific features
        let user_service = symbols.iter().find(|s| s.name == "UserService");
        assert!(user_service.unwrap().signature.as_ref().unwrap().contains("implements CrudService<User, Long>"));

        let find_by_id_method = symbols.iter().find(|s| s.name == "findById");
        assert!(find_by_id_method.unwrap().signature.as_ref().unwrap().contains("@Override"));

        let find_active_users = symbols.iter().find(|s| s.name == "findActiveUsers");
        assert!(find_active_users.unwrap().signature.as_ref().unwrap().contains("@Async"));

        let delete_user = symbols.iter().find(|s| s.name == "deleteUser");
        assert!(delete_user.unwrap().signature.as_ref().unwrap().contains("@PreAuthorize"));

        println!("☕ Extracted {} Java symbols successfully", symbols.len());
    }

    // Performance and Edge Cases Tests
    #[test]
    fn test_handle_large_java_files_with_many_symbols() {
        // Generate a large Java file with many classes and methods
        let services = (0..15).map(|i| format!(r#"
/**
 * Service class for Service{i}
 */
@Service
@Transactional
public class Service{i} {{

    @Autowired
    private Repository{i} repository;

    @Value("${{service{i}.timeout:30}}")
    private int timeout;

    public static final String SERVICE_NAME = "Service{i}";

    public List<Entity{i}> findAll() {{
        return repository.findAll();
    }}

    public Optional<Entity{i}> findById(Long id) {{
        return repository.findById(id);
    }}

    @Async
    public CompletableFuture<Entity{i}> createAsync(Entity{i} entity) {{
        return CompletableFuture.supplyAsync(() -> repository.save(entity));
    }}

    @PreAuthorize("hasRole('ADMIN')")
    public void delete(Long id) {{
        repository.deleteById(id);
    }}
}}"#, i = i)).collect::<Vec<_>>().join("\n");

        let entities = (0..15).map(|i| format!(r#"
@Entity
@Table(name = "entity_{i}")
public class Entity{i} {{

    @Id
    @GeneratedValue(strategy = GenerationType.IDENTITY)
    private Long id;

    @Column(nullable = false)
    private String name;

    @Column
    private String description;

    @CreatedDate
    private LocalDateTime createdAt;

    @LastModifiedDate
    private LocalDateTime updatedAt;

    // Default constructor
    public Entity{i}() {{}}

    // Constructor with name
    public Entity{i}(String name) {{
        this.name = name;
    }}

    // Getters and setters
    public Long getId() {{ return id; }}
    public void setId(Long id) {{ this.id = id; }}

    public String getName() {{ return name; }}
    public void setName(String name) {{ this.name = name; }}

    public String getDescription() {{ return description; }}
    public void setDescription(String description) {{ this.description = description; }}

    public LocalDateTime getCreatedAt() {{ return createdAt; }}
    public void setCreatedAt(LocalDateTime createdAt) {{ this.createdAt = createdAt; }}

    public LocalDateTime getUpdatedAt() {{ return updatedAt; }}
    public void setUpdatedAt(LocalDateTime updatedAt) {{ this.updatedAt = updatedAt; }}

    @Override
    public boolean equals(Object obj) {{
        if (this == obj) return true;
        if (obj == null || getClass() != obj.getClass()) return false;
        Entity{i} entity = (Entity{i}) obj;
        return Objects.equals(id, entity.id);
    }}

    @Override
    public int hashCode() {{
        return Objects.hash(id);
    }}

    @Override
    public String toString() {{
        return "Entity{i}{{" +
            "id=" + id +
            ", name='" + name + '\'' +
            ", description='" + description + '\'' +
            '}}';
    }}
}}"#, i = i)).collect::<Vec<_>>().join("\n");

        let java_code = format!(r#"
package com.example.large;

import java.util.*;
import java.time.LocalDateTime;
import java.util.concurrent.CompletableFuture;
import javax.persistence.*;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;
import org.springframework.scheduling.annotation.Async;
import org.springframework.security.access.prepost.PreAuthorize;
import org.springframework.data.annotation.CreatedDate;
import org.springframework.data.annotation.LastModifiedDate;
import static java.util.stream.Collectors.*;

// Constants
public final class Constants {{
    public static final String APPLICATION_NAME = "Large Application";
    public static final String VERSION = "1.0.0";
    public static final int MAX_CONNECTIONS = 100;
    public static final long TIMEOUT_SECONDS = 30L;

    private Constants() {{
        // Utility class
    }}
}}

// Configuration
@Configuration
@EnableJpaRepositories
@EnableAsync
public class ApplicationConfig {{

    @Bean
    public TaskExecutor taskExecutor() {{
        ThreadPoolTaskExecutor executor = new ThreadPoolTaskExecutor();
        executor.setCorePoolSize(10);
        executor.setMaxPoolSize(20);
        executor.setQueueCapacity(200);
        executor.setThreadNamePrefix("app-");
        executor.initialize();
        return executor;
    }}

    @Bean
    public RestTemplate restTemplate() {{
        return new RestTemplate();
    }}
}}

{entities}
{services}

// Main application class
@SpringBootApplication
@EnableScheduling
@EnableCaching
public class LargeApplication {{

    private static final Logger log = LoggerFactory.getLogger(LargeApplication.class);

    public static void main(String[] args) {{
        log.info("Starting application: {{}}", Constants.APPLICATION_NAME);
        SpringApplication.run(LargeApplication.class, args);
        log.info("Application started successfully");
    }}
}}
"#, entities = entities, services = services);

        let mut parser = init_parser();
        let tree = parser.parse(&java_code, None).unwrap();

        let mut extractor = JavaExtractor::new(
            "java".to_string(),
            "test.java".to_string(),
            java_code.clone(),
        );

        let symbols = extractor.extract_symbols(&tree);
        let relationships = extractor.extract_relationships(&tree, &symbols);

        // Should extract many symbols
        assert!(symbols.len() > 200);

        // Check that all generated services were extracted
        for i in 0..15 {
            let service = symbols.iter().find(|s| s.name == format!("Service{}", i));
            assert!(service.is_some());
            assert_eq!(service.unwrap().kind, SymbolKind::Class);
            assert!(service.unwrap().signature.as_ref().unwrap().contains("@Service"));
        }

        // Check that all entities were extracted
        for i in 0..15 {
            let entity = symbols.iter().find(|s| s.name == format!("Entity{}", i));
            assert!(entity.is_some());
            assert_eq!(entity.unwrap().kind, SymbolKind::Class);
            assert!(entity.unwrap().signature.as_ref().unwrap().contains("@Entity"));
        }

        // Check constants class
        let constants = symbols.iter().find(|s| s.name == "Constants");
        assert!(constants.is_some());
        assert!(constants.unwrap().signature.as_ref().unwrap().contains("final class Constants"));

        let application_name = symbols.iter().find(|s| s.name == "APPLICATION_NAME");
        assert!(application_name.is_some());
        assert_eq!(application_name.unwrap().kind, SymbolKind::Constant);

        // Check configuration
        let app_config = symbols.iter().find(|s| s.name == "ApplicationConfig");
        assert!(app_config.is_some());
        assert!(app_config.unwrap().signature.as_ref().unwrap().contains("@Configuration"));

        // Check main application
        let large_application = symbols.iter().find(|s| s.name == "LargeApplication");
        assert!(large_application.is_some());
        assert!(large_application.unwrap().signature.as_ref().unwrap().contains("@SpringBootApplication"));

        let main_method = symbols.iter().find(|s| s.name == "main");
        assert!(main_method.is_some());
        assert!(main_method.unwrap().signature.as_ref().unwrap().contains("static void main(String[] args)"));

        println!("☕ Performance test: Extracted {} symbols and {} relationships", symbols.len(), relationships.len());
    }

    #[test]
    fn test_handle_edge_cases_and_malformed_code_gracefully() {
        let java_code = r#"
package com.example.edge;

// Edge cases and unusual Java constructs

// Empty classes and interfaces
public class EmptyClass {}
public interface EmptyInterface {}
public abstract class EmptyAbstractClass {}

// Classes with only static members
public final class UtilityClass {
    private UtilityClass() {}

    public static void utilityMethod() {}
    public static final String CONSTANT = "value";
}

// Deeply nested classes
public class Outer {
    public class Level1 {
        public class Level2 {
            public class Level3 {
                public void deepMethod() {}
            }
        }
    }
}

// Malformed code that shouldn't crash parser
public class MissingBrace {
    public void method() {
        // Missing closing brace

// Complex generics with wildcards
public class ComplexGenerics<T extends Comparable<? super T> & Serializable> {
    public <U extends T> void wildcardMethod(
        Map<? extends U, ? super T> input,
        Function<? super T, ? extends U> mapper
    ) {}
}

// Annotation with all possible targets
@Target({ElementType.TYPE, ElementType.METHOD, ElementType.FIELD, ElementType.PARAMETER})
@Retention(RetentionPolicy.RUNTIME)
public @interface ComplexAnnotation {
    String value() default "";
    Class<?>[] types() default {};
    ElementType[] targets() default {};
    int[] numbers() default {1, 2, 3};
}

// Enum with complex features
public enum ComplexEnum implements Comparable<ComplexEnum>, Serializable {
    FIRST("first", 1) {
        @Override
        public void abstractMethod() {
            System.out.println("FIRST implementation");
        }
    },
    SECOND("second", 2) {
        @Override
        public void abstractMethod() {
            System.out.println("SECOND implementation");
        }
    };

    private final String name;
    private final int value;

    ComplexEnum(String name, int value) {
        this.name = name;
        this.value = value;
    }

    public abstract void abstractMethod();

    public String getName() { return name; }
    public int getValue() { return value; }
}

// Interface with default and static methods
public interface ModernInterface {
    void abstractMethod();

    default void defaultMethod() {
        System.out.println("Default implementation");
    }

    static void staticMethod() {
        System.out.println("Static method in interface");
    }

    private void privateMethod() {
        System.out.println("Private method in interface");
    }
}

// Class with all possible modifiers
public final strictfp class AllModifiers {
    public static final transient volatile int field = 0;

    public static synchronized native void nativeMethod();

    public final strictfp void strictMethod() {}
}

// Anonymous class usage
public class AnonymousExample {
    public void useAnonymousClass() {
        Runnable runnable = new Runnable() {
            @Override
            public void run() {
                System.out.println("Anonymous implementation");
            }
        };

        Comparator<String> comparator = new Comparator<String>() {
            @Override
            public int compare(String s1, String s2) {
                return s1.compareToIgnoreCase(s2);
            }
        };
    }
}

// Method with all parameter types
public class ParameterTypes {
    public void allParameterTypes(
        final int primitiveParam,
        String objectParam,
        int... varargs,
        @Nullable String annotatedParam,
        List<? extends Number> wildcardParam,
        Map<String, ? super Integer> complexWildcard
    ) {}
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(java_code, None).unwrap();

        let mut extractor = JavaExtractor::new(
            "java".to_string(),
            "test.java".to_string(),
            java_code.to_string(),
        );

        // Should not throw even with malformed code
        let symbols = extractor.extract_symbols(&tree);
        let relationships = extractor.extract_relationships(&tree, &symbols);

        // Should still extract valid symbols
        let empty_class = symbols.iter().find(|s| s.name == "EmptyClass");
        assert!(empty_class.is_some());
        assert_eq!(empty_class.unwrap().kind, SymbolKind::Class);

        let empty_interface = symbols.iter().find(|s| s.name == "EmptyInterface");
        assert!(empty_interface.is_some());
        assert_eq!(empty_interface.unwrap().kind, SymbolKind::Interface);

        let utility_class = symbols.iter().find(|s| s.name == "UtilityClass");
        assert!(utility_class.is_some());
        assert!(utility_class.unwrap().signature.as_ref().unwrap().contains("final class UtilityClass"));

        let outer = symbols.iter().find(|s| s.name == "Outer");
        assert!(outer.is_some());

        let level3 = symbols.iter().find(|s| s.name == "Level3");
        assert!(level3.is_some());

        let complex_generics = symbols.iter().find(|s| s.name == "ComplexGenerics");
        assert!(complex_generics.is_some());
        assert!(complex_generics.unwrap().signature.as_ref().unwrap().contains("<T extends Comparable"));

        let complex_annotation = symbols.iter().find(|s| s.name == "ComplexAnnotation");
        assert!(complex_annotation.is_some());
        assert!(complex_annotation.unwrap().signature.as_ref().unwrap().contains("@interface ComplexAnnotation"));

        let complex_enum = symbols.iter().find(|s| s.name == "ComplexEnum");
        assert!(complex_enum.is_some());
        assert!(complex_enum.unwrap().signature.as_ref().unwrap().contains("enum ComplexEnum implements"));

        let modern_interface = symbols.iter().find(|s| s.name == "ModernInterface");
        assert!(modern_interface.is_some());
        assert_eq!(modern_interface.unwrap().kind, SymbolKind::Interface);

        let all_modifiers = symbols.iter().find(|s| s.name == "AllModifiers");
        assert!(all_modifiers.is_some());
        assert!(all_modifiers.unwrap().signature.as_ref().unwrap().contains("final strictfp class"));

        let native_method = symbols.iter().find(|s| s.name == "nativeMethod");
        assert!(native_method.is_some());
        assert!(native_method.unwrap().signature.as_ref().unwrap().contains("native"));

        let parameter_types = symbols.iter().find(|s| s.name == "ParameterTypes");
        assert!(parameter_types.is_some());

        let all_parameter_types_method = symbols.iter().find(|s| s.name == "allParameterTypes");
        assert!(all_parameter_types_method.is_some());
        assert!(all_parameter_types_method.unwrap().signature.as_ref().unwrap().contains("int..."));

        println!("☕ Edge case test: Extracted {} symbols from complex code", symbols.len());
    }

    // Type Inference Tests
    #[test]
    fn test_infer_types_from_java_annotations() {
        let java_code = r#"
package com.example;

public class TypeExample {
    public String getName() {
        return "test";
    }

    public int calculate(int x, int y) {
        return x + y;
    }

    public List<String> getNames() {
        return Arrays.asList("a", "b");
    }

    private boolean isValid;
    public final String CONSTANT = "value";
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(java_code, None).unwrap();

        let mut extractor = JavaExtractor::new(
            "java".to_string(),
            "test.java".to_string(),
            java_code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);
        let types = extractor.infer_types(&symbols);

        let get_name = symbols.iter().find(|s| s.name == "getName");
        assert!(get_name.is_some());
        assert_eq!(types.get(&get_name.unwrap().id).unwrap(), "String");

        let calculate = symbols.iter().find(|s| s.name == "calculate");
        assert!(calculate.is_some());
        assert_eq!(types.get(&calculate.unwrap().id).unwrap(), "int");

        let get_names = symbols.iter().find(|s| s.name == "getNames");
        assert!(get_names.is_some());
        assert_eq!(types.get(&get_names.unwrap().id).unwrap(), "List<String>");

        let is_valid = symbols.iter().find(|s| s.name == "isValid");
        assert!(is_valid.is_some());
        assert_eq!(types.get(&is_valid.unwrap().id).unwrap(), "boolean");

        println!("☕ Type inference extracted {} types", types.len());
    }

    // Relationship Extraction Tests
    #[test]
    fn test_extract_inheritance_and_implementation_relationships() {
        let java_code = r#"
package com.example;

public class Dog extends Animal implements Runnable {
    @Override
    public void run() {
        // implementation
    }
}

public abstract class Animal {
    public abstract void move();
}

public interface Runnable {
    void run();
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(java_code, None).unwrap();

        let mut extractor = JavaExtractor::new(
            "java".to_string(),
            "test.java".to_string(),
            java_code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);
        let relationships = extractor.extract_relationships(&tree, &symbols);

        // Should find inheritance and implementation relationships
        assert!(relationships.len() >= 1);

        println!("☕ Found {} Java relationships", relationships.len());
    }
}