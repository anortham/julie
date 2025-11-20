//! Shared extractor factory - Single source of truth for all 27 languages
//!
//! This module provides the centralized factory function for all language extractors.
//! It ensures consistency across the codebase and prevents bugs from missing languages
//! in different code paths.

use crate::extractors::base::{ExtractionResults, TypeInfo};
use anyhow::anyhow;
use std::collections::HashMap;
use std::path::Path;

/// Extract symbols and relationships for ANY supported language
///
/// This is the centralized factory function for all 27 language extractors.
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
/// `Ok((symbols, relationships))` on success, or error if extraction fails
///
/// # Example
/// ```rust
/// let (symbols, rels) = extract_symbols_and_relationships(
///     &tree, "src/main.rs", &content, "rust", workspace_root
/// )?;
/// ```
pub fn extract_symbols_and_relationships(
    tree: &tree_sitter::Tree,
    file_path: &str,
    content: &str,
    language: &str,
    workspace_root: &Path,
) -> Result<ExtractionResults, anyhow::Error> {
    // Single match statement for ALL 27 languages
    match language {
        "rust" => {
            let mut extractor = crate::extractors::rust::RustExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            let _identifiers = extractor.extract_identifiers(tree, &symbols);
            let _types = extractor.infer_types(&symbols);

            Ok(ExtractionResults {
                symbols,
                relationships,
                identifiers: _identifiers,
                types: _types
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
                    .collect(),
            })
        }
        "typescript" | "tsx" => {
            let mut extractor = crate::extractors::typescript::TypeScriptExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            let _identifiers = extractor.extract_identifiers(tree, &symbols);
            let _types = extractor.infer_types(&symbols);
            Ok(ExtractionResults {
                symbols,
                relationships,
                identifiers: _identifiers,
                types: _types
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
                    .collect(),
            })
        }
        "javascript" | "jsx" => {
            let mut extractor = crate::extractors::javascript::JavaScriptExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            let _identifiers = extractor.extract_identifiers(tree, &symbols);
            let _types = extractor.infer_types(&symbols);

            Ok(ExtractionResults {
                symbols,
                relationships,
                identifiers: _identifiers,
                types: _types
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
                    .collect(),
            })
        }
        "python" => {
            let mut extractor = crate::extractors::python::PythonExtractor::new(
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            let _identifiers = extractor.extract_identifiers(tree, &symbols);
            let _types = extractor.infer_types(&symbols);

            // DEBUG: Log what we extracted
            if !_identifiers.is_empty() || !_types.is_empty() {
                eprintln!(
                    "🔍 PYTHON {}: {} identifiers, {} types",
                    file_path,
                    _identifiers.len(),
                    _types.len()
                );
            }

            Ok(ExtractionResults {
                symbols,
                relationships,
                identifiers: _identifiers,
                types: _types
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
                    .collect(),
            })
        }
        "java" => {
            let mut extractor = crate::extractors::java::JavaExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            let _identifiers = extractor.extract_identifiers(tree, &symbols);
            let _types = extractor.infer_types(&symbols);
            Ok(ExtractionResults {
                symbols,
                relationships,
                identifiers: _identifiers,
                types: _types
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
                    .collect(),
            })
        }
        "csharp" => {
            let mut extractor = crate::extractors::csharp::CSharpExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            let _identifiers = extractor.extract_identifiers(tree, &symbols);
            let _types = extractor.infer_types(&symbols);
            Ok(ExtractionResults {
                symbols,
                relationships,
                identifiers: _identifiers,
                types: _types
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
                    .collect(),
            })
        }
        "php" => {
            let mut extractor = crate::extractors::php::PhpExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            let _identifiers = extractor.extract_identifiers(tree, &symbols);
            let _types = extractor.infer_types(&symbols);
            Ok(ExtractionResults {
                symbols,
                relationships,
                identifiers: _identifiers,
                types: _types
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
                    .collect(),
            })
        }
        "ruby" => {
            let mut extractor = crate::extractors::ruby::RubyExtractor::new(
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            let _identifiers = extractor.extract_identifiers(tree, &symbols);
            Ok(ExtractionResults {
                symbols,
                relationships,
                identifiers: _identifiers,
                types: HashMap::new(),
            })
        }
        "swift" => {
            let mut extractor = crate::extractors::swift::SwiftExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            let _identifiers = extractor.extract_identifiers(tree, &symbols);
            let _types = extractor.infer_types(&symbols);
            Ok(ExtractionResults {
                symbols,
                relationships,
                identifiers: _identifiers,
                types: _types
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
                    .collect(),
            })
        }
        "kotlin" => {
            let mut extractor = crate::extractors::kotlin::KotlinExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            let _identifiers = extractor.extract_identifiers(tree, &symbols);
            let _types = extractor.infer_types(&symbols);
            Ok(ExtractionResults {
                symbols,
                relationships,
                identifiers: _identifiers,
                types: _types
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
                    .collect(),
            })
        }
        "dart" => {
            let mut extractor = crate::extractors::dart::DartExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            let _identifiers = extractor.extract_identifiers(tree, &symbols);
            let _types = extractor.infer_types(&symbols);
            Ok(ExtractionResults {
                symbols,
                relationships,
                identifiers: _identifiers,
                types: _types
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
                    .collect(),
            })
        }
        "go" => {
            let mut extractor = crate::extractors::go::GoExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            let _identifiers = extractor.extract_identifiers(tree, &symbols);
            let _types = extractor.infer_types(&symbols);
            Ok(ExtractionResults {
                symbols,
                relationships,
                identifiers: _identifiers,
                types: _types
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
                    .collect(),
            })
        }
        "c" => {
            let mut extractor = crate::extractors::c::CExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            let _identifiers = extractor.extract_identifiers(tree, &symbols);
            let _types = extractor.infer_types(&symbols);

            Ok(ExtractionResults {
                symbols,
                relationships,
                identifiers: _identifiers,
                types: _types
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
                    .collect(),
            })
        }
        "cpp" => {
            let mut extractor = crate::extractors::cpp::CppExtractor::new(
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            let _identifiers = extractor.extract_identifiers(tree, &symbols);
            let _types = extractor.infer_types(&symbols);
            Ok(ExtractionResults {
                symbols,
                relationships,
                identifiers: _identifiers,
                types: _types
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
                                language: "cpp".to_string(),
                                metadata: None,
                            },
                        )
                    })
                    .collect(),
            })
        }
        "lua" => {
            let mut extractor = crate::extractors::lua::LuaExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            let _identifiers = extractor.extract_identifiers(tree, &symbols);
            Ok(ExtractionResults {
                symbols,
                relationships,
                identifiers: _identifiers,
                types: HashMap::new(),
            })
        }
        "qml" => {
            let mut extractor = crate::extractors::qml::QmlExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            let _identifiers = extractor.extract_identifiers(tree, &symbols);
            Ok(ExtractionResults {
                symbols,
                relationships,
                identifiers: _identifiers,
                types: HashMap::new(),
            })
        }
        "r" => {
            let mut extractor = crate::extractors::r::RExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            let _identifiers = extractor.extract_identifiers(tree, &symbols);
            Ok(ExtractionResults {
                symbols,
                relationships,
                identifiers: _identifiers,
                types: HashMap::new(),
            })
        }
        "sql" => {
            let mut extractor = crate::extractors::sql::SqlExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            let _identifiers = extractor.extract_identifiers(tree, &symbols);
            let _types = extractor.infer_types(&symbols);
            Ok(ExtractionResults {
                symbols,
                relationships,
                identifiers: _identifiers,
                types: _types
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
                    .collect(),
            })
        }
        "html" => {
            let mut extractor = crate::extractors::html::HTMLExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            let _identifiers = extractor.extract_identifiers(tree, &symbols);
            let _types = extractor.infer_types(&symbols);
            Ok(ExtractionResults {
                symbols,
                relationships,
                identifiers: _identifiers,
                types: _types
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
                    .collect(),
            })
        }
        "css" => {
            let mut extractor = crate::extractors::css::CSSExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let _identifiers = extractor.extract_identifiers(tree, &symbols); // CSSExtractor doesn't have extract_relationships method yet

            Ok(ExtractionResults {
                symbols,

                relationships: Vec::new(),

                identifiers: _identifiers,

                types: HashMap::new(),
            })
        }
        "vue" => {
            let mut extractor = crate::extractors::vue::VueExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(Some(tree));
            let relationships = extractor.extract_relationships(Some(tree), &symbols);
            let _identifiers = extractor.extract_identifiers(&symbols);
            let _types = extractor.infer_types(&symbols);
            Ok(ExtractionResults {
                symbols,
                relationships,
                identifiers: _identifiers,
                types: _types
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
                    .collect(),
            })
        }
        "razor" => {
            let mut extractor = crate::extractors::razor::RazorExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            let _identifiers = extractor.extract_identifiers(tree, &symbols);
            let _types = extractor.infer_types(&symbols);
            Ok(ExtractionResults {
                symbols,
                relationships,
                identifiers: _identifiers,
                types: _types
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
                    .collect(),
            })
        }
        "bash" => {
            let mut extractor = crate::extractors::bash::BashExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            let _identifiers = extractor.extract_identifiers(tree, &symbols);
            let _types = extractor.infer_types(&symbols);
            Ok(ExtractionResults {
                symbols,
                relationships,
                identifiers: _identifiers,
                types: _types
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
                    .collect(),
            })
        }
        "powershell" => {
            let mut extractor = crate::extractors::powershell::PowerShellExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            let _identifiers = extractor.extract_identifiers(tree, &symbols);
            let _types = extractor.infer_types(&symbols);
            Ok(ExtractionResults {
                symbols,
                relationships,
                identifiers: _identifiers,
                types: _types
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
                    .collect(),
            })
        }
        "gdscript" => {
            let mut extractor = crate::extractors::gdscript::GDScriptExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            let _identifiers = extractor.extract_identifiers(tree, &symbols);
            Ok(ExtractionResults {
                symbols,
                relationships,
                identifiers: _identifiers,
                types: HashMap::new(),
            })
        }
        "zig" => {
            let mut extractor = crate::extractors::zig::ZigExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            let _identifiers = extractor.extract_identifiers(tree, &symbols);
            let _types = extractor.infer_types(&symbols);
            Ok(ExtractionResults {
                symbols,
                relationships,
                identifiers: _identifiers,
                types: _types
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
                    .collect(),
            })
        }
        "regex" => {
            let mut extractor = crate::extractors::regex::RegexExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            let _identifiers = extractor.extract_identifiers(tree, &symbols);
            let _types = extractor.infer_types(&symbols);
            Ok(ExtractionResults {
                symbols,
                relationships,
                identifiers: _identifiers,
                types: _types
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
                    .collect(),
            })
        }
        "markdown" => {
            let mut extractor = crate::extractors::markdown::MarkdownExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let _identifiers = extractor.extract_identifiers(tree, &symbols); // Markdown is documentation - no code relationships

            Ok(ExtractionResults {
                symbols,

                relationships: Vec::new(),

                identifiers: _identifiers,

                types: HashMap::new(),
            })
        }
        "json" => {
            let mut extractor = crate::extractors::json::JsonExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let _identifiers = extractor.extract_identifiers(tree, &symbols);
            // JSON is configuration data - no code relationships

            Ok(ExtractionResults {
                symbols,

                relationships: Vec::new(),

                identifiers: _identifiers,

                types: HashMap::new(),
            })
        }
        "toml" => {
            let mut extractor = crate::extractors::toml::TomlExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let _identifiers = extractor.extract_identifiers(tree, &symbols);
            // TOML is configuration data - no code relationships

            Ok(ExtractionResults {
                symbols,

                relationships: Vec::new(),

                identifiers: _identifiers,

                types: HashMap::new(),
            })
        }
        "yaml" => {
            let mut extractor = crate::extractors::yaml::YamlExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let _identifiers = extractor.extract_identifiers(tree, &symbols);
            // YAML is configuration data - no code relationships

            Ok(ExtractionResults {
                symbols,

                relationships: Vec::new(),

                identifiers: _identifiers,

                types: HashMap::new(),
            })
        }

        _ => {
            return Err(anyhow!(
                "No extractor available for language '{}' (file: {})",
                language,
                file_path
            ));
        }
    }
}

#[cfg(test)]
mod factory_consistency_tests {
    use super::*;
    use std::path::PathBuf;
    use tree_sitter::Parser;

    /// Test that ALL 27 supported languages work with the factory function
    ///
    /// This test prevents the R/QML/PHP bug from happening again by ensuring
    /// every language in supported_languages() can be extracted via the factory.
    #[test]
    fn test_all_languages_in_factory() {
        let manager = crate::extractors::ExtractorManager::new();
        let supported = manager.supported_languages();

        // Verify we have all 27 languages
        assert_eq!(
            supported.len(),
            29,
            "Expected 29 language entries (27 languages, 2 with aliases)"
        );

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
        let results = crate::extractors::factory::extract_symbols_and_relationships(
            &tree,
            "test.py",
            code,
            "python",
            &workspace_root,
        )
        .unwrap();

        // Assert we got identifiers
        println!("Symbols: {}", results.symbols.len());
        println!("Identifiers: {}", results.identifiers.len());
        println!("Types: {}", results.types.len());

        assert!(results.symbols.len() > 0, "Should extract symbols");
        assert!(
            results.identifiers.len() > 0,
            "Factory should return identifiers from Python code!"
        );
    }
}
