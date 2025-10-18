// Annotation Extraction Tests
//
// Tests for Java annotation extraction including:
// - Annotation definitions (@interface)
// - Annotation usage on classes, methods, fields
// - Built-in annotations (@Override, @Deprecated)
// - Custom annotations with parameters

use super::*;

#[cfg(test)]
mod annotation_tests {
    use super::*;

    #[test]
    fn test_extract_annotation_definitions() {
        let code = r#"
package com.example.annotations;

public @interface RequestMapping {
    String value() default "";
    String method() default "GET";
}

@interface Retention {
    RetentionPolicy value();
}

@Target({ElementType.TYPE, ElementType.METHOD})
@interface Controller {
    String name() default "";
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

        let request_mapping = symbols.iter().find(|s| s.name == "RequestMapping");
        assert!(request_mapping.is_some());
        assert_eq!(request_mapping.unwrap().kind, SymbolKind::Interface); // Annotations are interfaces
        assert!(request_mapping
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("public @interface RequestMapping"));

        let retention = symbols.iter().find(|s| s.name == "Retention");
        assert!(retention.is_some());
        assert_eq!(retention.unwrap().kind, SymbolKind::Interface);

        let controller = symbols.iter().find(|s| s.name == "Controller");
        assert!(controller.is_some());
        assert_eq!(controller.unwrap().kind, SymbolKind::Interface);
    }

    #[test]
    fn test_extract_annotation_usage() {
        let code = r#"
@RestController
@RequestMapping("/api/users")
public class UserController {

    @Autowired
    private UserService userService;

    @GetMapping("/{id}")
    public User getUser(@PathVariable Long id) {
        return userService.findById(id);
    }

    @PostMapping
    @Transactional
    public User createUser(@RequestBody @Valid User user) {
        return userService.save(user);
    }

    @Override
    public String toString() {
        return "UserController";
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

        // Verify class annotations are captured
        let user_controller = symbols.iter().find(|s| s.name == "UserController");
        assert!(user_controller.is_some());
        assert!(user_controller
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("@RestController"));
        assert!(user_controller
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("@RequestMapping"));

        // Verify method annotations are captured
        let get_user_method = symbols.iter().find(|s| s.name == "getUser");
        assert!(get_user_method.is_some());
        assert!(get_user_method
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("@GetMapping"));

        let create_user_method = symbols.iter().find(|s| s.name == "createUser");
        assert!(create_user_method.is_some());
        assert!(create_user_method
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("@PostMapping"));
        assert!(create_user_method
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("@Transactional"));
    }

    #[test]
    fn test_extract_builtin_annotations() {
        let code = r#"
public class Example {
    @Override
    public String toString() {
        return "example";
    }

    @Deprecated
    public void oldMethod() {
        // This method is deprecated
    }

    @SuppressWarnings("unchecked")
    public void uncheckedMethod() {
        List list = new ArrayList();
    }

    @SafeVarargs
    public final void varargsMethod(String... args) {
        // Safe varargs usage
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

        let to_string_method = symbols.iter().find(|s| s.name == "toString");
        assert!(to_string_method.is_some());
        assert!(to_string_method
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("@Override"));

        let old_method = symbols.iter().find(|s| s.name == "oldMethod");
        assert!(old_method.is_some());
        assert!(old_method
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("@Deprecated"));

        let unchecked_method = symbols.iter().find(|s| s.name == "uncheckedMethod");
        assert!(unchecked_method.is_some());
        assert!(unchecked_method
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("@SuppressWarnings"));
    }
}
