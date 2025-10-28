/// Tests for Java extractor JavaDoc comment extraction
use crate::extractors::java::JavaExtractor;
use crate::tests::test_utils::init_parser;
use std::path::PathBuf;

#[test]
fn test_extract_javadoc_on_class() {
    let workspace_root = PathBuf::from("/tmp/test");
    let code = r#"
/**
 * Manages user sessions and authentication
 */
public class SessionManager {
    private String sessionId;
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

    // Find the SessionManager class
    let class_symbol = symbols
        .iter()
        .find(|s| s.name == "SessionManager")
        .expect("SessionManager class should be extracted");

    // Verify the JavaDoc comment is extracted
    assert!(
        class_symbol.doc_comment.is_some(),
        "SessionManager should have a doc_comment"
    );

    let doc_comment = class_symbol.doc_comment.as_ref().unwrap();
    assert!(
        doc_comment.contains("Manages user sessions and authentication"),
        "doc_comment should contain the JavaDoc text. Got: {:?}",
        doc_comment
    );
}

#[test]
fn test_extract_javadoc_on_method() {
    let workspace_root = PathBuf::from("/tmp/test");
    let code = r#"
public class SessionManager {
    /**
     * Authenticates user credentials
     * @param username the user's login name
     * @param password the user's password
     */
    public boolean authenticate(String username, String password) {
        return true;
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

    // Find the authenticate method
    let method_symbol = symbols
        .iter()
        .find(|s| s.name == "authenticate")
        .expect("authenticate method should be extracted");

    // Verify the JavaDoc comment is extracted
    assert!(
        method_symbol.doc_comment.is_some(),
        "authenticate method should have a doc_comment"
    );

    let doc_comment = method_symbol.doc_comment.as_ref().unwrap();
    assert!(
        doc_comment.contains("Authenticates user credentials"),
        "doc_comment should contain the JavaDoc text. Got: {:?}",
        doc_comment
    );
}

#[test]
fn test_extract_javadoc_on_field() {
    let workspace_root = PathBuf::from("/tmp/test");
    let code = r#"
public class SessionManager {
    /** The session timeout in milliseconds */
    private long sessionTimeout = 3600000;
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

    // Find the sessionTimeout field
    let field_symbol = symbols
        .iter()
        .find(|s| s.name == "sessionTimeout")
        .expect("sessionTimeout field should be extracted");

    // Verify the JavaDoc comment is extracted
    assert!(
        field_symbol.doc_comment.is_some(),
        "sessionTimeout field should have a doc_comment"
    );

    let doc_comment = field_symbol.doc_comment.as_ref().unwrap();
    assert!(
        doc_comment.contains("The session timeout in milliseconds"),
        "doc_comment should contain the JavaDoc text. Got: {:?}",
        doc_comment
    );
}

#[test]
fn test_no_javadoc_when_missing() {
    let workspace_root = PathBuf::from("/tmp/test");
    let code = r#"
public class SimpleClass {
    private String name;
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

    // Find the SimpleClass
    let class_symbol = symbols
        .iter()
        .find(|s| s.name == "SimpleClass")
        .expect("SimpleClass should be extracted");

    // Verify there is no doc_comment when not provided
    assert!(
        class_symbol.doc_comment.is_none(),
        "SimpleClass should have no doc_comment"
    );
}

#[test]
fn test_extract_javadoc_on_interface() {
    let workspace_root = PathBuf::from("/tmp/test");
    let code = r#"
/**
 * Authentication service interface
 */
public interface AuthService {
    /**
     * Validates credentials
     */
    boolean validate(String username, String password);
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

    // Find the AuthService interface
    let interface_symbol = symbols
        .iter()
        .find(|s| s.name == "AuthService")
        .expect("AuthService interface should be extracted");

    // Verify the JavaDoc comment is extracted
    assert!(
        interface_symbol.doc_comment.is_some(),
        "AuthService should have a doc_comment"
    );

    let doc_comment = interface_symbol.doc_comment.as_ref().unwrap();
    assert!(
        doc_comment.contains("Authentication service interface"),
        "doc_comment should contain the JavaDoc text. Got: {:?}",
        doc_comment
    );
}

#[test]
fn test_extract_javadoc_on_enum() {
    let workspace_root = PathBuf::from("/tmp/test");
    let code = r#"
/**
 * User role enumeration
 */
public enum Role {
    /** Administrator role */
    ADMIN,
    /** Regular user role */
    USER
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

    // Find the Role enum
    let enum_symbol = symbols
        .iter()
        .find(|s| s.name == "Role")
        .expect("Role enum should be extracted");

    // Verify the JavaDoc comment is extracted
    assert!(
        enum_symbol.doc_comment.is_some(),
        "Role enum should have a doc_comment"
    );

    let doc_comment = enum_symbol.doc_comment.as_ref().unwrap();
    assert!(
        doc_comment.contains("User role enumeration"),
        "doc_comment should contain the JavaDoc text. Got: {:?}",
        doc_comment
    );
}
