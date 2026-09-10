//! CLI subcommand definitions for Julie's shell-first tool surface.

use clap::{Parser, Subcommand, ValueEnum};

// ---------------------------------------------------------------------------
// Output format shared across all tool commands
// ---------------------------------------------------------------------------

/// Output format for tool results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
    Markdown,
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutputFormat::Text => write!(f, "text"),
            OutputFormat::Json => write!(f, "json"),
            OutputFormat::Markdown => write!(f, "markdown"),
        }
    }
}

// ---------------------------------------------------------------------------
// Global tool flags (mixed into Cli)
// ---------------------------------------------------------------------------

/// Global flags that apply to all tool subcommands.
#[derive(Debug, Clone, Parser)]
pub struct GlobalToolFlags {
    /// Output JSON (shorthand for --format json)
    #[arg(long, global = true)]
    pub json: bool,

    /// Output format: text, json, or markdown
    #[arg(long, global = true, value_enum)]
    pub format: Option<OutputFormat>,

    /// Run without a daemon (single-shot indexing, then execute)
    #[arg(long, global = true)]
    pub standalone: bool,

    /// Semantic retrieval mode: off, auto, or required
    #[arg(long, global = true, value_enum)]
    pub semantics: Option<crate::request_engine::SemanticMode>,
}

impl GlobalToolFlags {
    /// Resolve effective output format. `--json` takes precedence when `--format` is not set.
    pub fn effective_format(&self) -> OutputFormat {
        if let Some(fmt) = self.format {
            fmt
        } else if self.json {
            OutputFormat::Json
        } else {
            OutputFormat::Text
        }
    }
}

// ---------------------------------------------------------------------------
// search
// ---------------------------------------------------------------------------

/// Search code and symbols using unified search.
#[derive(Debug, Clone, Parser)]
pub struct SearchArgs {
    /// Search query
    pub query: String,
    /// Maximum results (default: 10)
    #[arg(short = 'n', long, default_value = "10")]
    pub limit: u32,
    /// Language filter (e.g. rust, typescript, python)
    #[arg(short = 'l', long)]
    pub language: Option<String>,
    /// File pattern filter (glob syntax, e.g. "src/**/*.rs")
    #[arg(short = 'f', long)]
    pub file_pattern: Option<String>,
    /// Context lines before/after a match
    #[arg(short = 'C', long)]
    pub context_lines: Option<u32>,
    /// Exclude test symbols from results
    #[arg(short = 'T', long)]
    pub exclude_tests: bool,
    /// Restrict content matches to stored source-region kinds.
    #[arg(long)]
    pub regions: Option<String>,
    /// Deprecated target selector (retained as no-op for compatibility)
    #[arg(short = 't', long, hide = true)]
    pub target: Option<String>,
}

// ---------------------------------------------------------------------------
// refs
// ---------------------------------------------------------------------------

/// Find all references to a symbol across the codebase.
#[derive(Debug, Clone, Parser)]
pub struct RefsArgs {
    /// Symbol name (supports qualified names like Processor::process)
    pub symbol: String,
    /// Include definitions in results (default: true)
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set, value_parser = clap::builder::BoolishValueParser::new())]
    pub include_definition: bool,
    /// Maximum references (default: 10)
    #[arg(short = 'n', long, default_value = "10")]
    pub limit: u32,
    /// Tool workspace target: primary or a workspace id
    #[arg(id = "target_workspace", long = "target-workspace")]
    pub workspace: Option<String>,
    /// Narrow by reference kind: call, variable_ref, type_usage, member_access, import
    #[arg(short = 'k', long)]
    pub kind: Option<String>,
}

// ---------------------------------------------------------------------------
// symbols
// ---------------------------------------------------------------------------

/// List symbols (functions, structs, etc.) in a file.
#[derive(Debug, Clone, Parser)]
pub struct SymbolsArgs {
    /// File path (relative to workspace root)
    pub file_path: String,
    /// Reading mode: structure, minimal, or full
    #[arg(short = 'm', long, default_value = "structure")]
    pub mode: String,
    /// Filter to a specific symbol name (partial match)
    #[arg(short = 't', long)]
    pub target: Option<String>,
    /// Maximum symbols to return (default: 50)
    #[arg(short = 'n', long, default_value = "50")]
    pub limit: u32,
    /// Maximum nesting depth (0=top-level, 1=include methods, 2+=deeper)
    #[arg(short = 'd', long, default_value = "1")]
    pub max_depth: u32,
}

// ---------------------------------------------------------------------------
// context
// ---------------------------------------------------------------------------

/// Get token-budgeted context for a concept or task.
#[derive(Debug, Clone, Parser)]
pub struct ContextArgs {
    /// Search query (text or pattern)
    pub query: String,
    /// Token budget override (default: auto-scaled 2000-4000)
    #[arg(short = 'b', long)]
    pub budget: Option<u32>,
    /// Maximum graph hop depth (default: 1)
    #[arg(long)]
    pub max_hops: Option<u32>,
    /// Explicit symbol entry points (comma-separated)
    #[arg(short = 'e', long, value_delimiter = ',')]
    pub entry_symbols: Option<Vec<String>>,
    /// Include test-linked symbols in neighbor slots
    #[arg(long)]
    pub prefer_tests: bool,
}

// ---------------------------------------------------------------------------
// call-path
// ---------------------------------------------------------------------------

/// Trace one shortest call-graph path between two symbols.
#[derive(Debug, Clone, Parser)]
pub struct CallPathArgs {
    /// Source symbol name
    pub from: String,
    /// Target symbol name
    pub to: String,
    /// Maximum relationship hops to traverse
    #[arg(long, default_value = "6")]
    pub max_hops: u32,
    /// Tool workspace target
    #[arg(id = "target_workspace", long = "target-workspace")]
    pub workspace: Option<String>,
    /// File path hint for source symbol
    #[arg(long = "from-file")]
    pub from_file_path: Option<String>,
    /// File path hint for target symbol
    #[arg(long = "to-file")]
    pub to_file_path: Option<String>,
}

// ---------------------------------------------------------------------------
// blast-radius
// ---------------------------------------------------------------------------

/// Analyze what would break if files, internal symbol IDs, or revisions change.
#[derive(Debug, Clone, Parser)]
#[command(after_help = "Examples:
  julie-server blast-radius --files src/cli.rs
  julie-server blast-radius --symbols sym_1234abcd --report-format readable
  julie-server blast-radius --format markdown
  julie-server blast-radius --rev HEAD~3

Prefer --files when you know a symbol name or file path. --symbols accepts internal Julie symbol IDs only.")]
pub struct BlastRadiusArgs {
    /// Git revision or range (e.g. HEAD~3, abc123..def456)
    #[arg(short = 'r', long)]
    pub rev: Option<String>,
    /// File paths to analyze (comma-separated)
    #[arg(short = 'f', long, value_delimiter = ',')]
    pub files: Option<Vec<String>>,
    /// Internal symbol IDs to analyze (comma-separated).
    /// Prefer --files when you know a symbol name or file path.
    #[arg(short = 's', long, value_delimiter = ',')]
    pub symbols: Option<Vec<String>>,
    /// Blast-radius text layout: readable or compact
    #[arg(long = "report-format", value_parser = ["readable", "compact"])]
    pub report_format: Option<String>,
}

// ---------------------------------------------------------------------------
// patterns
// ---------------------------------------------------------------------------

/// Query generic code-shape facts extracted across supported languages.
#[derive(Debug, Clone, Parser)]
pub struct PatternsArgs {
    /// Operation: list, summary, or search
    #[arg(long, default_value = "list")]
    pub operation: String,
    /// Exact structural pattern ID
    #[arg(long)]
    pub pattern_id: Option<String>,
    /// Case-insensitive substring matched against observed pattern IDs
    #[arg(long)]
    pub query: Option<String>,
    /// Workspace-relative glob filter
    #[arg(long)]
    pub path: Option<String>,
    /// Language filter
    #[arg(long)]
    pub language: Option<String>,
    /// Top-level metadata equality filter, repeatable
    #[arg(long = "where", value_name = "KEY=VALUE")]
    pub where_filters: Vec<String>,
    /// Summary metadata facet key
    #[arg(long)]
    pub facet: Option<String>,
    /// Summary grouping: language_pattern_capture, file, or directory
    #[arg(long, default_value = "language_pattern_capture")]
    pub group_by: String,
    /// Maximum search or summary rows
    #[arg(long, default_value = "50")]
    pub limit: u32,
}

// ---------------------------------------------------------------------------
// workspace
// ---------------------------------------------------------------------------

/// Manage workspaces (index, list, stats, health, etc.).
#[derive(Debug, Clone, Parser)]
pub struct WorkspaceArgs {
    /// Operation: index, list, register, remove, stats, clean, refresh, open, health
    pub operation: String,
    /// Path to workspace (used by: index, register, open)
    #[arg(short = 'p', long)]
    pub path: Option<String>,
    /// Force complete re-indexing (used by: index, refresh, open)
    #[arg(long)]
    pub force: bool,
    /// Display name for workspace metadata (used by: register)
    #[arg(short = 'n', long)]
    pub name: Option<String>,
    /// Run in foreground mode (required for interactive dashboard)
    #[arg(long)]
    pub foreground: bool,
    /// Edit ID for recover_edit operation
    #[arg(long)]
    pub edit_id: Option<String>,
    /// Recovery action for recover_edit operation (resume or rollback)
    #[arg(long, value_name = "ACTION")]
    pub action: Option<String>,
}

// ---------------------------------------------------------------------------
// signals (early warning report)
// ---------------------------------------------------------------------------

/// Generate an early warning signals report from annotation-derived data.
#[derive(Debug, Clone, Parser)]
pub struct SignalsArgs {
    /// Scope analysis to files matching this glob pattern
    #[arg(long)]
    pub file_pattern: Option<String>,
    /// Bypass cache and regenerate the report
    #[arg(long)]
    pub fresh: bool,
    /// Maximum rows per report section
    #[arg(long)]
    pub limit: Option<usize>,
}

// ---------------------------------------------------------------------------
// tool (generic)
// ---------------------------------------------------------------------------

/// Run any tool by name with JSON parameters.
#[derive(Debug, Clone, Parser)]
#[command(group(
    clap::ArgGroup::new("parameter_input")
        .args(["params", "params_file", "params_stdin"])
        .multiple(false)
))]
pub struct GenericToolArgs {
    /// Tool name (e.g. fast_search, deep_dive, get_symbols, call_path, blast_radius)
    pub name: String,
    /// JSON-encoded tool parameters
    #[arg(short = 'p', long)]
    pub params: Option<String>,
    /// Path to file containing JSON parameters
    #[arg(long, value_name = "PATH")]
    pub params_file: Option<std::path::PathBuf>,
    /// Read JSON parameters from standard input
    #[arg(long)]
    pub params_stdin: bool,
    /// Run in foreground mode (required for interactive dashboard)
    #[arg(long)]
    pub foreground: bool,
}

impl GenericToolArgs {
    /// Helper constructor for tests and programmatic construction with inline params.
    pub fn from_params(name: impl Into<String>, params: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            params: Some(params.into()),
            params_file: None,
            params_stdin: false,
            foreground: false,
        }
    }

    /// Resolve parameters according to the CLI input contract.
    pub fn resolve_params(
        &self,
    ) -> Result<serde_json::Map<String, serde_json::Value>, crate::request_engine::RequestFailure>
    {
        crate::cli_tools::input::resolve_parameter_map(
            self.params.as_deref(),
            self.params_file.as_deref(),
            self.params_stdin,
        )
    }
}

// ---------------------------------------------------------------------------
// tools (discovery and replay)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Parser)]
pub struct ToolsArgs {
    #[command(subcommand)]
    pub command: ToolsSubcommand,
}

#[derive(Debug, Clone, Subcommand)]
pub enum ToolsSubcommand {
    /// List all 13 tools and schemas
    List,
    /// Output JSON Schema for a specific tool
    Schema(ToolSchemaArgs),
    /// Replay requests from a JSONL file or stdin
    Replay(ReplayArgs),
}

#[derive(Debug, Clone, Parser)]
pub struct ToolSchemaArgs {
    /// Tool name
    pub name: String,
}

#[derive(Debug, Clone, Parser)]
pub struct ReplayArgs {
    /// Input JSONL file path, or "-" for stdin
    #[arg(short = 'i', long)]
    pub input: std::path::PathBuf,
    /// Timeout per request in milliseconds (default: 30000)
    #[arg(long, default_value = "30000")]
    pub timeout_ms: u64,
}

// ---------------------------------------------------------------------------
// deep-dive
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Parser)]
pub struct DeepDiveArgs {
    /// Symbol name to investigate
    pub symbol: String,
    /// Investigation depth: overview, callers, callees, references, types, full
    #[arg(short = 'd', long, default_value = "overview")]
    pub depth: Option<String>,
    /// Optional context file to resolve ambiguous symbols
    #[arg(short = 'c', long)]
    pub context_file: Option<String>,
    /// Target workspace path
    #[arg(id = "target_workspace", long = "target-workspace")]
    pub workspace: Option<String>,
}

// ---------------------------------------------------------------------------
// edit
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Parser)]
pub struct EditArgs {
    /// Relative or absolute path to the file to edit
    pub file_path: String,
    /// Existing text to replace
    #[arg(short = 'o', long)]
    pub old_text: String,
    /// Replacement text
    #[arg(short = 'n', long)]
    pub new_text: String,
    /// Preview changes without applying
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set, value_parser = clap::builder::BoolishValueParser::new())]
    pub dry_run: bool,
    /// Occurrence to replace: first, all, or specific index
    #[arg(long, default_value = "first")]
    pub occurrence: Option<String>,
    /// Target workspace path
    #[arg(id = "target_workspace", long = "target-workspace")]
    pub workspace: Option<String>,
}

// ---------------------------------------------------------------------------
// rename
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Parser)]
pub struct RenameArgs {
    /// Current name of the symbol to rename
    pub old_name: String,
    /// New name for the symbol
    pub new_name: String,
    /// Scope of rename: file or workspace
    #[arg(short = 's', long)]
    pub scope: Option<String>,
    /// Preview changes without applying
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set, value_parser = clap::builder::BoolishValueParser::new())]
    pub dry_run: bool,
    /// Target workspace path
    #[arg(id = "target_workspace", long = "target-workspace")]
    pub workspace: Option<String>,
}

// ---------------------------------------------------------------------------
// rewrite
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Parser)]
pub struct RewriteArgs {
    /// Symbol name to rewrite
    pub symbol: String,
    /// Rewrite operation
    #[arg(short = 'o', long, default_value = "replace_full")]
    pub operation: String,
    /// New content for the symbol
    #[arg(short = 'c', long)]
    pub content: String,
    /// File path to disambiguate symbol
    #[arg(short = 'f', long)]
    pub file_path: Option<String>,
    /// Preview changes without applying
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set, value_parser = clap::builder::BoolishValueParser::new())]
    pub dry_run: bool,
    /// Target workspace path
    #[arg(id = "target_workspace", long = "target-workspace")]
    pub workspace: Option<String>,
}
