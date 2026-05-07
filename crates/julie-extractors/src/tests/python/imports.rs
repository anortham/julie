use crate::base::{Symbol, SymbolKind};
use crate::python::PythonExtractor;
use std::path::PathBuf;

fn extract_symbols(code: &str) -> Vec<Symbol> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor =
        PythonExtractor::new("test.py".to_string(), code.to_string(), &workspace_root);
    extractor.extract_symbols(&tree)
}

/// Verifies that `import a, b` emits one Import symbol per binding, not one
/// symbol for the whole statement. The multi-binding case is handled by iterating
/// over `dotted_name` children inside the `import_statement` node.
#[test]
fn test_python_plain_import_statement_emits_every_binding() {
    let code = "import os, sys\n";
    let symbols = extract_symbols(code);

    let import_symbols: Vec<&Symbol> = symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Import)
        .collect();

    assert_eq!(
        import_symbols.len(),
        2,
        "import os, sys must emit exactly 2 Import symbols (one per binding), got: {:?}",
        import_symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
    );

    let os_sym = import_symbols
        .iter()
        .find(|s| s.name == "os")
        .expect("must extract 'os' binding");
    assert_eq!(
        os_sym.signature.as_deref(),
        Some("import os"),
        "os symbol must have signature 'import os'"
    );

    let sys_sym = import_symbols
        .iter()
        .find(|s| s.name == "sys")
        .expect("must extract 'sys' binding");
    assert_eq!(
        sys_sym.signature.as_deref(),
        Some("import sys"),
        "sys symbol must have signature 'import sys'"
    );
}
