//! Shared extractor factory - Single source of truth for all 30 languages
//!
//! This module provides the centralized factory function for all language extractors.
//! It ensures consistency across the codebase and prevents bugs from missing languages
//! in different code paths.

use crate::base::{ExtractionResults, TypeInfo};
use anyhow::anyhow;
use std::collections::HashMap;
use std::path::Path;

/// Convert a raw type map from `infer_types()` into the richer `TypeInfo` structure.
///
/// Currently all extracted types are marked as inferred with no generic params,
/// constraints, or metadata. This is the single place to enrich type extraction
/// when we're ready (see Task #1 in the audit plan).
fn convert_types_map(types: HashMap<String, String>, language: &str) -> HashMap<String, TypeInfo> {
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

/// Extract symbols and relationships for ANY supported language
///
/// This is the centralized factory function for all 30 language extractors.
/// It ensures consistency across the codebase and prevents bugs from missing
/// languages in different code paths.
///
/// # Parameters
/// - `tree`: Pre-parsed tree-sitter AST
/// - `file_path`: Relative Unix-style file path (for symbol storage)
/// - `content`: Source code content
/// - `language`: Language identifier (lowercase, e.g., "rust", "r", "qml")
/// - `workspace_root`: Workspace root path for relative path calculations
///
/// # Returns
/// `ExtractionResults` containing symbols, relationships, identifiers, and types
pub fn extract_symbols_and_relationships(
    tree: &tree_sitter::Tree,
    file_path: &str,
    content: &str,
    language: &str,
    workspace_root: &Path,
) -> Result<ExtractionResults, anyhow::Error> {
    match language {
        // ─── Languages with full extraction (symbols + rels + pending + identifiers + types) ───

        "rust" => {
            let mut ext = crate::rust::RustExtractor::new(language.to_string(), file_path.to_string(), content.to_string(), workspace_root);
            let symbols = ext.extract_symbols(tree);
            let relationships = ext.extract_relationships(tree, &symbols);
            let identifiers = ext.extract_identifiers(tree, &symbols);
            let types = ext.infer_types(&symbols);
            let pending = ext.get_pending_relationships();
            Ok(ExtractionResults { symbols, relationships, pending_relationships: pending, identifiers, types: convert_types_map(types, language) })
        }
        "typescript" | "tsx" => {
            let mut ext = crate::typescript::TypeScriptExtractor::new(language.to_string(), file_path.to_string(), content.to_string(), workspace_root);
            let symbols = ext.extract_symbols(tree);
            let relationships = ext.extract_relationships(tree, &symbols);
            let identifiers = ext.extract_identifiers(tree, &symbols);
            let types = ext.infer_types(&symbols);
            let pending = ext.get_pending_relationships();
            Ok(ExtractionResults { symbols, relationships, pending_relationships: pending, identifiers, types: convert_types_map(types, language) })
        }
        "javascript" | "jsx" => {
            let mut ext = crate::javascript::JavaScriptExtractor::new(language.to_string(), file_path.to_string(), content.to_string(), workspace_root);
            let symbols = ext.extract_symbols(tree);
            let relationships = ext.extract_relationships(tree, &symbols);
            let identifiers = ext.extract_identifiers(tree, &symbols);
            let types = ext.infer_types(&symbols);
            let pending = ext.get_pending_relationships();
            Ok(ExtractionResults { symbols, relationships, pending_relationships: pending, identifiers, types: convert_types_map(types, language) })
        }
        "python" => {
            let mut ext = crate::python::PythonExtractor::new(file_path.to_string(), content.to_string(), workspace_root);
            let symbols = ext.extract_symbols(tree);
            let relationships = ext.extract_relationships(tree, &symbols);
            let identifiers = ext.extract_identifiers(tree, &symbols);
            let types = ext.infer_types(&symbols);
            let pending = ext.get_pending_relationships();
            Ok(ExtractionResults { symbols, relationships, pending_relationships: pending, identifiers, types: convert_types_map(types, language) })
        }
        "java" => {
            let mut ext = crate::java::JavaExtractor::new(language.to_string(), file_path.to_string(), content.to_string(), workspace_root);
            let symbols = ext.extract_symbols(tree);
            let relationships = ext.extract_relationships(tree, &symbols);
            let identifiers = ext.extract_identifiers(tree, &symbols);
            let types = ext.infer_types(&symbols);
            let pending = ext.get_pending_relationships();
            Ok(ExtractionResults { symbols, relationships, pending_relationships: pending, identifiers, types: convert_types_map(types, language) })
        }
        "csharp" => {
            let mut ext = crate::csharp::CSharpExtractor::new(language.to_string(), file_path.to_string(), content.to_string(), workspace_root);
            let symbols = ext.extract_symbols(tree);
            let relationships = ext.extract_relationships(tree, &symbols);
            let identifiers = ext.extract_identifiers(tree, &symbols);
            let types = ext.infer_types(&symbols);
            let pending = ext.get_pending_relationships();
            Ok(ExtractionResults { symbols, relationships, pending_relationships: pending, identifiers, types: convert_types_map(types, language) })
        }
        "php" => {
            let mut ext = crate::php::PhpExtractor::new(language.to_string(), file_path.to_string(), content.to_string(), workspace_root);
            let symbols = ext.extract_symbols(tree);
            let relationships = ext.extract_relationships(tree, &symbols);
            let identifiers = ext.extract_identifiers(tree, &symbols);
            let types = ext.infer_types(&symbols);
            let pending = ext.get_pending_relationships();
            Ok(ExtractionResults { symbols, relationships, pending_relationships: pending, identifiers, types: convert_types_map(types, language) })
        }
        "swift" => {
            let mut ext = crate::swift::SwiftExtractor::new(language.to_string(), file_path.to_string(), content.to_string(), workspace_root);
            let symbols = ext.extract_symbols(tree);
            let relationships = ext.extract_relationships(tree, &symbols);
            let identifiers = ext.extract_identifiers(tree, &symbols);
            let types = ext.infer_types(&symbols);
            let pending = ext.get_pending_relationships();
            Ok(ExtractionResults { symbols, relationships, pending_relationships: pending, identifiers, types: convert_types_map(types, language) })
        }
        "kotlin" => {
            let mut ext = crate::kotlin::KotlinExtractor::new(language.to_string(), file_path.to_string(), content.to_string(), workspace_root);
            let symbols = ext.extract_symbols(tree);
            let relationships = ext.extract_relationships(tree, &symbols);
            let identifiers = ext.extract_identifiers(tree, &symbols);
            let types = ext.infer_types(&symbols);
            let pending = ext.get_pending_relationships();
            Ok(ExtractionResults { symbols, relationships, pending_relationships: pending, identifiers, types: convert_types_map(types, language) })
        }
        "dart" => {
            let mut ext = crate::dart::DartExtractor::new(language.to_string(), file_path.to_string(), content.to_string(), workspace_root);
            let symbols = ext.extract_symbols(tree);
            let relationships = ext.extract_relationships(tree, &symbols);
            let identifiers = ext.extract_identifiers(tree, &symbols);
            let types = ext.infer_types(&symbols);
            let pending = ext.get_pending_relationships();
            Ok(ExtractionResults { symbols, relationships, pending_relationships: pending, identifiers, types: convert_types_map(types, language) })
        }
        "go" => {
            let mut ext = crate::go::GoExtractor::new(language.to_string(), file_path.to_string(), content.to_string(), workspace_root);
            let symbols = ext.extract_symbols(tree);
            let relationships = ext.extract_relationships(tree, &symbols);
            let identifiers = ext.extract_identifiers(tree, &symbols);
            let types = ext.infer_types(&symbols);
            let pending = ext.get_pending_relationships();
            Ok(ExtractionResults { symbols, relationships, pending_relationships: pending, identifiers, types: convert_types_map(types, language) })
        }
        "c" => {
            let mut ext = crate::c::CExtractor::new(language.to_string(), file_path.to_string(), content.to_string(), workspace_root);
            let symbols = ext.extract_symbols(tree);
            let relationships = ext.extract_relationships(tree, &symbols);
            let identifiers = ext.extract_identifiers(tree, &symbols);
            let types = ext.infer_types(&symbols);
            let pending = ext.get_pending_relationships();
            Ok(ExtractionResults { symbols, relationships, pending_relationships: pending, identifiers, types: convert_types_map(types, language) })
        }
        "cpp" => {
            let mut ext = crate::cpp::CppExtractor::new(file_path.to_string(), content.to_string(), workspace_root);
            let symbols = ext.extract_symbols(tree);
            let relationships = ext.extract_relationships(tree, &symbols);
            let identifiers = ext.extract_identifiers(tree, &symbols);
            let types = ext.infer_types(&symbols);
            let pending = ext.get_pending_relationships();
            Ok(ExtractionResults { symbols, relationships, pending_relationships: pending, identifiers, types: convert_types_map(types, "cpp") })
        }
        "powershell" => {
            let mut ext = crate::powershell::PowerShellExtractor::new(language.to_string(), file_path.to_string(), content.to_string(), workspace_root);
            let symbols = ext.extract_symbols(tree);
            let relationships = ext.extract_relationships(tree, &symbols);
            let identifiers = ext.extract_identifiers(tree, &symbols);
            let types = ext.infer_types(&symbols);
            let pending = ext.get_pending_relationships();
            Ok(ExtractionResults { symbols, relationships, pending_relationships: pending, identifiers, types: convert_types_map(types, language) })
        }
        "bash" => {
            let mut ext = crate::bash::BashExtractor::new(language.to_string(), file_path.to_string(), content.to_string(), workspace_root);
            let symbols = ext.extract_symbols(tree);
            let relationships = ext.extract_relationships(tree, &symbols);
            let identifiers = ext.extract_identifiers(tree, &symbols);
            let types = ext.infer_types(&symbols);
            let pending = ext.get_pending_relationships();
            Ok(ExtractionResults { symbols, relationships, pending_relationships: pending, identifiers, types: convert_types_map(types, language) })
        }
        "zig" => {
            let mut ext = crate::zig::ZigExtractor::new(language.to_string(), file_path.to_string(), content.to_string(), workspace_root);
            let symbols = ext.extract_symbols(tree);
            let relationships = ext.extract_relationships(tree, &symbols);
            let identifiers = ext.extract_identifiers(tree, &symbols);
            let types = ext.infer_types(&symbols);
            let pending = ext.get_pending_relationships();
            Ok(ExtractionResults { symbols, relationships, pending_relationships: pending, identifiers, types: convert_types_map(types, language) })
        }

        // ─── Languages with rels + identifiers + types but no pending relationships ───

        "sql" => {
            let mut ext = crate::sql::SqlExtractor::new(language.to_string(), file_path.to_string(), content.to_string(), workspace_root);
            let symbols = ext.extract_symbols(tree);
            let relationships = ext.extract_relationships(tree, &symbols);
            let identifiers = ext.extract_identifiers(tree, &symbols);
            let types = ext.infer_types(&symbols);
            Ok(ExtractionResults { symbols, relationships, pending_relationships: Vec::new(), identifiers, types: convert_types_map(types, language) })
        }
        "html" => {
            let mut ext = crate::html::HTMLExtractor::new(language.to_string(), file_path.to_string(), content.to_string(), workspace_root);
            let symbols = ext.extract_symbols(tree);
            let relationships = ext.extract_relationships(tree, &symbols);
            let identifiers = ext.extract_identifiers(tree, &symbols);
            let types = ext.infer_types(&symbols);
            Ok(ExtractionResults { symbols, relationships, pending_relationships: Vec::new(), identifiers, types: convert_types_map(types, language) })
        }
        "razor" => {
            let mut ext = crate::razor::RazorExtractor::new(language.to_string(), file_path.to_string(), content.to_string(), workspace_root);
            let symbols = ext.extract_symbols(tree);
            let relationships = ext.extract_relationships(tree, &symbols);
            let identifiers = ext.extract_identifiers(tree, &symbols);
            let types = ext.infer_types(&symbols);
            Ok(ExtractionResults { symbols, relationships, pending_relationships: Vec::new(), identifiers, types: convert_types_map(types, language) })
        }
        "regex" => {
            let mut ext = crate::regex::RegexExtractor::new(language.to_string(), file_path.to_string(), content.to_string(), workspace_root);
            let symbols = ext.extract_symbols(tree);
            let relationships = ext.extract_relationships(tree, &symbols);
            let identifiers = ext.extract_identifiers(tree, &symbols);
            let types = ext.infer_types(&symbols);
            Ok(ExtractionResults { symbols, relationships, pending_relationships: Vec::new(), identifiers, types: convert_types_map(types, language) })
        }

        // ─── Vue (unique signatures: Option<&Tree>, no tree param for identifiers) ───

        "vue" => {
            let mut ext = crate::vue::VueExtractor::new(language.to_string(), file_path.to_string(), content.to_string(), workspace_root);
            let symbols = ext.extract_symbols(Some(tree));
            let relationships = ext.extract_relationships(Some(tree), &symbols);
            let identifiers = ext.extract_identifiers(&symbols);
            let types = ext.infer_types(&symbols);
            Ok(ExtractionResults { symbols, relationships, pending_relationships: Vec::new(), identifiers, types: convert_types_map(types, language) })
        }

        // ─── Languages with rels + pending + identifiers but no types ───

        "ruby" => {
            let mut ext = crate::ruby::RubyExtractor::new(file_path.to_string(), content.to_string(), workspace_root);
            let symbols = ext.extract_symbols(tree);
            let relationships = ext.extract_relationships(tree, &symbols);
            let identifiers = ext.extract_identifiers(tree, &symbols);
            let types = convert_types_map(ext.infer_types(&symbols), language);
            let pending = ext.get_pending_relationships();
            Ok(ExtractionResults { symbols, relationships, pending_relationships: pending, identifiers, types })
        }
        "lua" => {
            let mut ext = crate::lua::LuaExtractor::new(language.to_string(), file_path.to_string(), content.to_string(), workspace_root);
            let symbols = ext.extract_symbols(tree);
            let relationships = ext.extract_relationships(tree, &symbols);
            let identifiers = ext.extract_identifiers(tree, &symbols);
            let pending = ext.get_pending_relationships();
            Ok(ExtractionResults { symbols, relationships, pending_relationships: pending, identifiers, types: HashMap::new() })
        }
        "gdscript" => {
            let mut ext = crate::gdscript::GDScriptExtractor::new(language.to_string(), file_path.to_string(), content.to_string(), workspace_root);
            let symbols = ext.extract_symbols(tree);
            let relationships = ext.extract_relationships(tree, &symbols);
            let identifiers = ext.extract_identifiers(tree, &symbols);
            let types = convert_types_map(ext.infer_types(&symbols), language);
            let pending = ext.get_pending_relationships();
            Ok(ExtractionResults { symbols, relationships, pending_relationships: pending, identifiers, types })
        }
        "qml" => {
            let mut ext = crate::qml::QmlExtractor::new(language.to_string(), file_path.to_string(), content.to_string(), workspace_root);
            let symbols = ext.extract_symbols(tree);
            let relationships = ext.extract_relationships(tree, &symbols);
            let identifiers = ext.extract_identifiers(tree, &symbols);
            let pending = ext.get_pending_relationships();
            Ok(ExtractionResults { symbols, relationships, pending_relationships: pending, identifiers, types: HashMap::new() })
        }
        "r" => {
            let mut ext = crate::r::RExtractor::new(language.to_string(), file_path.to_string(), content.to_string(), workspace_root);
            let symbols = ext.extract_symbols(tree);
            let relationships = ext.extract_relationships(tree, &symbols);
            let identifiers = ext.extract_identifiers(tree, &symbols);
            let pending = ext.get_pending_relationships();
            Ok(ExtractionResults { symbols, relationships, pending_relationships: pending, identifiers, types: HashMap::new() })
        }

        // ─── Data/markup languages (symbols + identifiers only) ───

        "css" => {
            let mut ext = crate::css::CSSExtractor::new(language.to_string(), file_path.to_string(), content.to_string(), workspace_root);
            let symbols = ext.extract_symbols(tree);
            let identifiers = ext.extract_identifiers(tree, &symbols);
            Ok(ExtractionResults { symbols, relationships: Vec::new(), pending_relationships: Vec::new(), identifiers, types: HashMap::new() })
        }
        "markdown" => {
            let mut ext = crate::markdown::MarkdownExtractor::new(language.to_string(), file_path.to_string(), content.to_string(), workspace_root);
            let symbols = ext.extract_symbols(tree);
            let identifiers = ext.extract_identifiers(tree, &symbols);
            Ok(ExtractionResults { symbols, relationships: Vec::new(), pending_relationships: Vec::new(), identifiers, types: HashMap::new() })
        }
        "json" => {
            let mut ext = crate::json::JsonExtractor::new(language.to_string(), file_path.to_string(), content.to_string(), workspace_root);
            let symbols = ext.extract_symbols(tree);
            let identifiers = ext.extract_identifiers(tree, &symbols);
            Ok(ExtractionResults { symbols, relationships: Vec::new(), pending_relationships: Vec::new(), identifiers, types: HashMap::new() })
        }
        "toml" => {
            let mut ext = crate::toml::TomlExtractor::new(language.to_string(), file_path.to_string(), content.to_string(), workspace_root);
            let symbols = ext.extract_symbols(tree);
            let identifiers = ext.extract_identifiers(tree, &symbols);
            Ok(ExtractionResults { symbols, relationships: Vec::new(), pending_relationships: Vec::new(), identifiers, types: HashMap::new() })
        }
        "yaml" => {
            let mut ext = crate::yaml::YamlExtractor::new(language.to_string(), file_path.to_string(), content.to_string(), workspace_root);
            let symbols = ext.extract_symbols(tree);
            let identifiers = ext.extract_identifiers(tree, &symbols);
            Ok(ExtractionResults { symbols, relationships: Vec::new(), pending_relationships: Vec::new(), identifiers, types: HashMap::new() })
        }

        _ => Err(anyhow!(
            "No extractor available for language '{}' (file: {})",
            language,
            file_path
        )),
    }
}

#[cfg(test)]
mod factory_consistency_tests {
    use super::*;
    use std::path::PathBuf;
    use tree_sitter::Parser;

    /// Test that ALL supported languages work with the factory function
    ///
    /// This test prevents the R/QML/PHP bug from happening again by ensuring
    /// every language in supported_languages() can be extracted via the factory.
    #[test]
    fn test_all_languages_in_factory() {
        let manager = crate::ExtractorManager::new();
        let supported = manager.supported_languages();

        // Verify we have all 27 languages
        assert_eq!(supported.len(), 29, "Expected 29 language entries (27 languages, 2 with aliases)");

        let workspace_root = PathBuf::from("/tmp/test");

        // Test each language can be handled by the factory
        // Note: Some will fail to parse invalid code, but they should NOT return
        // "No extractor available" error
        for language in &supported {
            let test_content = "// test";

            // Create a minimal valid tree for testing
            let mut parser = Parser::new();
            let ts_lang = match crate::language::get_tree_sitter_language(language) {
                Ok(lang) => lang,
                Err(_) => continue, // Skip if language not available
            };

            parser.set_language(&ts_lang).unwrap();
            let tree = parser.parse(test_content, None).unwrap();

            // The factory should handle this language (even if it extracts 0 symbols)
            let result = extract_symbols_and_relationships(
                &tree,
                "test.rs",
                test_content,
                language,
                &workspace_root,
            );

            // Should succeed OR fail for parsing reasons, but NEVER "No extractor available"
            if let Err(e) = result {
                let error_msg = format!("{}", e);
                assert!(
                    !error_msg.contains("No extractor available"),
                    "Language '{}' is missing from factory function! Error: {}",
                    language,
                    error_msg
                );
            }
        }
    }

    /// Test that the factory function rejects unknown languages
    #[test]
    fn test_factory_rejects_unknown_language() {
        let workspace_root = PathBuf::from("/tmp/test");
        let mut parser = Parser::new();

        // Use Rust parser for a fake language
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

        // Parse the code
        let mut parser = Parser::new();
        let language = tree_sitter_python::LANGUAGE;
        parser.set_language(&language.into()).unwrap();
        let tree = parser.parse(code, None).unwrap();

        // Call the factory
        let results = crate::factory::extract_symbols_and_relationships(
            &tree,
            "test.py",
            code,
            "python",
            &workspace_root,
        ).unwrap();

        assert!(results.symbols.len() > 0, "Should extract symbols");
        assert!(results.identifiers.len() > 0, "Factory should return identifiers from Python code!");
    }
}
