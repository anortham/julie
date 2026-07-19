//! CLI subcommand definitions for Julie's shell-first tool surface.
//!
//! Each named subcommand is an ergonomic alias over the same underlying MCP tool
//! structs. The generic `Tool` variant is the fallback for any tool by name.

use clap::{Parser, ValueEnum};

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
}

impl GlobalToolFlags {
    /// Resolve the effective output format. `--json` is a shorthand that
    /// takes precedence when `--format` is not set.
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
///
/// Examples:
///   julie-server search "FastSearchTool"
///   julie-server search "parse" --language rust
///   julie-server search "browser_client.rs"
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

    /// Deprecated and accepted as a no-op since T8 unified-search cutover.
    /// Older harnesses (e.g. the eros bakeoff comparator) still pass
    /// `--target definitions|files|content`; we keep the flag so they can run
    /// against the current unified path without a clap parse error. The value
    /// is read but otherwise ignored — every query routes through the same
    /// unified path regardless.
    #[arg(short = 't', long, hide = true)]
    pub target: Option<String>,
}

// ---------------------------------------------------------------------------
// refs
// ---------------------------------------------------------------------------

/// Find all references to a symbol across the codebase.
///
/// Examples:
///   julie-server refs "FastSearchTool"
///   julie-server refs "Command" --kind call --limit 20
#[derive(Debug, Clone, Parser)]
pub struct RefsArgs {
    /// Symbol name (supports qualified names like Processor::process)
    pub symbol: String,

    /// Include definitions in results (default: true)
    #[arg(
        long,
        default_value_t = true,
        action = clap::ArgAction::Set,
        value_parser = clap::builder::BoolishValueParser::new()
    )]
    pub include_definition: bool,

    /// Maximum references (default: 10)
    #[arg(short = 'n', long, default_value = "10")]
    pub limit: u32,

    /// Tool workspace target: primary or a workspace id opened through manage_workspace
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
///
/// Examples:
///   julie-server symbols src/cli.rs
///   julie-server symbols src/tools/search/mod.rs --target FastSearchTool --mode minimal
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
///
/// Examples:
///   julie-server context "CLI command parsing"
///   julie-server context "search scoring" --budget 4000 --max-hops 2
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
///
/// Examples:
///   julie-server call-path "LoginButton::onClick" "insert_session"
///   julie-server call-path handle_request write_response --from-file src/server.rs --to-file src/response.rs
#[derive(Debug, Clone, Parser)]
pub struct CallPathArgs {
    /// Source symbol name
    pub from: String,

    /// Target symbol name
    pub to: String,

    /// Maximum relationship hops to traverse
    #[arg(long, default_value = "6")]
    pub max_hops: u32,

    /// Tool workspace target: primary or a workspace id opened through manage_workspace
    #[arg(id = "target_workspace", long = "target-workspace")]
    pub workspace: Option<String>,

    /// File path hint for the source symbol when names are ambiguous
    #[arg(long = "from-file")]
    pub from_file_path: Option<String>,

    /// File path hint for the target symbol when names are ambiguous
    #[arg(long = "to-file")]
    pub to_file_path: Option<String>,
}

// ---------------------------------------------------------------------------
// blast-radius
// ---------------------------------------------------------------------------

/// Analyze what would break if files, internal symbol IDs, or revisions change.
///
/// Examples:
///   julie-server blast-radius --files src/cli.rs
///   julie-server blast-radius --symbols sym_1234abcd --report-format readable
///   julie-server blast-radius --format markdown
///   julie-server blast-radius --rev HEAD~3
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
///
/// Examples:
///   julie-server workspace index
///   julie-server workspace stats
///   julie-server workspace health --force
///   julie-server workspace register --path /code/myproject --name "My Project"
///
/// Note: `open`, `register`, `remove`, `refresh`, `stats`, and `dashboard`
/// require either the MCP `manage_workspace` tool or a dedicated CLI entry
/// point, not the one-shot standalone workspace wrapper.
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
}

// ---------------------------------------------------------------------------
// signals (early warning report)
// ---------------------------------------------------------------------------

/// Generate an early warning signals report from annotation-derived data.
///
/// Identifies entry points, auth coverage gaps, and review markers.
/// Output uses structural signal language, not vulnerability claims.
///
/// Examples:
///   julie-server signals --standalone
///   julie-server signals --file-pattern "src/api/**" --standalone --json
///   julie-server signals --fresh --standalone --format markdown
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
///
/// Examples:
///   julie-server tool fast_search --params '{"query":"main","limit":5}'
///   julie-server tool deep_dive --params '{"symbol":"Command","depth":"full"}'
///   julie-server tool call_path --params '{"from":"handle_request","to":"write_response"}'
#[derive(Debug, Clone, Parser)]
pub struct GenericToolArgs {
    /// Tool name (e.g. fast_search, deep_dive, get_symbols, call_path, blast_radius)
    pub name: String,

    /// JSON-encoded tool parameters
    #[arg(short = 'p', long, default_value = "{}")]
    pub params: String,
}
