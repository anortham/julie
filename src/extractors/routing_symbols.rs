//! Routing for symbol extraction - delegates to language-specific extractors

use crate::extractors::base::Symbol;
use std::path::Path;

/// Route symbol extraction to the appropriate language extractor
pub(crate) fn extract_symbols_for_language(
    file_path: &str,
    content: &str,
    language: &str,
    tree: &tree_sitter::Tree,
    workspace_root: &Path,
) -> Result<Vec<Symbol>, anyhow::Error> {
    match language {
        "rust" => {
            let mut extractor = crate::extractors::rust::RustExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "typescript" | "tsx" => {
            let mut extractor = crate::extractors::typescript::TypeScriptExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "javascript" | "jsx" => {
            let mut extractor = crate::extractors::javascript::JavaScriptExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "python" => {
            let mut extractor = crate::extractors::python::PythonExtractor::new(
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "go" => {
            let mut extractor = crate::extractors::go::GoExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "java" => {
            let mut extractor = crate::extractors::java::JavaExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "c" => {
            let mut extractor = crate::extractors::c::CExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "cpp" => {
            let mut extractor = crate::extractors::cpp::CppExtractor::new(
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "csharp" => {
            let mut extractor = crate::extractors::csharp::CSharpExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "ruby" => {
            let mut extractor = crate::extractors::ruby::RubyExtractor::new(
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "php" => {
            let mut extractor = crate::extractors::php::PhpExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "swift" => {
            let mut extractor = crate::extractors::swift::SwiftExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "kotlin" => {
            let mut extractor = crate::extractors::kotlin::KotlinExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "dart" => {
            let mut extractor = crate::extractors::dart::DartExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "gdscript" => {
            let mut extractor = crate::extractors::gdscript::GDScriptExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "lua" => {
            let mut extractor = crate::extractors::lua::LuaExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "qml" => {
            let mut extractor = crate::extractors::qml::QmlExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "r" => {
            let mut extractor = crate::extractors::r::RExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "vue" => {
            let mut extractor = crate::extractors::vue::VueExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(Some(tree)))
        }
        "razor" => {
            let mut extractor = crate::extractors::razor::RazorExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "sql" => {
            let mut extractor = crate::extractors::sql::SqlExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "html" => {
            let mut extractor = crate::extractors::html::HTMLExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "css" => {
            let mut extractor = crate::extractors::css::CSSExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "bash" => {
            let mut extractor = crate::extractors::bash::BashExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "powershell" => {
            let mut extractor = crate::extractors::powershell::PowerShellExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "zig" => {
            let mut extractor = crate::extractors::zig::ZigExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "regex" => {
            let mut extractor = crate::extractors::regex::RegexExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "markdown" => {
            let mut extractor = crate::extractors::markdown::MarkdownExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "json" => {
            let mut extractor = crate::extractors::json::JsonExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "toml" => {
            let mut extractor = crate::extractors::toml::TomlExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "yaml" => {
            let mut extractor = crate::extractors::yaml::YamlExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        _ => {
            tracing::debug!(
                "No extractor available for language: {} (file: {})",
                language,
                file_path
            );
            Ok(Vec::new())
        }
    }
}
