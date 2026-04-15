use crate::extract_canonical;

fn symbol_doc_comment(file_path: &str, content: &str, symbol_name: &str) -> Option<String> {
    let workspace_root = std::path::PathBuf::from("/test/workspace");
    let results = extract_canonical(file_path, content, &workspace_root)
        .expect("canonical extraction should succeed");

    results
        .symbols
        .iter()
        .find(|symbol| symbol.name == symbol_name)
        .and_then(|symbol| symbol.doc_comment.clone())
}

#[test]
fn test_rust_plain_line_comment_is_not_doc_comment() {
    let doc = symbol_doc_comment("src/lib.rs", "// helper comment\nfn foo() {}", "foo");
    assert_eq!(doc, None);
}

#[test]
fn test_rust_plain_block_comment_is_not_doc_comment() {
    let doc = symbol_doc_comment("src/lib.rs", "/* helper comment */\nfn foo() {}", "foo");
    assert_eq!(doc, None);
}

#[test]
fn test_python_hash_comment_is_not_doc_comment() {
    let doc = symbol_doc_comment(
        "src/module.py",
        "# helper comment\ndef foo():\n    return 1\n",
        "foo",
    );
    assert_eq!(doc, None);
}

#[test]
fn test_go_line_comment_remains_doc_comment() {
    let doc = symbol_doc_comment("src/main.go", "// Foo docs\nfunc Foo() {}", "Foo");
    assert!(
        doc.as_deref()
            .is_some_and(|comment| comment.contains("Foo docs"))
    );
}
