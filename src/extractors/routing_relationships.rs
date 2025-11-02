//! Routing for relationship extraction - delegates to language-specific extractors

use crate::extractors::base::{Relationship, Symbol};

/// Route relationship extraction to the appropriate language extractor
pub(crate) fn extract_relationships_for_language(
    file_path: &str,
    content: &str,
    language: &str,
    tree: &tree_sitter::Tree,
    symbols: &[Symbol],
) -> Result<Vec<Relationship>, anyhow::Error> {
    let tmp_path = std::path::PathBuf::from("/tmp/test");

    match language {
        "rust" => {
            let mut extractor = crate::extractors::rust::RustExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_relationships(tree, symbols))
        }
        "csharp" => {
            let mut extractor = crate::extractors::csharp::CSharpExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_relationships(tree, symbols))
        }
        "python" => {
            let mut extractor = crate::extractors::python::PythonExtractor::new(
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_relationships(tree, symbols))
        }
        "javascript" | "jsx" => {
            let mut extractor = crate::extractors::javascript::JavaScriptExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_relationships(tree, symbols))
        }
        "typescript" | "tsx" => {
            let mut extractor = crate::extractors::typescript::TypeScriptExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_relationships(tree, symbols))
        }
        "java" => {
            let mut extractor = crate::extractors::java::JavaExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_relationships(tree, symbols))
        }
        "go" => {
            let mut extractor = crate::extractors::go::GoExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_relationships(tree, symbols))
        }
        "swift" => {
            let mut extractor = crate::extractors::swift::SwiftExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_relationships(tree, symbols))
        }
        "ruby" => {
            let extractor = crate::extractors::ruby::RubyExtractor::new(
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_relationships(tree, symbols))
        }
        "php" => {
            let mut extractor = crate::extractors::php::PhpExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_relationships(tree, symbols))
        }
        "kotlin" => {
            let mut extractor = crate::extractors::kotlin::KotlinExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_relationships(tree, symbols))
        }
        "c" => {
            let mut extractor = crate::extractors::c::CExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_relationships(tree, symbols))
        }
        "cpp" => {
            let mut extractor = crate::extractors::cpp::CppExtractor::new(
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_relationships(tree, symbols))
        }
        "bash" => {
            let mut extractor = crate::extractors::bash::BashExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_relationships(tree, symbols))
        }
        "lua" => {
            let mut extractor = crate::extractors::lua::LuaExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_relationships(tree, symbols))
        }
        "gdscript" => {
            let mut extractor = crate::extractors::gdscript::GDScriptExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_relationships(tree, symbols))
        }
        "vue" => {
            let mut extractor = crate::extractors::vue::VueExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_relationships(Some(tree), symbols))
        }
        "razor" => {
            let mut extractor = crate::extractors::razor::RazorExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_relationships(tree, symbols))
        }
        "zig" => {
            let mut extractor = crate::extractors::zig::ZigExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_relationships(tree, symbols))
        }
        "dart" => {
            let mut extractor = crate::extractors::dart::DartExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_relationships(tree, symbols))
        }
        "sql" => {
            let mut extractor = crate::extractors::sql::SqlExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_relationships(tree, symbols))
        }
        "html" => {
            let mut extractor = crate::extractors::html::HTMLExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_relationships(tree, symbols))
        }
        "css" => {
            // CSS doesn't have relationships
            Ok(Vec::new())
        }
        "powershell" => {
            let mut extractor = crate::extractors::powershell::PowerShellExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_relationships(tree, symbols))
        }
        "regex" => {
            let mut extractor = crate::extractors::regex::RegexExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                &tmp_path,
            );
            Ok(extractor.extract_relationships(tree, symbols))
        }
        _ => {
            tracing::debug!(
                "No relationship extraction available for language: {} (file: {})",
                language,
                file_path
            );
            Ok(Vec::new())
        }
    }
}
