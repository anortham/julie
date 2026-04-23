// Python decorators inline tests extracted from extractors/python/decorators.rs

use crate::base::SymbolKind;
use crate::python::PythonExtractor;
use std::path::PathBuf;

#[test]
fn test_decorator_extraction() {
    // This test is placeholder - actual testing requires tree-sitter
    // Real tests are in the integration tests
}

#[test]
fn python_decorator_markers_persist_for_functions_and_classes() {
    let code = r#"
@app.route("/users/<id>")
def show_user(id):
    return id

@pytest.mark.parametrize("value", [1, 2])
def test_value(value):
    assert value

@dataclass
class User:
    id: int
"#;

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .expect("failed to load Python grammar");
    let tree = parser.parse(code, None).expect("failed to parse Python");

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = PythonExtractor::new(
        "test_routes.py".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);

    let show_user = symbols
        .iter()
        .find(|s| s.name == "show_user" && s.kind == SymbolKind::Function)
        .expect("show_user function should be extracted");
    assert_eq!(show_user.annotations.len(), 1);
    assert_eq!(show_user.annotations[0].annotation, "app.route");
    assert_eq!(show_user.annotations[0].annotation_key, "app.route");
    assert_eq!(
        show_user.annotations[0].raw_text.as_deref(),
        Some("app.route(\"/users/<id>\")")
    );
    assert_eq!(show_user.annotations[0].carrier, None);
    assert!(
        show_user
            .signature
            .as_deref()
            .unwrap_or_default()
            .contains("@app.route def show_user")
    );

    let test_value = symbols
        .iter()
        .find(|s| s.name == "test_value" && s.kind == SymbolKind::Function)
        .expect("test_value function should be extracted");
    assert_eq!(test_value.annotations.len(), 1);
    assert_eq!(
        test_value.annotations[0].annotation,
        "pytest.mark.parametrize"
    );
    assert_eq!(
        test_value.annotations[0].annotation_key,
        "pytest.mark.parametrize"
    );
    assert_eq!(
        test_value.annotations[0].raw_text.as_deref(),
        Some("pytest.mark.parametrize(\"value\", [1, 2])")
    );
    assert_eq!(
        test_value
            .metadata
            .as_ref()
            .and_then(|m| m.get("is_test"))
            .and_then(|v| v.as_bool()),
        Some(true)
    );

    let user = symbols
        .iter()
        .find(|s| s.name == "User" && s.kind == SymbolKind::Class)
        .expect("User class should be extracted");
    assert_eq!(user.annotations.len(), 1);
    assert_eq!(user.annotations[0].annotation, "dataclass");
    assert_eq!(user.annotations[0].annotation_key, "dataclass");
    assert_eq!(user.annotations[0].raw_text.as_deref(), Some("dataclass"));
    assert!(
        user.signature
            .as_deref()
            .unwrap_or_default()
            .contains("@dataclass class User")
    );
}
