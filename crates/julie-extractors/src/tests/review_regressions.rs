use crate::base::RelationshipKind;
use crate::extract_canonical;

#[test]
fn test_review_regression_typescript_implements_keeps_namespace_context() {
    let workspace_root = std::path::PathBuf::from("/test/workspace");
    let code = r#"
class ServiceImpl implements Api.IService<Response> {
    serve() { }
}
"#;

    let results = extract_canonical("src/service-impl.ts", code, &workspace_root)
        .expect("typescript extraction should succeed");

    let pending = results
        .structured_pending_relationships
        .iter()
        .find(|pending| pending.pending.kind == RelationshipKind::Implements)
        .expect("should emit structured implements pending relationship");
    assert_eq!(pending.target.display_name, "Api.IService");
    assert_eq!(pending.target.terminal_name, "IService");
    assert_eq!(pending.target.namespace_path, vec!["Api"]);
}

#[test]
fn test_review_regression_python_hash_comments_are_not_docs() {
    let workspace_root = std::path::PathBuf::from("/test/workspace");
    let code = "# helper comment\ndef foo():\n    return 1\n";

    let results = extract_canonical("src/module.py", code, &workspace_root)
        .expect("python extraction should succeed");
    let foo = results
        .symbols
        .iter()
        .find(|symbol| symbol.name == "foo")
        .expect("should extract foo");
    assert_eq!(foo.doc_comment, None);
}
