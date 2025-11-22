//! Routing for identifier extraction - delegates to language-specific extractors

use crate::base::{Identifier, Symbol};

/// Route identifier extraction to the appropriate language extractor
pub(crate) fn extract_identifiers_for_language(
    file_path: &str,
    content: &str,
    language: &str,
    tree: &tree_sitter::Tree,
    symbols: &[Symbol],
) -> Result<Vec<Identifier>, anyhow::Error> {
    let tmp_path = std::path::PathBuf::from("/tmp/test");

    match language {
        "rust" => {
            let mut extractor = crate::rust::RustExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_identifiers(tree, symbols))
        }
        "csharp" => {
            let mut extractor = crate::csharp::CSharpExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_identifiers(tree, symbols))
        }
        "python" => {
            let mut extractor = crate::python::PythonExtractor::new(
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_identifiers(tree, symbols))
        }
        "javascript" | "jsx" => {
            let mut extractor = crate::javascript::JavaScriptExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_identifiers(tree, symbols))
        }
        "typescript" | "tsx" => {
            let mut extractor = crate::typescript::TypeScriptExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_identifiers(tree, symbols))
        }
        "java" => {
            let mut extractor = crate::java::JavaExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_identifiers(tree, symbols))
        }
        "go" => {
            let mut extractor = crate::go::GoExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_identifiers(tree, symbols))
        }
        "swift" => {
            let mut extractor = crate::swift::SwiftExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_identifiers(tree, symbols))
        }
        "ruby" => {
            let mut extractor = crate::ruby::RubyExtractor::new(
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_identifiers(tree, symbols))
        }
        "php" => {
            let mut extractor = crate::php::PhpExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_identifiers(tree, symbols))
        }
        "kotlin" => {
            let mut extractor = crate::kotlin::KotlinExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_identifiers(tree, symbols))
        }
        "c" => {
            let mut extractor = crate::c::CExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_identifiers(tree, symbols))
        }
        "cpp" => {
            let mut extractor = crate::cpp::CppExtractor::new(
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_identifiers(tree, symbols))
        }
        "bash" => {
            let mut extractor = crate::bash::BashExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_identifiers(tree, symbols))
        }
        "lua" => {
            let mut extractor = crate::lua::LuaExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_identifiers(tree, symbols))
        }
        "gdscript" => {
            let mut extractor = crate::gdscript::GDScriptExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_identifiers(tree, symbols))
        }
        "vue" => {
            let mut extractor = crate::vue::VueExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_identifiers(symbols))
        }
        "razor" => {
            let mut extractor = crate::razor::RazorExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_identifiers(tree, symbols))
        }
        "zig" => {
            let mut extractor = crate::zig::ZigExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_identifiers(tree, symbols))
        }
        "dart" => {
            let mut extractor = crate::dart::DartExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_identifiers(tree, symbols))
        }
        "sql" => {
            let mut extractor = crate::sql::SqlExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_identifiers(tree, symbols))
        }
        "html" => {
            let mut extractor = crate::html::HTMLExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_identifiers(tree, symbols))
        }
        "css" => {
            let mut extractor = crate::css::CSSExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_identifiers(tree, symbols))
        }
        "powershell" => {
            let mut extractor = crate::powershell::PowerShellExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_identifiers(tree, symbols))
        }
        "regex" => {
            let mut extractor = crate::regex::RegexExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_identifiers(tree, symbols))
        }
        _ => {
            tracing::debug!(
                "No identifier extraction available for language: {} (file: {})",
                language,
                file_path
            );
            Ok(Vec::new())
        }
    }
}
