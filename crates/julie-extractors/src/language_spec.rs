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
    PlainSlashStarBlock,
    HtmlBlock,
    TripleSlash,
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
        trimmed.starts_with("/**")
            || self
                .doc_comment_styles
                .iter()
                .any(|style| style.matches(trimmed))
    }
}

impl DocCommentStyle {
    fn matches(self, trimmed: &str) -> bool {
        match self {
            Self::PlainSlashStarBlock => trimmed.starts_with("/*"),
            Self::HtmlBlock => trimmed.starts_with("<!--"),
            Self::TripleSlash => trimmed.starts_with("///"),
            Self::GoLine => trimmed.starts_with("//"),
            Self::LuaTripleDash => trimmed.starts_with("---"),
            Self::LuaDoubleDash => trimmed.starts_with("--"),
            Self::LuaBlock => trimmed.starts_with("--[["),
            Self::SqlLine => trimmed.starts_with("--"),
            Self::RHashPrime => trimmed.starts_with("#'"),
            Self::HashLine => trimmed.starts_with("#"),
            Self::RazorBlock => trimmed.starts_with("@*"),
            Self::GdscriptDoubleHash => trimmed.starts_with("##"),
            Self::VbTripleApostrophe => trimmed.starts_with("'''"),
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
const C_DOCS: &[DocCommentStyle] = &[DocCommentStyle::TripleSlash];
const GO_DOCS: &[DocCommentStyle] = &[
    DocCommentStyle::PlainSlashStarBlock,
    DocCommentStyle::GoLine,
];
const JAVA_DOCS: &[DocCommentStyle] = &[DocCommentStyle::TripleSlash];
const CSHARP_DOCS: &[DocCommentStyle] = &[DocCommentStyle::TripleSlash];
const VBNET_DOCS: &[DocCommentStyle] = &[DocCommentStyle::VbTripleApostrophe];
const SWIFT_DOCS: &[DocCommentStyle] = &[DocCommentStyle::TripleSlash];
const KOTLIN_DOCS: &[DocCommentStyle] = &[DocCommentStyle::TripleSlash];
const DART_DOCS: &[DocCommentStyle] = &[DocCommentStyle::TripleSlash];
const HTML_DOCS: &[DocCommentStyle] = &[DocCommentStyle::HtmlBlock];
const CSS_DOCS: &[DocCommentStyle] = &[DocCommentStyle::PlainSlashStarBlock];
const SQL_DOCS: &[DocCommentStyle] = &[
    DocCommentStyle::PlainSlashStarBlock,
    DocCommentStyle::SqlLine,
];
const LUA_DOCS: &[DocCommentStyle] = &[
    DocCommentStyle::PlainSlashStarBlock,
    DocCommentStyle::LuaTripleDash,
    DocCommentStyle::LuaDoubleDash,
    DocCommentStyle::LuaBlock,
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

const LANGUAGE_SPECS: &[LanguageSpec] = &[
    spec(
        "rust",
        &["rs"],
        "tree-sitter-rust",
        FULL_CAPABILITIES,
        parser_rust,
        C_DOCS,
    ),
    spec(
        "c",
        &["c", "h"],
        "tree-sitter-c",
        FULL_CAPABILITIES,
        parser_c,
        C_DOCS,
    ),
    spec(
        "cpp",
        &["cpp", "cc", "cxx", "c++", "hpp", "hh", "hxx", "h++"],
        "tree-sitter-cpp",
        FULL_CAPABILITIES,
        parser_cpp,
        C_DOCS,
    ),
    spec(
        "go",
        &["go"],
        "tree-sitter-go",
        FULL_CAPABILITIES,
        parser_go,
        GO_DOCS,
    ),
    spec(
        "zig",
        &["zig"],
        "tree-sitter-zig",
        FULL_CAPABILITIES,
        parser_zig,
        ZIG_DOCS,
    ),
    spec(
        "typescript",
        &["ts", "mts", "cts"],
        "tree-sitter-typescript",
        FULL_CAPABILITIES,
        parser_typescript,
        EMPTY,
    ),
    spec(
        "tsx",
        &["tsx"],
        "tree-sitter-typescript",
        FULL_CAPABILITIES,
        parser_tsx,
        EMPTY,
    ),
    spec(
        "javascript",
        &["js", "mjs", "cjs"],
        "tree-sitter-javascript",
        FULL_CAPABILITIES,
        parser_javascript,
        EMPTY,
    ),
    spec(
        "jsx",
        &["jsx"],
        "tree-sitter-javascript",
        FULL_CAPABILITIES,
        parser_javascript,
        EMPTY,
    ),
    spec(
        "html",
        &["html", "htm"],
        "tree-sitter-html",
        NO_PENDING_CAPABILITIES,
        parser_html,
        HTML_DOCS,
    ),
    spec(
        "css",
        &["css"],
        "tree-sitter-css",
        RELATIONSHIP_DATA_CAPABILITIES,
        parser_css,
        CSS_DOCS,
    ),
    spec(
        "vue",
        &["vue"],
        "tree-sitter-html",
        FULL_CAPABILITIES,
        parser_html,
        HTML_DOCS,
    ),
    spec(
        "python",
        &["py", "pyi", "pyw"],
        "tree-sitter-python",
        FULL_CAPABILITIES,
        parser_python,
        EMPTY,
    ),
    spec(
        "java",
        &["java"],
        "tree-sitter-java",
        FULL_CAPABILITIES,
        parser_java,
        JAVA_DOCS,
    ),
    spec(
        "csharp",
        &["cs"],
        "tree-sitter-c-sharp",
        FULL_CAPABILITIES,
        parser_csharp,
        CSHARP_DOCS,
    ),
    spec(
        "vbnet",
        &["vb"],
        "tree-sitter-vb-dotnet",
        FULL_CAPABILITIES,
        parser_vbnet,
        VBNET_DOCS,
    ),
    spec(
        "php",
        &["php", "phtml"],
        "tree-sitter-php",
        FULL_CAPABILITIES,
        parser_php,
        EMPTY,
    ),
    spec(
        "ruby",
        &["rb", "rbw"],
        "tree-sitter-ruby",
        FULL_CAPABILITIES,
        parser_ruby,
        HASH_DOCS,
    ),
    spec(
        "swift",
        &["swift"],
        "tree-sitter-swift",
        FULL_CAPABILITIES,
        parser_swift,
        SWIFT_DOCS,
    ),
    spec(
        "kotlin",
        &["kt", "kts"],
        "tree-sitter-kotlin-ng",
        FULL_CAPABILITIES,
        parser_kotlin,
        KOTLIN_DOCS,
    ),
    spec(
        "scala",
        &["scala", "sc"],
        "tree-sitter-scala",
        FULL_CAPABILITIES,
        parser_scala,
        EMPTY,
    ),
    spec(
        "dart",
        &["dart"],
        "tree-sitter-dart",
        FULL_CAPABILITIES,
        parser_dart,
        DART_DOCS,
    ),
    spec(
        "elixir",
        &["ex", "exs"],
        "tree-sitter-elixir",
        FULL_CAPABILITIES,
        parser_elixir,
        EMPTY,
    ),
    spec(
        "lua",
        &["lua"],
        "tree-sitter-lua",
        PENDING_NO_TYPES_CAPABILITIES,
        parser_lua,
        LUA_DOCS,
    ),
    spec(
        "qml",
        &["qml"],
        "tree-sitter-qmljs",
        PENDING_NO_TYPES_CAPABILITIES,
        parser_qml,
        EMPTY,
    ),
    spec(
        "r",
        &["r", "R"],
        "tree-sitter-r",
        PENDING_NO_TYPES_CAPABILITIES,
        parser_r,
        R_DOCS,
    ),
    spec(
        "bash",
        &["sh", "bash"],
        "tree-sitter-bash",
        FULL_CAPABILITIES,
        parser_bash,
        HASH_DOCS,
    ),
    spec(
        "powershell",
        &["ps1", "psm1", "psd1"],
        "tree-sitter-powershell",
        FULL_CAPABILITIES,
        parser_powershell,
        EMPTY,
    ),
    spec(
        "gdscript",
        &["gd"],
        "tree-sitter-gdscript",
        FULL_CAPABILITIES,
        parser_gdscript,
        GDSCRIPT_DOCS,
    ),
    spec(
        "razor",
        &["razor", "cshtml"],
        "tree-sitter-razor",
        NO_PENDING_CAPABILITIES,
        parser_razor,
        RAZOR_DOCS,
    ),
    spec(
        "sql",
        &["sql"],
        "tree-sitter-sequel",
        NO_PENDING_CAPABILITIES,
        parser_sql,
        SQL_DOCS,
    ),
    spec(
        "regex",
        &["regex"],
        "tree-sitter-regex",
        NO_PENDING_CAPABILITIES,
        parser_regex,
        EMPTY,
    ),
    spec(
        "markdown",
        &["md", "markdown"],
        "tree-sitter-md",
        RELATIONSHIP_DATA_CAPABILITIES,
        parser_markdown,
        EMPTY,
    ),
    spec(
        "json",
        &["json", "jsonl", "jsonc"],
        "tree-sitter-json",
        DATA_ONLY_CAPABILITIES,
        parser_json,
        EMPTY,
    ),
    spec(
        "toml",
        &["toml"],
        "tree-sitter-toml-ng",
        DATA_ONLY_CAPABILITIES,
        parser_toml,
        EMPTY,
    ),
    spec(
        "yaml",
        &["yml", "yaml"],
        "tree-sitter-yaml",
        RELATIONSHIP_DATA_CAPABILITIES,
        parser_yaml,
        EMPTY,
    ),
];

const fn spec(
    name: &'static str,
    extensions: &'static [&'static str],
    parser_crate: &'static str,
    capabilities: LanguageCapabilities,
    parser: ParserFn,
    doc_comment_styles: &'static [DocCommentStyle],
) -> LanguageSpec {
    LanguageSpec {
        name,
        aliases: &[],
        extensions,
        parser_crate,
        capabilities,
        parser,
        doc_comment_styles,
    }
}

pub fn language_specs() -> &'static [LanguageSpec] {
    LANGUAGE_SPECS
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
