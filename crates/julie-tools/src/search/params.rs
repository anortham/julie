use julie_core::mcp_compat::CallToolResult;
use schemars::JsonSchema;
use serde::de::{Deserializer, Error as DeError, IntoDeserializer};
use serde::{Deserialize, Serialize};

use super::backend::SearchBackend;
use super::trace::SearchExecutionResult;

pub(crate) const MIN_LIMIT: u32 = 1;
pub(crate) const MAX_LIMIT: u32 = 500;

#[derive(Debug, Clone, Serialize, JsonSchema)]
/// Search code and symbols using unified code-aware full-text search. Supports multi-word queries with AND/OR logic, exact symbol name matches, file-path fragments, and conceptual semantic search. Optional backend: omitted/default lexical returns mixed file+symbol hits and may show labeled semantic fallback candidates on identifier-like zero-hit queries when embeddings are ready; explicit "lexical" stays pure lexical; "semantic" and "hybrid" are symbol-only concept search. Use lexical for file/path queries.
pub struct FastSearchTool {
    /// Search query. Exact symbol names, file path fragments, and natural-language descriptions all work. Too many results? Add file_pattern or language filter. Zero lexical results may show labeled semantic fallback candidates for identifier-like queries when backend is omitted and embeddings are ready. Still zero? Run manage_workspace(operation="index")
    pub query: String,
    /// Language filter: "rust", "typescript", "javascript", "python", "java", "csharp", "vbnet", "php", "ruby", "swift", "kotlin", "scala", "c", "cpp", "go", "lua", "zig", "elixir", "erlang", "gdscript", "vue", "qml", "r", "razor", "sql", "html", "css", "regex", "bash", "powershell", "dart", "markdown", "json", "toml", "yaml", "xml"
    #[serde(default)]
    pub language: Option<String>,
    /// File pattern filter (glob syntax)
    #[serde(default)]
    pub file_pattern: Option<String>,
    /// Maximum results (default: 6, range: 1-500)
    #[serde(
        default = "default_limit",
        deserialize_with = "deserialize_limit_lenient_clamped"
    )]
    pub limit: u32,
    /// Context lines before/after a match (default: 1)
    #[serde(
        default = "default_context_lines",
        deserialize_with = "julie_core::serde_lenient::deserialize_option_u32_lenient"
    )]
    pub context_lines: Option<u32>,
    /// Exclude test symbols from results.
    /// Default: auto (excludes for NL queries, includes for symbol searches).
    /// Set explicitly to override.
    #[serde(
        default,
        deserialize_with = "julie_core::serde_lenient::deserialize_option_bool_lenient"
    )]
    pub exclude_tests: Option<bool>,
    /// Search backend: omitted/default lexical uses BM25/full-text mixed file+symbol hits and may show labeled semantic fallback candidates on identifier-like zero-hit queries when embeddings are ready; explicit "lexical" stays pure lexical; "semantic" uses KNN symbol search; "hybrid" uses BM25+KNN symbol search. Semantic and hybrid are symbol-only; use lexical for file/path queries.
    #[serde(default)]
    pub backend: Option<SearchBackend>,
    /// Workspace filter: "primary" (default) or a workspace ID
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,
    /// Return format: "compact" (default, one line per hit grouped by file) or "full" (code context and rich summaries)
    #[serde(default = "default_return_format")]
    pub return_format: String,
    /// Skip this many hits before keeping `limit` rows (default: 0)
    #[serde(
        default,
        deserialize_with = "julie_core::serde_lenient::deserialize_u32_lenient"
    )]
    pub offset: u32,
    /// Optional semantic mode override (Auto, Off, Required)
    #[serde(default)]
    pub semantics: Option<julie_core::embeddings_contract::SemanticMode>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct FastSearchParams {
    #[serde(flatten)]
    pub search: FastSearchTool,
    /// Restrict line-level lexical matches to stored source-region kinds.
    #[serde(default)]
    pub regions: Option<String>,
}

impl From<FastSearchTool> for FastSearchParams {
    fn from(search: FastSearchTool) -> Self {
        Self {
            search,
            regions: None,
        }
    }
}

#[derive(Deserialize)]
struct FastSearchToolSerde {
    query: String,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    file_pattern: Option<String>,
    #[serde(
        default = "default_limit",
        deserialize_with = "deserialize_limit_lenient_clamped"
    )]
    limit: u32,
    #[serde(default, deserialize_with = "deserialize_presence_tracked_option_u32")]
    context_lines: Option<Option<u32>>,
    #[serde(
        default,
        deserialize_with = "julie_core::serde_lenient::deserialize_option_bool_lenient"
    )]
    exclude_tests: Option<bool>,
    #[serde(default)]
    backend: Option<SearchBackend>,
    #[serde(default = "default_workspace")]
    workspace: Option<String>,
    #[serde(default = "default_return_format")]
    return_format: String,
    #[serde(
        default,
        deserialize_with = "julie_core::serde_lenient::deserialize_u32_lenient"
    )]
    offset: u32,
    #[serde(default)]
    semantics: Option<julie_core::embeddings_contract::SemanticMode>,
}

impl<'de> Deserialize<'de> for FastSearchTool {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = FastSearchToolSerde::deserialize(deserializer)?;
        let context_lines = match raw.context_lines {
            Some(value) => value,
            None => default_context_lines(),
        };

        Ok(Self {
            query: raw.query,
            language: raw.language,
            file_pattern: raw.file_pattern,
            limit: raw.limit,
            context_lines,
            exclude_tests: raw.exclude_tests,
            backend: raw.backend,
            workspace: raw.workspace,
            return_format: raw.return_format,
            offset: raw.offset,
            semantics: raw.semantics,
        })
    }
}

pub(crate) fn default_limit() -> u32 {
    6 // Higher search quality makes a smaller MCP default enough for normal agent use.
}

pub(crate) fn clamp_limit(limit: u32) -> u32 {
    limit.clamp(MIN_LIMIT, MAX_LIMIT)
}

fn deserialize_limit_lenient_clamped<'de, D>(deserializer: D) -> std::result::Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    let parsed = match value {
        None => default_limit(),
        Some(value) => {
            let parsed =
                julie_core::serde_lenient::deserialize_u32_lenient(value.into_deserializer())
                    .map_err(D::Error::custom)?;
            clamp_limit(parsed)
        }
    };
    Ok(parsed)
}

fn default_context_lines() -> Option<u32> {
    Some(1)
}

fn default_workspace() -> Option<String> {
    Some("primary".to_string())
}

fn default_return_format() -> String {
    "compact".to_string()
}

fn deserialize_presence_tracked_option_u32<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<Option<u32>>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    match value {
        None => Ok(None),
        Some(value) => {
            let parsed = julie_core::serde_lenient::deserialize_option_u32_lenient(
                value.into_deserializer(),
            )
            .map_err(D::Error::custom)?;
            Ok(Some(parsed))
        }
    }
}

impl Default for FastSearchTool {
    fn default() -> Self {
        Self {
            query: String::new(),
            language: None,
            file_pattern: None,
            limit: default_limit(),
            context_lines: default_context_lines(),
            exclude_tests: None,
            backend: None,
            workspace: default_workspace(),
            return_format: default_return_format(),
            offset: 0,
            semantics: None,
        }
    }
}

impl FastSearchTool {
    pub fn validated_format(&self) -> anyhow::Result<&str> {
        match self.return_format.as_str() {
            "compact" | "full" => Ok(self.return_format.as_str()),
            other => anyhow::bail!("Invalid return_format: '{other}'. Expected compact or full"),
        }
    }

    pub fn fetch_limit(&self) -> u32 {
        let needed = self
            .effective_limit()
            .saturating_add(self.offset)
            .saturating_add(1);
        clamp_limit(needed)
    }
}

pub struct FastSearchExecution {
    pub result: CallToolResult,
    pub execution: Option<SearchExecutionResult>,
}
