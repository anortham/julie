use crate::base::ExtractionResults;
use crate::factory::convert_types_map;
use anyhow::anyhow;
use std::collections::HashMap;
use std::path::Path;
use tree_sitter::Tree;

type ExtractFn = fn(&Tree, &str, &str, &Path) -> Result<ExtractionResults, anyhow::Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LanguageCapabilities {
    pub symbols: bool,
    pub relationships: bool,
    pub pending_relationships: bool,
    pub identifiers: bool,
    pub types: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct LanguageRegistryEntry {
    pub language: &'static str,
    pub capabilities: LanguageCapabilities,
    pub extract: ExtractFn,
}

const FULL_CAPABILITIES: LanguageCapabilities = LanguageCapabilities {
    symbols: true,
    relationships: true,
    pending_relationships: true,
    identifiers: true,
    types: true,
};

const NO_PENDING_CAPABILITIES: LanguageCapabilities = LanguageCapabilities {
    pending_relationships: false,
    ..FULL_CAPABILITIES
};

const PENDING_NO_TYPES_CAPABILITIES: LanguageCapabilities = LanguageCapabilities {
    types: false,
    ..FULL_CAPABILITIES
};

const DATA_ONLY_CAPABILITIES: LanguageCapabilities = LanguageCapabilities {
    symbols: true,
    relationships: false,
    pending_relationships: false,
    identifiers: true,
    types: false,
};

const fn entry(
    language: &'static str,
    capabilities: LanguageCapabilities,
    extract: ExtractFn,
) -> LanguageRegistryEntry {
    LanguageRegistryEntry {
        language,
        capabilities,
        extract,
    }
}

macro_rules! define_full_language_extractors {
    ($(($fn_name:ident, $language:literal, $extractor:path)),+ $(,)?) => {
        $(
            fn $fn_name(
                tree: &Tree,
                file_path: &str,
                content: &str,
                workspace_root: &Path,
            ) -> Result<ExtractionResults, anyhow::Error> {
                let mut ext = <$extractor>::new(
                    $language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    workspace_root,
                );
                let symbols = ext.extract_symbols(tree);
                let relationships = ext.extract_relationships(tree, &symbols);
                let identifiers = ext.extract_identifiers(tree, &symbols);
                let types = ext.infer_types(&symbols);
                let pending_relationships = ext.get_pending_relationships();
                Ok(ExtractionResults {
                    symbols,
                    relationships,
                    pending_relationships,
                    structured_pending_relationships: Vec::new(),
                    identifiers,
                    types: convert_types_map(types, $language),
                })
            }
        )+
    };
}

macro_rules! define_structured_full_language_extractors {
    ($(($fn_name:ident, $language:literal, $extractor:path)),+ $(,)?) => {
        $(
            fn $fn_name(
                tree: &Tree,
                file_path: &str,
                content: &str,
                workspace_root: &Path,
            ) -> Result<ExtractionResults, anyhow::Error> {
                let mut ext = <$extractor>::new(
                    $language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    workspace_root,
                );
                let symbols = ext.extract_symbols(tree);
                let relationships = ext.extract_relationships(tree, &symbols);
                let identifiers = ext.extract_identifiers(tree, &symbols);
                let types = ext.infer_types(&symbols);
                let pending_relationships = ext.get_pending_relationships();
                let structured_pending_relationships = ext.get_structured_pending_relationships();
                Ok(ExtractionResults {
                    symbols,
                    relationships,
                    pending_relationships,
                    structured_pending_relationships,
                    identifiers,
                    types: convert_types_map(types, $language),
                })
            }
        )+
    };
}

macro_rules! define_structured_full_file_extractors {
    ($(($fn_name:ident, $language:literal, $extractor:path)),+ $(,)?) => {
        $(
            fn $fn_name(
                tree: &Tree,
                file_path: &str,
                content: &str,
                workspace_root: &Path,
            ) -> Result<ExtractionResults, anyhow::Error> {
                let mut ext = <$extractor>::new(
                    file_path.to_string(),
                    content.to_string(),
                    workspace_root,
                );
                let symbols = ext.extract_symbols(tree);
                let relationships = ext.extract_relationships(tree, &symbols);
                let identifiers = ext.extract_identifiers(tree, &symbols);
                let types = ext.infer_types(&symbols);
                let pending_relationships = ext.get_pending_relationships();
                let structured_pending_relationships = ext.get_structured_pending_relationships();
                Ok(ExtractionResults {
                    symbols,
                    relationships,
                    pending_relationships,
                    structured_pending_relationships,
                    identifiers,
                    types: convert_types_map(types, $language),
                })
            }
        )+
    };
}

macro_rules! define_no_pending_extractors {
    ($(($fn_name:ident, $language:literal, $extractor:path)),+ $(,)?) => {
        $(
            fn $fn_name(
                tree: &Tree,
                file_path: &str,
                content: &str,
                workspace_root: &Path,
            ) -> Result<ExtractionResults, anyhow::Error> {
                let mut ext = <$extractor>::new(
                    $language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    workspace_root,
                );
                let symbols = ext.extract_symbols(tree);
                let relationships = ext.extract_relationships(tree, &symbols);
                let identifiers = ext.extract_identifiers(tree, &symbols);
                let types = ext.infer_types(&symbols);
                Ok(ExtractionResults {
                    symbols,
                    relationships,
                    pending_relationships: Vec::new(),
                    structured_pending_relationships: Vec::new(),
                    identifiers,
                    types: convert_types_map(types, $language),
                })
            }
        )+
    };
}

macro_rules! define_data_only_extractors {
    ($(($fn_name:ident, $language:literal, $extractor:path)),+ $(,)?) => {
        $(
            fn $fn_name(
                tree: &Tree,
                file_path: &str,
                content: &str,
                workspace_root: &Path,
            ) -> Result<ExtractionResults, anyhow::Error> {
                let mut ext = <$extractor>::new(
                    $language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    workspace_root,
                );
                let symbols = ext.extract_symbols(tree);
                let identifiers = ext.extract_identifiers(tree, &symbols);
                Ok(ExtractionResults {
                    symbols,
                    relationships: Vec::new(),
                    pending_relationships: Vec::new(),
                    structured_pending_relationships: Vec::new(),
                    identifiers,
                    types: HashMap::new(),
                })
            }
        )+
    };
}

define_full_language_extractors![(extract_elixir, "elixir", crate::elixir::ElixirExtractor)];

define_structured_full_language_extractors![
    (extract_rust, "rust", crate::rust::RustExtractor),
    (extract_dart, "dart", crate::dart::DartExtractor),
    (extract_go, "go", crate::go::GoExtractor),
    (extract_c, "c", crate::c::CExtractor),
    (extract_zig, "zig", crate::zig::ZigExtractor),
    (
        extract_gdscript,
        "gdscript",
        crate::gdscript::GDScriptExtractor
    )
];

fn extract_java(
    tree: &Tree,
    file_path: &str,
    content: &str,
    workspace_root: &Path,
) -> Result<ExtractionResults, anyhow::Error> {
    let mut ext = crate::java::JavaExtractor::new(
        "java".to_string(),
        file_path.to_string(),
        content.to_string(),
        workspace_root,
    );
    let symbols = ext.extract_symbols(tree);
    let relationships = ext.extract_relationships(tree, &symbols);
    let identifiers = ext.extract_identifiers(tree, &symbols);
    let types = ext.infer_types(&symbols);
    let pending_relationships = ext.get_pending_relationships();
    let structured_pending_relationships = ext.get_structured_pending_relationships();
    Ok(ExtractionResults {
        symbols,
        relationships,
        pending_relationships,
        structured_pending_relationships,
        identifiers,
        types: convert_types_map(types, "java"),
    })
}

fn extract_csharp(
    tree: &Tree,
    file_path: &str,
    content: &str,
    workspace_root: &Path,
) -> Result<ExtractionResults, anyhow::Error> {
    let mut ext = crate::csharp::CSharpExtractor::new(
        "csharp".to_string(),
        file_path.to_string(),
        content.to_string(),
        workspace_root,
    );
    let symbols = ext.extract_symbols(tree);
    let relationships = ext.extract_relationships(tree, &symbols);
    let identifiers = ext.extract_identifiers(tree, &symbols);
    let types = ext.infer_types(&symbols);
    let pending_relationships = ext.get_pending_relationships();
    let structured_pending_relationships = ext.get_structured_pending_relationships();
    Ok(ExtractionResults {
        symbols,
        relationships,
        pending_relationships,
        structured_pending_relationships,
        identifiers,
        types: convert_types_map(types, "csharp"),
    })
}

fn extract_kotlin(
    tree: &Tree,
    file_path: &str,
    content: &str,
    workspace_root: &Path,
) -> Result<ExtractionResults, anyhow::Error> {
    let mut ext = crate::kotlin::KotlinExtractor::new(
        "kotlin".to_string(),
        file_path.to_string(),
        content.to_string(),
        workspace_root,
    );
    let symbols = ext.extract_symbols(tree);
    let relationships = ext.extract_relationships(tree, &symbols);
    let identifiers = ext.extract_identifiers(tree, &symbols);
    let types = ext.infer_types(&symbols);
    let pending_relationships = ext.get_pending_relationships();
    let structured_pending_relationships = ext.get_structured_pending_relationships();
    Ok(ExtractionResults {
        symbols,
        relationships,
        pending_relationships,
        structured_pending_relationships,
        identifiers,
        types: convert_types_map(types, "kotlin"),
    })
}

fn extract_swift(
    tree: &Tree,
    file_path: &str,
    content: &str,
    workspace_root: &Path,
) -> Result<ExtractionResults, anyhow::Error> {
    let mut ext = crate::swift::SwiftExtractor::new(
        "swift".to_string(),
        file_path.to_string(),
        content.to_string(),
        workspace_root,
    );
    let symbols = ext.extract_symbols(tree);
    let relationships = ext.extract_relationships(tree, &symbols);
    let identifiers = ext.extract_identifiers(tree, &symbols);
    let types = ext.infer_types(&symbols);
    let pending_relationships = ext.get_pending_relationships();
    let structured_pending_relationships = ext.get_structured_pending_relationships();
    Ok(ExtractionResults {
        symbols,
        relationships,
        pending_relationships,
        structured_pending_relationships,
        identifiers,
        types: convert_types_map(types, "swift"),
    })
}

fn extract_php(
    tree: &Tree,
    file_path: &str,
    content: &str,
    workspace_root: &Path,
) -> Result<ExtractionResults, anyhow::Error> {
    let mut ext = crate::php::PhpExtractor::new(
        "php".to_string(),
        file_path.to_string(),
        content.to_string(),
        workspace_root,
    );
    let symbols = ext.extract_symbols(tree);
    let relationships = ext.extract_relationships(tree, &symbols);
    let identifiers = ext.extract_identifiers(tree, &symbols);
    let types = ext.infer_types(&symbols);
    let pending_relationships = ext.get_pending_relationships();
    let structured_pending_relationships = ext.get_structured_pending_relationships();
    Ok(ExtractionResults {
        symbols,
        relationships,
        pending_relationships,
        structured_pending_relationships,
        identifiers,
        types: convert_types_map(types, "php"),
    })
}

fn extract_scala(
    tree: &Tree,
    file_path: &str,
    content: &str,
    workspace_root: &Path,
) -> Result<ExtractionResults, anyhow::Error> {
    let mut ext = crate::scala::ScalaExtractor::new(
        "scala".to_string(),
        file_path.to_string(),
        content.to_string(),
        workspace_root,
    );
    let symbols = ext.extract_symbols(tree);
    let relationships = ext.extract_relationships(tree, &symbols);
    let identifiers = ext.extract_identifiers(tree, &symbols);
    let types = ext.infer_types(&symbols);
    let pending_relationships = ext.get_pending_relationships();
    let structured_pending_relationships = ext.get_structured_pending_relationships();
    Ok(ExtractionResults {
        symbols,
        relationships,
        pending_relationships,
        structured_pending_relationships,
        identifiers,
        types: convert_types_map(types, "scala"),
    })
}

fn extract_typescript(
    tree: &Tree,
    file_path: &str,
    content: &str,
    workspace_root: &Path,
) -> Result<ExtractionResults, anyhow::Error> {
    let mut ext = crate::typescript::TypeScriptExtractor::new(
        "typescript".to_string(),
        file_path.to_string(),
        content.to_string(),
        workspace_root,
    );
    let symbols = ext.extract_symbols(tree);
    let relationships = ext.extract_relationships(tree, &symbols);
    let identifiers = ext.extract_identifiers(tree, &symbols);
    let types = ext.infer_types(&symbols);
    let pending_relationships = ext.get_pending_relationships();
    let structured_pending_relationships = ext.get_structured_pending_relationships();
    Ok(ExtractionResults {
        symbols,
        relationships,
        pending_relationships,
        structured_pending_relationships,
        identifiers,
        types: convert_types_map(types, "typescript"),
    })
}

fn extract_tsx(
    tree: &Tree,
    file_path: &str,
    content: &str,
    workspace_root: &Path,
) -> Result<ExtractionResults, anyhow::Error> {
    let mut ext = crate::typescript::TypeScriptExtractor::new(
        "tsx".to_string(),
        file_path.to_string(),
        content.to_string(),
        workspace_root,
    );
    let symbols = ext.extract_symbols(tree);
    let relationships = ext.extract_relationships(tree, &symbols);
    let identifiers = ext.extract_identifiers(tree, &symbols);
    let types = ext.infer_types(&symbols);
    let pending_relationships = ext.get_pending_relationships();
    let structured_pending_relationships = ext.get_structured_pending_relationships();
    Ok(ExtractionResults {
        symbols,
        relationships,
        pending_relationships,
        structured_pending_relationships,
        identifiers,
        types: convert_types_map(types, "tsx"),
    })
}

fn extract_javascript(
    tree: &Tree,
    file_path: &str,
    content: &str,
    workspace_root: &Path,
) -> Result<ExtractionResults, anyhow::Error> {
    let mut ext = crate::javascript::JavaScriptExtractor::new(
        "javascript".to_string(),
        file_path.to_string(),
        content.to_string(),
        workspace_root,
    );
    let symbols = ext.extract_symbols(tree);
    let relationships = ext.extract_relationships(tree, &symbols);
    let identifiers = ext.extract_identifiers(tree, &symbols);
    let types = ext.infer_types(&symbols);
    let pending_relationships = ext.get_pending_relationships();
    let structured_pending_relationships = ext.get_structured_pending_relationships();
    Ok(ExtractionResults {
        symbols,
        relationships,
        pending_relationships,
        structured_pending_relationships,
        identifiers,
        types: convert_types_map(types, "javascript"),
    })
}

fn extract_jsx(
    tree: &Tree,
    file_path: &str,
    content: &str,
    workspace_root: &Path,
) -> Result<ExtractionResults, anyhow::Error> {
    let mut ext = crate::javascript::JavaScriptExtractor::new(
        "jsx".to_string(),
        file_path.to_string(),
        content.to_string(),
        workspace_root,
    );
    let symbols = ext.extract_symbols(tree);
    let relationships = ext.extract_relationships(tree, &symbols);
    let identifiers = ext.extract_identifiers(tree, &symbols);
    let types = ext.infer_types(&symbols);
    let pending_relationships = ext.get_pending_relationships();
    let structured_pending_relationships = ext.get_structured_pending_relationships();
    Ok(ExtractionResults {
        symbols,
        relationships,
        pending_relationships,
        structured_pending_relationships,
        identifiers,
        types: convert_types_map(types, "jsx"),
    })
}

fn extract_bash(
    tree: &Tree,
    file_path: &str,
    content: &str,
    workspace_root: &Path,
) -> Result<ExtractionResults, anyhow::Error> {
    let mut ext = crate::bash::BashExtractor::new(
        "bash".to_string(),
        file_path.to_string(),
        content.to_string(),
        workspace_root,
    );
    let symbols = ext.extract_symbols(tree);
    let relationships = ext.extract_relationships(tree, &symbols);
    let identifiers = ext.extract_identifiers(tree, &symbols);
    let types = ext.infer_types(&symbols);
    let pending_relationships = ext.get_pending_relationships();
    let structured_pending_relationships = ext.get_structured_pending_relationships();
    Ok(ExtractionResults {
        symbols,
        relationships,
        pending_relationships,
        structured_pending_relationships,
        identifiers,
        types: convert_types_map(types, "bash"),
    })
}

fn extract_powershell(
    tree: &Tree,
    file_path: &str,
    content: &str,
    workspace_root: &Path,
) -> Result<ExtractionResults, anyhow::Error> {
    let mut ext = crate::powershell::PowerShellExtractor::new(
        "powershell".to_string(),
        file_path.to_string(),
        content.to_string(),
        workspace_root,
    );
    let symbols = ext.extract_symbols(tree);
    let relationships = ext.extract_relationships(tree, &symbols);
    let identifiers = ext.extract_identifiers(tree, &symbols);
    let types = ext.infer_types(&symbols);
    let pending_relationships = ext.get_pending_relationships();
    let structured_pending_relationships = ext.get_structured_pending_relationships();
    Ok(ExtractionResults {
        symbols,
        relationships,
        pending_relationships,
        structured_pending_relationships,
        identifiers,
        types: convert_types_map(types, "powershell"),
    })
}

fn extract_lua(
    tree: &Tree,
    file_path: &str,
    content: &str,
    workspace_root: &Path,
) -> Result<ExtractionResults, anyhow::Error> {
    let mut ext = crate::lua::LuaExtractor::new(
        "lua".to_string(),
        file_path.to_string(),
        content.to_string(),
        workspace_root,
    );
    let symbols = ext.extract_symbols(tree);
    let relationships = ext.extract_relationships(tree, &symbols);
    let identifiers = ext.extract_identifiers(tree, &symbols);
    let pending_relationships = ext.get_pending_relationships();
    let structured_pending_relationships = ext.get_structured_pending_relationships();
    Ok(ExtractionResults {
        symbols,
        relationships,
        pending_relationships,
        structured_pending_relationships,
        identifiers,
        types: HashMap::new(),
    })
}

fn extract_qml(
    tree: &Tree,
    file_path: &str,
    content: &str,
    workspace_root: &Path,
) -> Result<ExtractionResults, anyhow::Error> {
    let mut ext = crate::qml::QmlExtractor::new(
        "qml".to_string(),
        file_path.to_string(),
        content.to_string(),
        workspace_root,
    );
    let symbols = ext.extract_symbols(tree);
    let relationships = ext.extract_relationships(tree, &symbols);
    let identifiers = ext.extract_identifiers(tree, &symbols);
    let pending_relationships = ext.get_pending_relationships();
    let structured_pending_relationships = ext.get_structured_pending_relationships();
    Ok(ExtractionResults {
        symbols,
        relationships,
        pending_relationships,
        structured_pending_relationships,
        identifiers,
        types: HashMap::new(),
    })
}

fn extract_r(
    tree: &Tree,
    file_path: &str,
    content: &str,
    workspace_root: &Path,
) -> Result<ExtractionResults, anyhow::Error> {
    let mut ext = crate::r::RExtractor::new(
        "r".to_string(),
        file_path.to_string(),
        content.to_string(),
        workspace_root,
    );
    let symbols = ext.extract_symbols(tree);
    let relationships = ext.extract_relationships(tree, &symbols);
    let identifiers = ext.extract_identifiers(tree, &symbols);
    let pending_relationships = ext.get_pending_relationships();
    let structured_pending_relationships = ext.get_structured_pending_relationships();
    Ok(ExtractionResults {
        symbols,
        relationships,
        pending_relationships,
        structured_pending_relationships,
        identifiers,
        types: HashMap::new(),
    })
}

define_structured_full_file_extractors![
    (extract_python, "python", crate::python::PythonExtractor),
    (extract_cpp, "cpp", crate::cpp::CppExtractor),
    (extract_ruby, "ruby", crate::ruby::RubyExtractor)
];

define_no_pending_extractors![
    (extract_sql, "sql", crate::sql::SqlExtractor),
    (extract_html, "html", crate::html::HTMLExtractor),
    (extract_razor, "razor", crate::razor::RazorExtractor),
    (extract_regex, "regex", crate::regex::RegexExtractor)
];

define_data_only_extractors![
    (extract_css, "css", crate::css::CSSExtractor),
    (
        extract_markdown,
        "markdown",
        crate::markdown::MarkdownExtractor
    ),
    (extract_json, "json", crate::json::JsonExtractor),
    (extract_toml, "toml", crate::toml::TomlExtractor),
    (extract_yaml, "yaml", crate::yaml::YamlExtractor)
];

fn extract_vue(
    tree: &Tree,
    file_path: &str,
    content: &str,
    workspace_root: &Path,
) -> Result<ExtractionResults, anyhow::Error> {
    let mut ext = crate::vue::VueExtractor::new(
        "vue".to_string(),
        file_path.to_string(),
        content.to_string(),
        workspace_root,
    );
    let symbols = ext.extract_symbols(Some(tree));
    let relationships = ext.extract_relationships(Some(tree), &symbols);
    let identifiers = ext.extract_identifiers(&symbols);
    let types = ext.infer_types(&symbols);
    Ok(ExtractionResults {
        symbols,
        relationships,
        pending_relationships: Vec::new(),
        structured_pending_relationships: Vec::new(),
        identifiers,
        types: convert_types_map(types, "vue"),
    })
}

const REGISTRY: &[LanguageRegistryEntry] = &[
    entry("rust", FULL_CAPABILITIES, extract_rust),
    entry("c", FULL_CAPABILITIES, extract_c),
    entry("cpp", FULL_CAPABILITIES, extract_cpp),
    entry("go", FULL_CAPABILITIES, extract_go),
    entry("zig", FULL_CAPABILITIES, extract_zig),
    entry("typescript", FULL_CAPABILITIES, extract_typescript),
    entry("tsx", FULL_CAPABILITIES, extract_tsx),
    entry("javascript", FULL_CAPABILITIES, extract_javascript),
    entry("jsx", FULL_CAPABILITIES, extract_jsx),
    entry("html", NO_PENDING_CAPABILITIES, extract_html),
    entry("css", DATA_ONLY_CAPABILITIES, extract_css),
    entry("vue", NO_PENDING_CAPABILITIES, extract_vue),
    entry("python", FULL_CAPABILITIES, extract_python),
    entry("java", FULL_CAPABILITIES, extract_java),
    entry("csharp", FULL_CAPABILITIES, extract_csharp),
    entry("php", FULL_CAPABILITIES, extract_php),
    entry("ruby", FULL_CAPABILITIES, extract_ruby),
    entry("swift", FULL_CAPABILITIES, extract_swift),
    entry("kotlin", FULL_CAPABILITIES, extract_kotlin),
    entry("scala", FULL_CAPABILITIES, extract_scala),
    entry("dart", FULL_CAPABILITIES, extract_dart),
    entry("elixir", FULL_CAPABILITIES, extract_elixir),
    entry("lua", PENDING_NO_TYPES_CAPABILITIES, extract_lua),
    entry("qml", PENDING_NO_TYPES_CAPABILITIES, extract_qml),
    entry("r", PENDING_NO_TYPES_CAPABILITIES, extract_r),
    entry("bash", FULL_CAPABILITIES, extract_bash),
    entry("powershell", FULL_CAPABILITIES, extract_powershell),
    entry("gdscript", FULL_CAPABILITIES, extract_gdscript),
    entry("razor", NO_PENDING_CAPABILITIES, extract_razor),
    entry("sql", NO_PENDING_CAPABILITIES, extract_sql),
    entry("regex", NO_PENDING_CAPABILITIES, extract_regex),
    entry("markdown", DATA_ONLY_CAPABILITIES, extract_markdown),
    entry("json", DATA_ONLY_CAPABILITIES, extract_json),
    entry("toml", DATA_ONLY_CAPABILITIES, extract_toml),
    entry("yaml", DATA_ONLY_CAPABILITIES, extract_yaml),
];

pub fn registry_entry(language: &str) -> Result<&'static LanguageRegistryEntry, anyhow::Error> {
    REGISTRY
        .iter()
        .find(|entry| entry.language == language)
        .ok_or_else(|| anyhow!("No extractor available for language '{}'", language))
}

pub fn supported_languages() -> Vec<&'static str> {
    REGISTRY.iter().map(|entry| entry.language).collect()
}

pub fn extract_for_language(
    language: &str,
    tree: &Tree,
    file_path: &str,
    content: &str,
    workspace_root: &Path,
) -> Result<ExtractionResults, anyhow::Error> {
    let entry = registry_entry(language).map_err(|_| {
        anyhow!(
            "No extractor available for language '{}' (file: {})",
            language,
            file_path
        )
    })?;
    (entry.extract)(tree, file_path, content, workspace_root)
}

pub fn capabilities_for_language(language: &str) -> Result<LanguageCapabilities, anyhow::Error> {
    Ok(registry_entry(language)?.capabilities)
}

#[cfg(test)]
mod registry_tests {
    use super::*;

    #[test]
    fn registry_matches_supported_language_count() {
        assert_eq!(supported_languages().len(), 35);
        assert!(
            capabilities_for_language("rust")
                .unwrap()
                .pending_relationships
        );
        assert!(!capabilities_for_language("css").unwrap().relationships);
    }
}
