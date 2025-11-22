//! Routing for symbol extraction - delegates to language-specific extractors

use crate::base::Symbol;
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
            let mut extractor = crate::rust::RustExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "typescript" | "tsx" => {
            let mut extractor = crate::typescript::TypeScriptExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "javascript" | "jsx" => {
            let mut extractor = crate::javascript::JavaScriptExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "python" => {
            let mut extractor = crate::python::PythonExtractor::new(
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "go" => {
            let mut extractor = crate::go::GoExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "java" => {
            let mut extractor = crate::java::JavaExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "c" => {
            let mut extractor = crate::c::CExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "cpp" => {
            let mut extractor = crate::cpp::CppExtractor::new(
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "csharp" => {
            let mut extractor = crate::csharp::CSharpExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "ruby" => {
            let mut extractor = crate::ruby::RubyExtractor::new(
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "php" => {
            let mut extractor = crate::php::PhpExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "swift" => {
            let mut extractor = crate::swift::SwiftExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "kotlin" => {
            let mut extractor = crate::kotlin::KotlinExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "dart" => {
            let mut extractor = crate::dart::DartExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "gdscript" => {
            let mut extractor = crate::gdscript::GDScriptExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "lua" => {
            let mut extractor = crate::lua::LuaExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "qml" => {
            let mut extractor = crate::qml::QmlExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "r" => {
            let mut extractor = crate::r::RExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "vue" => {
            let mut extractor = crate::vue::VueExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(Some(tree)))
        }
        "razor" => {
            let mut extractor = crate::razor::RazorExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "sql" => {
            let mut extractor = crate::sql::SqlExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "html" => {
            let mut extractor = crate::html::HTMLExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "css" => {
            let mut extractor = crate::css::CSSExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "bash" => {
            let mut extractor = crate::bash::BashExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "powershell" => {
            let mut extractor = crate::powershell::PowerShellExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "zig" => {
            let mut extractor = crate::zig::ZigExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "regex" => {
            let mut extractor = crate::regex::RegexExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "markdown" => {
            let mut extractor = crate::markdown::MarkdownExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "json" => {
            let mut extractor = crate::json::JsonExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "toml" => {
            let mut extractor = crate::toml::TomlExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            Ok(extractor.extract_symbols(tree))
        }
        "yaml" => {
            let mut extractor = crate::yaml::YamlExtractor::new(
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
