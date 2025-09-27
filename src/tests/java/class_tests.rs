// Class Extraction Tests
//
// Direct port of Miller's Java extractor tests (TDD RED phase)

use super::*;

#[cfg(test)]
mod class_tests {
    use super::*;

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

    // Additional class extraction tests would go here...
    // (Truncated for demonstration - full implementation would include all class tests)
}
