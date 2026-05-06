//! Inline tests extracted from extractors/python/functions.rs
//!
//! Tests for function and method extraction from Python code.
//! Handles regular functions, async functions, lambdas, and method detection.

#[cfg(test)]
mod tests {
    use crate::base::SymbolKind;
    use crate::python::PythonExtractor;
    use std::path::PathBuf;

    #[test]
    fn test_python_same_named_classes_keep_method_parent_ids_distinct() {
        let code = r#"
class A:
    def first(self):
        pass

class A:
    def second(self):
        pass
"#;
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor =
            PythonExtractor::new("test.py".to_string(), code.to_string(), &workspace_root);
        let symbols = extractor.extract_symbols(&tree);

        let classes: Vec<_> = symbols
            .iter()
            .filter(|symbol| symbol.name == "A" && symbol.kind == SymbolKind::Class)
            .collect();
        assert_eq!(classes.len(), 2);
        assert_ne!(classes[0].id, classes[1].id);

        let first = symbols
            .iter()
            .find(|symbol| symbol.name == "first" && symbol.kind == SymbolKind::Method)
            .expect("first method should be extracted");
        let second = symbols
            .iter()
            .find(|symbol| symbol.name == "second" && symbol.kind == SymbolKind::Method)
            .expect("second method should be extracted");

        let first_class = classes
            .iter()
            .find(|class| class.start_line <= first.start_line && class.end_line >= first.end_line)
            .expect("first method should be contained by a class");
        let second_class = classes
            .iter()
            .find(|class| {
                class.start_line <= second.start_line && class.end_line >= second.end_line
            })
            .expect("second method should be contained by a class");

        assert_eq!(first.parent_id.as_deref(), Some(first_class.id.as_str()));
        assert_eq!(second.parent_id.as_deref(), Some(second_class.id.as_str()));
        assert_ne!(first.parent_id, second.parent_id);
    }
}
