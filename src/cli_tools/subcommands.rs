//! CLI subcommand definitions for Julie's shell-first tool surface.
//!
//! Each named subcommand is an ergonomic alias over the same underlying MCP tool
//! structs. The generic `Tool` variant is the fallback for any tool by name.

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
// Tool subcommands
// ---------------------------------------------------------------------------

/// Tool commands exposed as top-level CLI subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum ToolCommand {
    /// Search code, symbols, or file paths
    Search(SearchArgs),

    /// Find all references to a symbol
    Refs(RefsArgs),

    /// List symbols in a file
    Symbols(SymbolsArgs),

    /// Get token-budgeted context for a concept or task
    Context(ContextArgs),

    /// Analyze blast radius of changes
    BlastRadius(BlastRadiusArgs),

    /// Manage workspaces (index, list, stats, health, etc.)
    Workspace(WorkspaceArgs),

    /// Run any tool by name with JSON params (generic fallback)
    Tool(GenericToolArgs),
}

// ---------------------------------------------------------------------------
// search
// ---------------------------------------------------------------------------

/// Search code, symbols, or file paths.
///
/// Examples:
///   julie-server search "FastSearchTool"
///   julie-server search "parse" --target definitions --language rust
///   julie-server search "*.rs" --target files
#[derive(Debug, Clone, Parser)]
pub struct SearchArgs {
    /// Search query
    pub query: String,

    /// Search target: content, definitions, or files
    #[arg(short = 't', long, default_value = "content")]
    pub target: String,

    /// Maximum results (default: 10)
    #[arg(short = 'n', long, default_value = "10")]
    pub limit: u32,

    /// Language filter (e.g. rust, typescript, python)
    #[arg(short = 'l', long)]
    pub language: Option<String>,

    /// File pattern filter (glob syntax, e.g. "src/**/*.rs")
    #[arg(short = 'f', long)]
    pub file_pattern: Option<String>,

    /// Context lines before/after a content match
    #[arg(short = 'C', long)]
    pub context_lines: Option<u32>,

    /// Exclude test symbols from results
    #[arg(short = 'T', long)]
    pub exclude_tests: bool,
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

    /// Narrow by reference kind: call, variable_ref, type_usage, member_access, import
    #[arg(short = 'k', long)]
    pub kind: Option<String>,

    /// Filter references to a specific file path
    #[arg(short = 'f', long)]
    pub file_path: Option<String>,

    /// Filter references by file pattern (glob syntax)
    #[arg(short = 'p', long)]
    pub file_pattern: Option<String>,

    /// Maximum references (default: 10)
    #[arg(short = 'n', long, default_value = "10")]
    pub limit: u32,
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
// blast-radius
// ---------------------------------------------------------------------------

/// Analyze what would break if symbols or files change.
///
/// Examples:
///   julie-server blast-radius --files src/cli.rs
///   julie-server blast-radius --symbols FastSearchTool --format markdown
///   julie-server blast-radius --rev HEAD~3
#[derive(Debug, Clone, Parser)]
pub struct BlastRadiusArgs {
    /// Git revision or range (e.g. HEAD~3, abc123..def456)
    #[arg(short = 'r', long)]
    pub rev: Option<String>,

    /// File paths to analyze (comma-separated)
    #[arg(short = 'f', long, value_delimiter = ',')]
    pub files: Option<Vec<String>>,

    /// Symbol names to analyze (comma-separated)
    #[arg(short = 's', long, value_delimiter = ',')]
    pub symbols: Option<Vec<String>>,

    /// Output format for blast radius results
    #[arg(long)]
    pub format: Option<String>,
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
// tool (generic)
// ---------------------------------------------------------------------------

/// Run any tool by name with JSON parameters.
///
/// Examples:
///   julie-server tool fast_search --params '{"query":"main","search_target":"definitions"}'
///   julie-server tool deep_dive --params '{"symbol":"Command","depth":"full"}'
#[derive(Debug, Clone, Parser)]
pub struct GenericToolArgs {
    /// Tool name (e.g. fast_search, deep_dive, get_symbols, blast_radius)
    pub name: String,

    /// JSON-encoded tool parameters
    #[arg(short = 'p', long, default_value = "{}")]
    pub params: String,
}
