use anyhow::Result;
use std::sync::OnceLock;

type ParserFn = fn() -> tree_sitter::Language;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LanguageCapabilities {
    pub symbols: bool,
    pub relationships: bool,
    pub pending_relationships: bool,
    pub identifiers: bool,
    pub types: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocCommentStyle {
    SlashStarDocBlock,
    PlainSlashStarBlock,
    HtmlBlock,
    TripleSlash,
    RustInnerLine,
    RustInnerBlock,
    GoLine,
    LuaTripleDash,
    LuaDoubleDash,
    LuaBlock,
    SqlLine,
    RHashPrime,
    HashLine,
    RazorBlock,
    GdscriptDoubleHash,
    VbTripleApostrophe,
}

#[derive(Debug, Clone, Copy)]
pub struct LanguageSpec {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub extensions: &'static [&'static str],
    pub parser_crate: &'static str,
    pub capabilities: LanguageCapabilities,
    parser: ParserFn,
    doc_comment_styles: &'static [DocCommentStyle],
}

impl LanguageSpec {
    pub fn parser_language(&self) -> tree_sitter::Language {
        (self.parser)()
    }

    pub fn is_doc_comment(&self, text: &str) -> bool {
        let trimmed = text.trim_start();
        self.doc_comment_styles
            .iter()
            .any(|style| style.starts_doc_comment(trimmed))
    }

    pub fn continues_doc_comment(&self, text: &str) -> bool {
        let trimmed = text.trim_start();
        self.doc_comment_styles
            .iter()
            .any(|style| style.continues_doc_comment(trimmed))
    }
}

impl DocCommentStyle {
    fn starts_doc_comment(self, trimmed: &str) -> bool {
        match self {
            Self::SlashStarDocBlock => trimmed.starts_with("/**"),
            Self::PlainSlashStarBlock => trimmed.starts_with("/*"),
            Self::HtmlBlock => trimmed.starts_with("<!--"),
            Self::TripleSlash => trimmed.starts_with("///"),
            Self::RustInnerLine => trimmed.starts_with("//!"),
            Self::RustInnerBlock => trimmed.starts_with("/*!"),
            Self::GoLine => trimmed.starts_with("//"),
            Self::LuaTripleDash => trimmed.starts_with("---"),
            Self::LuaDoubleDash => false,
            Self::LuaBlock => trimmed.starts_with("--[["),
            Self::SqlLine => trimmed.starts_with("--"),
            Self::RHashPrime => trimmed.starts_with("#'"),
            Self::HashLine => trimmed.starts_with("#"),
            Self::RazorBlock => trimmed.starts_with("@*"),
            Self::GdscriptDoubleHash => trimmed.starts_with("##"),
            Self::VbTripleApostrophe => trimmed.starts_with("'''"),
        }
    }

    fn continues_doc_comment(self, trimmed: &str) -> bool {
        match self {
            Self::LuaDoubleDash => trimmed.starts_with("--"),
            _ => self.starts_doc_comment(trimmed),
        }
    }
}

pub const FULL_CAPABILITIES: LanguageCapabilities = LanguageCapabilities {
    symbols: true,
    relationships: true,
    pending_relationships: true,
    identifiers: true,
    types: true,
};

pub const NO_PENDING_CAPABILITIES: LanguageCapabilities = LanguageCapabilities {
    pending_relationships: false,
    ..FULL_CAPABILITIES
};

pub const NO_RELATIONSHIP_CAPABILITIES: LanguageCapabilities = LanguageCapabilities {
    relationships: false,
    pending_relationships: false,
    ..FULL_CAPABILITIES
};

pub const PENDING_NO_TYPES_CAPABILITIES: LanguageCapabilities = LanguageCapabilities {
    types: false,
    ..FULL_CAPABILITIES
};

pub const DATA_ONLY_CAPABILITIES: LanguageCapabilities = LanguageCapabilities {
    symbols: true,
    relationships: false,
    pending_relationships: false,
    identifiers: true,
    types: false,
};

pub const RELATIONSHIP_DATA_CAPABILITIES: LanguageCapabilities = LanguageCapabilities {
    symbols: true,
    relationships: true,
    pending_relationships: false,
    identifiers: true,
    types: false,
};

const EMPTY: &[DocCommentStyle] = &[];
const RUST_DOCS: &[DocCommentStyle] = &[
    DocCommentStyle::SlashStarDocBlock,
    DocCommentStyle::TripleSlash,
    DocCommentStyle::RustInnerLine,
    DocCommentStyle::RustInnerBlock,
];
const C_DOCS: &[DocCommentStyle] = &[
    DocCommentStyle::SlashStarDocBlock,
    DocCommentStyle::TripleSlash,
];
const GO_DOCS: &[DocCommentStyle] = &[
    DocCommentStyle::PlainSlashStarBlock,
    DocCommentStyle::GoLine,
];
const JAVA_DOCS: &[DocCommentStyle] = &[
    DocCommentStyle::SlashStarDocBlock,
    DocCommentStyle::TripleSlash,
];
const CSHARP_DOCS: &[DocCommentStyle] = &[
    DocCommentStyle::SlashStarDocBlock,
    DocCommentStyle::TripleSlash,
];
const VBNET_DOCS: &[DocCommentStyle] = &[DocCommentStyle::VbTripleApostrophe];
const SWIFT_DOCS: &[DocCommentStyle] = &[
    DocCommentStyle::SlashStarDocBlock,
    DocCommentStyle::TripleSlash,
];
const KOTLIN_DOCS: &[DocCommentStyle] = &[
    DocCommentStyle::SlashStarDocBlock,
    DocCommentStyle::TripleSlash,
];
const DART_DOCS: &[DocCommentStyle] = &[
    DocCommentStyle::SlashStarDocBlock,
    DocCommentStyle::TripleSlash,
];
const JS_DOCS: &[DocCommentStyle] = &[DocCommentStyle::SlashStarDocBlock];
const PHP_DOCS: &[DocCommentStyle] = &[DocCommentStyle::SlashStarDocBlock];
const HTML_DOCS: &[DocCommentStyle] = &[DocCommentStyle::HtmlBlock];
const CSS_DOCS: &[DocCommentStyle] = &[DocCommentStyle::PlainSlashStarBlock];
const SQL_DOCS: &[DocCommentStyle] = &[
    DocCommentStyle::PlainSlashStarBlock,
    DocCommentStyle::SqlLine,
];
const LUA_DOCS: &[DocCommentStyle] = &[
    DocCommentStyle::LuaTripleDash,
    DocCommentStyle::LuaBlock,
    DocCommentStyle::LuaDoubleDash,
];
const R_DOCS: &[DocCommentStyle] = &[DocCommentStyle::RHashPrime];
const HASH_DOCS: &[DocCommentStyle] = &[DocCommentStyle::HashLine];
const RAZOR_DOCS: &[DocCommentStyle] = &[DocCommentStyle::TripleSlash, DocCommentStyle::RazorBlock];
const GDSCRIPT_DOCS: &[DocCommentStyle] = &[DocCommentStyle::GdscriptDoubleHash];
const ZIG_DOCS: &[DocCommentStyle] = &[DocCommentStyle::TripleSlash];

macro_rules! parser {
    ($name:ident, $language:path) => {
        fn $name() -> tree_sitter::Language {
            $language.into()
        }
    };
}

parser!(parser_rust, tree_sitter_rust::LANGUAGE);
parser!(parser_c, tree_sitter_c::LANGUAGE);
parser!(parser_cpp, tree_sitter_cpp::LANGUAGE);
parser!(parser_go, tree_sitter_go::LANGUAGE);
parser!(parser_zig, tree_sitter_zig::LANGUAGE);
parser!(
    parser_typescript,
    tree_sitter_typescript::LANGUAGE_TYPESCRIPT
);
parser!(parser_tsx, tree_sitter_typescript::LANGUAGE_TSX);
parser!(parser_javascript, tree_sitter_javascript::LANGUAGE);
parser!(parser_html, tree_sitter_html::LANGUAGE);
parser!(parser_css, tree_sitter_css::LANGUAGE);
parser!(parser_python, tree_sitter_python::LANGUAGE);
parser!(parser_java, tree_sitter_java::LANGUAGE);
parser!(parser_csharp, tree_sitter_c_sharp::LANGUAGE);
parser!(parser_vbnet, tree_sitter_vb_dotnet::LANGUAGE);
parser!(parser_php, tree_sitter_php::LANGUAGE_PHP);
parser!(parser_ruby, tree_sitter_ruby::LANGUAGE);
parser!(parser_swift, tree_sitter_swift::LANGUAGE);
parser!(parser_kotlin, tree_sitter_kotlin_ng::LANGUAGE);
parser!(parser_scala, tree_sitter_scala::LANGUAGE);
parser!(parser_dart, tree_sitter_dart::LANGUAGE);
parser!(parser_elixir, tree_sitter_elixir::LANGUAGE);
parser!(parser_lua, tree_sitter_lua::LANGUAGE);
parser!(parser_qml, tree_sitter_qmljs::LANGUAGE);
parser!(parser_r, tree_sitter_r::LANGUAGE);
parser!(parser_bash, tree_sitter_bash::LANGUAGE);
parser!(parser_powershell, tree_sitter_powershell::LANGUAGE);
parser!(parser_gdscript, tree_sitter_gdscript::LANGUAGE);
parser!(parser_razor, tree_sitter_razor::LANGUAGE);
parser!(parser_sql, tree_sitter_sequel::LANGUAGE);
parser!(parser_regex, tree_sitter_regex::LANGUAGE);
parser!(parser_markdown, tree_sitter_md::LANGUAGE);
parser!(parser_json, tree_sitter_json::LANGUAGE);
parser!(parser_toml, tree_sitter_toml_ng::LANGUAGE);
parser!(parser_yaml, tree_sitter_yaml::LANGUAGE);

mod specs;

pub fn language_specs() -> &'static [LanguageSpec] {
    specs::LANGUAGE_SPECS
}

pub fn language_spec(language: &str) -> Option<&'static LanguageSpec> {
    language_specs()
        .iter()
        .find(|spec| spec.name == language || spec.aliases.contains(&language))
}

pub fn get_tree_sitter_language(language: &str) -> Result<tree_sitter::Language> {
    language_spec(language)
        .map(LanguageSpec::parser_language)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Unsupported language: '{}'. Supported languages: {}",
                language,
                supported_languages().join(", ")
            )
        })
}

pub fn detect_language_from_extension(extension: &str) -> Option<&'static str> {
    language_specs()
        .iter()
        .find(|spec| spec.extensions.contains(&extension))
        .map(|spec| spec.name)
}

pub fn supported_extensions() -> &'static [&'static str] {
    static SUPPORTED_EXTENSIONS: OnceLock<Vec<&'static str>> = OnceLock::new();
    SUPPORTED_EXTENSIONS
        .get_or_init(|| {
            language_specs()
                .iter()
                .flat_map(|spec| spec.extensions.iter().copied())
                .collect()
        })
        .as_slice()
}

pub fn supported_languages() -> &'static [&'static str] {
    static SUPPORTED_LANGUAGES: OnceLock<Vec<&'static str>> = OnceLock::new();
    SUPPORTED_LANGUAGES
        .get_or_init(|| language_specs().iter().map(|spec| spec.name).collect())
        .as_slice()
}
