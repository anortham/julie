// Class Extraction Tests
//
// Direct Implementation of Java extractor tests (TDD RED phase)

use crate::extractors::base::{SymbolKind, Visibility};
use crate::extractors::java::JavaExtractor;
use crate::tests::test_utils::init_parser;
use std::path::PathBuf;

#[cfg(test)]
mod class_tests {
    use super::*;

    #[test]
    fn test_extract_class_definitions_with_modifiers() {
        let workspace_root = PathBuf::from("/tmp/test");
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

        let tree = init_parser(code, "java");

        let mut extractor = JavaExtractor::new(
            "java".to_string(),
            "test.java".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        let user_class = symbols.iter().find(|s| s.name == "User");
        assert!(user_class.is_some());
        assert_eq!(user_class.unwrap().kind, SymbolKind::Class);
        assert!(user_class
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("public class User"));
        assert_eq!(
            user_class.unwrap().visibility.as_ref().unwrap(),
            &Visibility::Public
        );

        let animal_class = symbols.iter().find(|s| s.name == "Animal");
        assert!(animal_class.is_some());
        assert!(animal_class
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("abstract class Animal"));
        assert_eq!(
            animal_class.unwrap().visibility.as_ref().unwrap(),
            &Visibility::Private
        );

        let constants_class = symbols.iter().find(|s| s.name == "Constants");
        assert!(constants_class.is_some());
        assert!(constants_class
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("final class Constants"));
    }

    #[test]
    fn test_extract_enum_declarations() {
        let workspace_root = PathBuf::from("/tmp/test");
        let code = r#"
package com.example;

public enum Status {
    PENDING,
    ACTIVE,
    COMPLETED
}

enum Priority {
    LOW, MEDIUM, HIGH
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

        let status_enum = symbols.iter().find(|s| s.name == "Status");
        assert!(status_enum.is_some(), "Status enum should be found");
        assert_eq!(status_enum.unwrap().kind, SymbolKind::Enum);
        assert_eq!(
            status_enum.unwrap().visibility.as_ref().unwrap(),
            &Visibility::Public
        );

        // Verify enum constants are extracted
        let pending = symbols.iter().find(|s| s.name == "PENDING");
        assert!(pending.is_some(), "PENDING constant should be found");
        assert_eq!(pending.unwrap().kind, SymbolKind::EnumMember);

        let priority_enum = symbols.iter().find(|s| s.name == "Priority");
        assert!(priority_enum.is_some(), "Priority enum should be found");
    }

    #[test]
    fn test_extract_record_declarations() {
        let workspace_root = PathBuf::from("/tmp/test");
        let code = r#"
package com.example;

public record Point(int x, int y) {}

record Person(String name, int age) {
    public Person {
        if (age < 0) throw new IllegalArgumentException();
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

        let point_record = symbols.iter().find(|s| s.name == "Point");
        assert!(point_record.is_some(), "Point record should be found");
        assert_eq!(point_record.unwrap().kind, SymbolKind::Class);
        assert_eq!(
            point_record.unwrap().visibility.as_ref().unwrap(),
            &Visibility::Public
        );

        let person_record = symbols.iter().find(|s| s.name == "Person");
        assert!(person_record.is_some(), "Person record should be found");
    }

    #[test]
    fn test_extract_nested_classes() {
        let workspace_root = PathBuf::from("/tmp/test");
        let code = r#"
package com.example;

public class Outer {
    private class Inner {
        void innerMethod() {}
    }

    public static class StaticNested {
        public void nestedMethod() {}
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

        let outer = symbols.iter().find(|s| s.name == "Outer");
        assert!(outer.is_some(), "Outer class should be found");

        let inner = symbols.iter().find(|s| s.name == "Inner");
        assert!(inner.is_some(), "Inner class should be found");
        assert!(inner.unwrap().parent_id.is_some(), "Inner should have parent");

        let nested = symbols.iter().find(|s| s.name == "StaticNested");
        assert!(nested.is_some(), "StaticNested class should be found");
    }
}
