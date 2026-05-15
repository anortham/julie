use crate::base::ExtractionResults;
use crate::factory::convert_types_map;
use crate::language;
pub use crate::language::LanguageCapabilities;
use anyhow::anyhow;
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;
use tree_sitter::Tree;

type ExtractFn = fn(&Tree, &str, &str, &Path) -> Result<ExtractionResults, anyhow::Error>;

#[derive(Debug, Clone, Copy)]
pub struct LanguageRegistryEntry {
    pub language: &'static str,
    pub capabilities: LanguageCapabilities,
    pub extract: ExtractFn,
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
                    parse_diagnostics: Vec::new(),
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
                    parse_diagnostics: Vec::new(),
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
                    parse_diagnostics: Vec::new(),
                })
            }
        )+
    };
}

macro_rules! define_relationship_data_extractors {
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
                Ok(ExtractionResults {
                    symbols,
                    relationships,
                    pending_relationships: Vec::new(),
                    structured_pending_relationships: Vec::new(),
                    identifiers,
                    types: HashMap::new(),
                    parse_diagnostics: Vec::new(),
                })
            }
        )+
    };
}

define_structured_full_language_extractors![
    (extract_elixir, "elixir", crate::elixir::ElixirExtractor),
    (extract_rust, "rust", crate::rust::RustExtractor),
    (extract_dart, "dart", crate::dart::DartExtractor),
    (extract_go, "go", crate::go::GoExtractor),
    (extract_c, "c", crate::c::CExtractor),
    (extract_zig, "zig", crate::zig::ZigExtractor),
    (extract_vbnet, "vbnet", crate::vbnet::VbNetExtractor),
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
        parse_diagnostics: Vec::new(),
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
        parse_diagnostics: Vec::new(),
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
        parse_diagnostics: Vec::new(),
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
        parse_diagnostics: Vec::new(),
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
        parse_diagnostics: Vec::new(),
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
        parse_diagnostics: Vec::new(),
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
        parse_diagnostics: Vec::new(),
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
        parse_diagnostics: Vec::new(),
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
        parse_diagnostics: Vec::new(),
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
        parse_diagnostics: Vec::new(),
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
        parse_diagnostics: Vec::new(),
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
        parse_diagnostics: Vec::new(),
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
        parse_diagnostics: Vec::new(),
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
        parse_diagnostics: Vec::new(),
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
        parse_diagnostics: Vec::new(),
    })
}

define_structured_full_file_extractors![
    (extract_python, "python", crate::python::PythonExtractor),
    (extract_cpp, "cpp", crate::cpp::CppExtractor),
    (extract_ruby, "ruby", crate::ruby::RubyExtractor)
];

define_no_pending_extractors![
    (extract_razor, "razor", crate::razor::RazorExtractor),
    (extract_regex, "regex", crate::regex::RegexExtractor)
];

/// Hand-written HTML extractor entry point. Phase 4b.html graduated HTML out
/// of `define_no_pending_extractors!` so its
/// `extract_structured_pending_relationships` emissions for external
/// `<script src=...>` and `<link href=...>` references reach the canonical
/// extraction results. See `crates/julie-extractors/src/html/relationships.rs`
/// for the shape contract.
fn extract_html(
    tree: &Tree,
    file_path: &str,
    content: &str,
    workspace_root: &Path,
) -> Result<ExtractionResults, anyhow::Error> {
    let mut ext = crate::html::HTMLExtractor::new(
        "html".to_string(),
        file_path.to_string(),
        content.to_string(),
        workspace_root,
    );
    let symbols = ext.extract_symbols(tree);
    let relationships = ext.extract_relationships(tree, &symbols);
    let identifiers = ext.extract_identifiers(tree, &symbols);
    let types = ext.infer_types(&symbols);
    let structured_pending_relationships =
        ext.extract_structured_pending_relationships(tree, &symbols);
    let pending_relationships = structured_pending_relationships
        .clone()
        .into_iter()
        .map(|pending| pending.into_pending_relationship())
        .collect();
    Ok(ExtractionResults {
        symbols,
        relationships,
        pending_relationships,
        structured_pending_relationships,
        identifiers,
        types: convert_types_map(types, "html"),
        parse_diagnostics: Vec::new(),
    })
}

/// Hand-written SQL extractor entry point. Phase 3.1 graduated SQL out of
/// `define_no_pending_extractors!` so its `add_structured_pending_relationship`
/// emissions for cross-schema FK targets reach the canonical extraction
/// results. See `crates/julie-extractors/src/sql/relationships.rs` for the
/// FK shape contract.
fn extract_sql(
    tree: &Tree,
    file_path: &str,
    content: &str,
    workspace_root: &Path,
) -> Result<ExtractionResults, anyhow::Error> {
    let mut ext = crate::sql::SqlExtractor::new(
        "sql".to_string(),
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
        types: convert_types_map(types, "sql"),
        parse_diagnostics: Vec::new(),
    })
}

define_relationship_data_extractors![
    (extract_css, "css", crate::css::CSSExtractor),
    (
        extract_markdown,
        "markdown",
        crate::markdown::MarkdownExtractor
    ),
    (extract_yaml, "yaml", crate::yaml::YamlExtractor)
];

/// TOML extractor (Phase 3.3): hand-written so it can emit domain-aware
/// relationships for Cargo `[dependencies]` and pyproject `[tool.*]`
/// tables. `pending_relationships` stays empty — TOML's references are
/// always file-local; `types` stays empty — TOML has no static type
/// system.
fn extract_toml(
    tree: &Tree,
    file_path: &str,
    content: &str,
    workspace_root: &Path,
) -> Result<ExtractionResults, anyhow::Error> {
    let mut ext = crate::toml::TomlExtractor::new(
        "toml".to_string(),
        file_path.to_string(),
        content.to_string(),
        workspace_root,
    );
    let symbols = ext.extract_symbols(tree);
    let relationships = ext.extract_relationships(tree, &symbols);
    let identifiers = ext.extract_identifiers(tree, &symbols);
    Ok(ExtractionResults {
        symbols,
        relationships,
        pending_relationships: Vec::new(),
        structured_pending_relationships: Vec::new(),
        identifiers,
        types: HashMap::new(),
        parse_diagnostics: Vec::new(),
    })
}

/// JSON extractor (Phase 3.2): hand-written so it can return relationships
/// (concrete + structured pending) for JSON Schema `$ref` shapes.
/// `types` stays empty — JSON has no static type system.
fn extract_json(
    tree: &Tree,
    file_path: &str,
    content: &str,
    workspace_root: &Path,
) -> Result<ExtractionResults, anyhow::Error> {
    let mut ext = crate::json::JsonExtractor::new(
        "json".to_string(),
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
        parse_diagnostics: Vec::new(),
    })
}

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
    let structured_pending_relationships = ext.extract_structured_pending_relationships(&symbols);
    let pending_relationships = structured_pending_relationships
        .clone()
        .into_iter()
        .map(|pending| pending.into_pending_relationship())
        .collect();
    Ok(ExtractionResults {
        symbols,
        relationships,
        pending_relationships,
        structured_pending_relationships,
        identifiers,
        types: convert_types_map(types, "vue"),
        parse_diagnostics: Vec::new(),
    })
}

const EXTRACTORS: &[(&str, ExtractFn)] = &[
    ("rust", extract_rust),
    ("c", extract_c),
    ("cpp", extract_cpp),
    ("go", extract_go),
    ("zig", extract_zig),
    ("typescript", extract_typescript),
    ("tsx", extract_tsx),
    ("javascript", extract_javascript),
    ("jsx", extract_jsx),
    ("html", extract_html),
    ("css", extract_css),
    ("vue", extract_vue),
    ("python", extract_python),
    ("java", extract_java),
    ("csharp", extract_csharp),
    ("vbnet", extract_vbnet),
    ("php", extract_php),
    ("ruby", extract_ruby),
    ("swift", extract_swift),
    ("kotlin", extract_kotlin),
    ("scala", extract_scala),
    ("dart", extract_dart),
    ("elixir", extract_elixir),
    ("lua", extract_lua),
    ("qml", extract_qml),
    ("r", extract_r),
    ("bash", extract_bash),
    ("powershell", extract_powershell),
    ("gdscript", extract_gdscript),
    ("razor", extract_razor),
    ("sql", extract_sql),
    ("regex", extract_regex),
    ("markdown", extract_markdown),
    ("json", extract_json),
    ("toml", extract_toml),
    ("yaml", extract_yaml),
];

fn registry() -> &'static [LanguageRegistryEntry] {
    static REGISTRY: OnceLock<Vec<LanguageRegistryEntry>> = OnceLock::new();
    REGISTRY
        .get_or_init(|| {
            language::language_specs()
                .iter()
                .map(|spec| {
                    let extract = EXTRACTORS
                        .iter()
                        .find(|(language, _)| *language == spec.name)
                        .map(|(_, extract)| *extract)
                        .unwrap_or_else(|| panic!("missing extractor for {}", spec.name));
                    LanguageRegistryEntry {
                        language: spec.name,
                        capabilities: spec.capabilities,
                        extract,
                    }
                })
                .collect()
        })
        .as_slice()
}

pub fn registry_entry(language: &str) -> Result<&'static LanguageRegistryEntry, anyhow::Error> {
    registry()
        .iter()
        .find(|entry| entry.language == language)
        .ok_or_else(|| anyhow!("No extractor available for language '{}'", language))
}

pub fn supported_languages() -> Vec<&'static str> {
    language::supported_languages().to_vec()
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
        assert_eq!(supported_languages().len(), 36);
        assert!(
            capabilities_for_language("rust")
                .unwrap()
                .pending_relationships
        );
        assert!(!capabilities_for_language("css").unwrap().relationships);
    }
}
