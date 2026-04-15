//! Shared extractor factory wrappers.
//!
//! Dispatch now lives in `registry.rs`. This module exists for in-crate tests and helpers that
//! still need direct registry dispatch against a pre-parsed tree.

#[cfg(test)]
use crate::base::ExtractionResults;
use crate::base::TypeInfo;
use std::collections::HashMap;
#[cfg(test)]
use std::path::Path;

/// Convert a raw type map from `infer_types()` into the richer `TypeInfo` structure.
pub(crate) fn convert_types_map(
    types: HashMap<String, String>,
    language: &str,
) -> HashMap<String, TypeInfo> {
    types
        .into_iter()
        .map(|(symbol_id, type_string)| {
            (
                symbol_id.clone(),
                TypeInfo {
                    symbol_id,
                    resolved_type: type_string,
                    generic_params: None,
                    constraints: None,
                    is_inferred: true,
                    language: language.to_string(),
                    metadata: None,
                },
            )
        })
        .collect()
}

#[cfg(test)]
pub(crate) fn extract_symbols_and_relationships(
    tree: &tree_sitter::Tree,
    file_path: &str,
    content: &str,
    language: &str,
    workspace_root: &Path,
) -> Result<ExtractionResults, anyhow::Error> {
    crate::registry::extract_for_language(language, tree, file_path, content, workspace_root)
}

#[cfg(test)]
mod factory_consistency_tests {
    use super::*;
    use std::path::PathBuf;
    use tree_sitter::Parser;

    #[test]
    fn test_all_languages_in_factory() {
        let supported = crate::registry::supported_languages();

        assert_eq!(
            supported.len(),
            35,
            "Expected 35 language entries including jsx and tsx aliases"
        );

        let workspace_root = PathBuf::from("/tmp/test");

        for language in &supported {
            let test_content = "// test";
            let mut parser = Parser::new();
            let ts_lang = match crate::language::get_tree_sitter_language(language) {
                Ok(lang) => lang,
                Err(_) => continue,
            };

            parser.set_language(&ts_lang).unwrap();
            let tree = parser.parse(test_content, None).unwrap();

            let result = extract_symbols_and_relationships(
                &tree,
                "test.rs",
                test_content,
                language,
                &workspace_root,
            );

            if let Err(e) = result {
                let error_msg = format!("{}", e);
                assert!(
                    !error_msg.contains("No extractor available"),
                    "Language '{}' is missing from registry dispatch! Error: {}",
                    language,
                    error_msg
                );
            }
        }
    }

    #[test]
    fn test_factory_rejects_unknown_language() {
        let workspace_root = PathBuf::from("/tmp/test");
        let mut parser = Parser::new();
        let ts_lang = crate::language::get_tree_sitter_language("rust").unwrap();
        parser.set_language(&ts_lang).unwrap();
        let tree = parser.parse("// test", None).unwrap();

        let result = extract_symbols_and_relationships(
            &tree,
            "test.unknown",
            "// test",
            "unknown_language_xyz",
            &workspace_root,
        );

        assert!(result.is_err(), "Should reject unknown language");
        assert!(
            format!("{}", result.unwrap_err()).contains("No extractor available"),
            "Error should mention no extractor available"
        );
    }
}

#[cfg(test)]
mod test_factory_returns_identifiers {
    use std::path::PathBuf;
    use tree_sitter::Parser;

    #[test]
    fn test_factory_returns_python_identifiers() {
        let code = r#"
def foo():
    bar()
    x.method()
"#;

        let workspace_root = PathBuf::from("/tmp");
        let mut parser = Parser::new();
        let language = tree_sitter_python::LANGUAGE;
        parser.set_language(&language.into()).unwrap();
        let tree = parser.parse(code, None).unwrap();

        let results = crate::factory::extract_symbols_and_relationships(
            &tree,
            "test.py",
            code,
            "python",
            &workspace_root,
        )
        .unwrap();

        assert!(!results.symbols.is_empty(), "Should extract symbols");
        assert!(
            !results.identifiers.is_empty(),
            "Factory should return identifiers from Python code!"
        );
    }
}
