use crate::{extract_canonical, language::language_spec};

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

#[test]
fn test_go_all_preceding_comments_can_remain_docs() {
    let doc = symbol_doc_comment(
        "src/main.go",
        "// Foo prepares requests\n// It keeps Go's package-comment convention\nfunc Foo() {}",
        "Foo",
    );
    let doc = doc.expect("Go line comments should be treated as docs");
    assert!(doc.contains("Foo prepares requests"));
    assert!(doc.contains("package-comment convention"));
}

#[test]
fn test_r_roxygen_comment_is_doc_but_plain_hash_comment_is_not() {
    let roxygen_doc = symbol_doc_comment(
        "src/stats.R",
        "#' Computes a moving average\nmoving_average <- function(values) values\n",
        "moving_average",
    );
    assert!(
        roxygen_doc
            .as_deref()
            .is_some_and(|comment| comment.contains("moving average"))
    );

    let plain_hash_doc = symbol_doc_comment(
        "src/stats.R",
        "# helper note for maintainers\nplain_average <- function(values) values\n",
        "plain_average",
    );
    assert_eq!(plain_hash_doc, None);
}

#[test]
fn test_lua_block_doc_precedence_over_line_comment() {
    let doc = symbol_doc_comment(
        "src/service.lua",
        r#"-- implementation note, not API docs
--[[
Service table documentation
]]
Service = {}
"#,
        "Service",
    )
    .expect("Lua block comment should be treated as docs");

    assert!(doc.contains("Service table documentation"));
    assert!(
        !doc.contains("implementation note"),
        "Plain Lua comments before a doc block should not be folded into docs"
    );
}

#[test]
fn test_language_without_slash_star_doc_style_does_not_accept_doc_block() {
    let spec = language_spec("markdown").expect("markdown spec should exist");

    assert!(!spec.is_doc_comment("/** not markdown docs */"));
}

#[test]
fn test_swift_slash_star_doc_block_remains_explicitly_supported() {
    let doc = symbol_doc_comment(
        "Sources/App/Service.swift",
        "/** Starts the service */\nfunc start() {}\n",
        "start",
    );

    assert!(
        doc.as_deref()
            .is_some_and(|comment| comment.contains("Starts the service"))
    );
}

#[test]
fn test_javascript_triple_slash_comment_is_not_java_doc_comment() {
    let doc = symbol_doc_comment(
        "src/app.js",
        "/// Java-style docs should not apply here\nfunction foo() {\n  return 1;\n}\n",
        "foo",
    );
    assert_eq!(doc, None);
}
