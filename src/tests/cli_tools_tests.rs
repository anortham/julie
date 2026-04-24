//! Tests for CLI tool subcommand parsing (clap).
//!
//! Validates that each named wrapper and the generic tool command parse
//! arguments correctly, including defaults, long flags, and short flags.

use crate::cli_tools::subcommands::*;
use clap::{CommandFactory, Parser};

// ---------------------------------------------------------------------------
// Structural validation
// ---------------------------------------------------------------------------

/// Verify all subcommands produce valid clap definitions (catches
/// conflicting short flags, missing attributes, etc.)
#[test]
fn test_tool_command_debug_assert() {
    use crate::cli::Cli;
    Cli::command().debug_assert();
}

// ---------------------------------------------------------------------------
// search
// ---------------------------------------------------------------------------

#[test]
fn test_search_defaults() {
    let args = SearchArgs::parse_from(["search", "hello"]);
    assert_eq!(args.query, "hello");
    assert_eq!(args.target, "content");
    assert_eq!(args.limit, 10);
    assert!(args.language.is_none());
    assert!(args.file_pattern.is_none());
    assert!(args.context_lines.is_none());
    assert!(!args.exclude_tests);
}

#[test]
fn test_search_all_flags() {
    let args = SearchArgs::parse_from([
        "search",
        "parse_token",
        "--target",
        "definitions",
        "--limit",
        "20",
        "--language",
        "rust",
        "--file-pattern",
        "src/**/*.rs",
        "--context-lines",
        "3",
        "--exclude-tests",
    ]);
    assert_eq!(args.query, "parse_token");
    assert_eq!(args.target, "definitions");
    assert_eq!(args.limit, 20);
    assert_eq!(args.language.as_deref(), Some("rust"));
    assert_eq!(args.file_pattern.as_deref(), Some("src/**/*.rs"));
    assert_eq!(args.context_lines, Some(3));
    assert!(args.exclude_tests);
}

#[test]
fn test_search_short_flags() {
    let args = SearchArgs::parse_from([
        "search", "query", "-t", "files", "-n", "5", "-l", "python", "-f", "*.py", "-C", "2", "-T",
    ]);
    assert_eq!(args.target, "files");
    assert_eq!(args.limit, 5);
    assert_eq!(args.language.as_deref(), Some("python"));
    assert_eq!(args.file_pattern.as_deref(), Some("*.py"));
    assert_eq!(args.context_lines, Some(2));
    assert!(args.exclude_tests);
}

// ---------------------------------------------------------------------------
// refs
// ---------------------------------------------------------------------------

#[test]
fn test_refs_defaults() {
    let args = RefsArgs::parse_from(["refs", "Command"]);
    assert_eq!(args.symbol, "Command");
    assert!(args.kind.is_none());
    assert_eq!(args.limit, 10);
}

#[test]
fn test_refs_all_flags() {
    let args = RefsArgs::parse_from(["refs", "FastSearchTool", "--kind", "call", "--limit", "25"]);
    assert_eq!(args.symbol, "FastSearchTool");
    assert_eq!(args.kind.as_deref(), Some("call"));
    assert_eq!(args.limit, 25);
}

#[test]
fn test_refs_short_flags() {
    let args = RefsArgs::parse_from(["refs", "Sym", "-k", "type_usage", "-n", "3"]);
    assert_eq!(args.kind.as_deref(), Some("type_usage"));
    assert_eq!(args.limit, 3);
}

// ---------------------------------------------------------------------------
// symbols
// ---------------------------------------------------------------------------

#[test]
fn test_symbols_defaults() {
    let args = SymbolsArgs::parse_from(["symbols", "src/cli.rs"]);
    assert_eq!(args.file_path, "src/cli.rs");
    assert_eq!(args.mode, "structure");
    assert!(args.target.is_none());
    assert_eq!(args.limit, 50);
    assert_eq!(args.max_depth, 1);
}

#[test]
fn test_symbols_all_flags() {
    let args = SymbolsArgs::parse_from([
        "symbols",
        "src/tools/search/mod.rs",
        "--mode",
        "minimal",
        "--target",
        "FastSearchTool",
        "--limit",
        "10",
        "--max-depth",
        "2",
    ]);
    assert_eq!(args.file_path, "src/tools/search/mod.rs");
    assert_eq!(args.mode, "minimal");
    assert_eq!(args.target.as_deref(), Some("FastSearchTool"));
    assert_eq!(args.limit, 10);
    assert_eq!(args.max_depth, 2);
}

#[test]
fn test_symbols_short_flags() {
    let args = SymbolsArgs::parse_from([
        "symbols", "f.rs", "-m", "full", "-t", "Foo", "-n", "5", "-d", "0",
    ]);
    assert_eq!(args.mode, "full");
    assert_eq!(args.target.as_deref(), Some("Foo"));
    assert_eq!(args.limit, 5);
    assert_eq!(args.max_depth, 0);
}

// ---------------------------------------------------------------------------
// context
// ---------------------------------------------------------------------------

#[test]
fn test_context_defaults() {
    let args = ContextArgs::parse_from(["context", "CLI parsing"]);
    assert_eq!(args.query, "CLI parsing");
    assert!(args.budget.is_none());
    assert!(args.max_hops.is_none());
    assert!(args.entry_symbols.is_none());
    assert!(!args.prefer_tests);
}

#[test]
fn test_context_all_flags() {
    let args = ContextArgs::parse_from([
        "context",
        "search scoring",
        "--budget",
        "4000",
        "--max-hops",
        "2",
        "--entry-symbols",
        "FastSearchTool,execute_search",
        "--prefer-tests",
    ]);
    assert_eq!(args.query, "search scoring");
    assert_eq!(args.budget, Some(4000));
    assert_eq!(args.max_hops, Some(2));
    let symbols = args.entry_symbols.unwrap();
    assert_eq!(symbols, vec!["FastSearchTool", "execute_search"]);
    assert!(args.prefer_tests);
}

#[test]
fn test_context_short_flags() {
    let args = ContextArgs::parse_from(["context", "q", "-b", "2000", "-e", "Sym1,Sym2"]);
    assert_eq!(args.budget, Some(2000));
    let symbols = args.entry_symbols.unwrap();
    assert_eq!(symbols, vec!["Sym1", "Sym2"]);
}

// ---------------------------------------------------------------------------
// blast-radius
// ---------------------------------------------------------------------------

#[test]
fn test_blast_radius_empty() {
    // blast-radius with no args is valid (could default to uncommitted changes)
    let args = BlastRadiusArgs::parse_from(["blast-radius"]);
    assert!(args.rev.is_none());
    assert!(args.files.is_none());
    assert!(args.symbols.is_none());
    assert!(args.report_format.is_none());
}

#[test]
fn test_blast_radius_all_flags() {
    let args = BlastRadiusArgs::parse_from([
        "blast-radius",
        "--rev",
        "HEAD~3",
        "--files",
        "src/cli.rs,src/main.rs",
        "--symbols",
        "Command,Cli",
        "--report-format",
        "markdown",
    ]);
    assert_eq!(args.rev.as_deref(), Some("HEAD~3"));
    let files = args.files.unwrap();
    assert_eq!(files, vec!["src/cli.rs", "src/main.rs"]);
    let symbols = args.symbols.unwrap();
    assert_eq!(symbols, vec!["Command", "Cli"]);
    assert_eq!(args.report_format.as_deref(), Some("markdown"));
}

#[test]
fn test_blast_radius_short_flags() {
    let args =
        BlastRadiusArgs::parse_from(["blast-radius", "-r", "abc123", "-f", "a.rs", "-s", "Foo"]);
    assert_eq!(args.rev.as_deref(), Some("abc123"));
    let files = args.files.unwrap();
    assert_eq!(files, vec!["a.rs"]);
    let symbols = args.symbols.unwrap();
    assert_eq!(symbols, vec!["Foo"]);
}

#[test]
fn test_cli_blast_radius_global_format_flag_uses_output_format() {
    use crate::cli::{Cli, Command};

    let cli = Cli::try_parse_from(["julie-server", "blast-radius", "--format", "markdown"])
        .expect("blast-radius should accept the global output format flag");

    match cli.command.expect("expected blast-radius command") {
        Command::BlastRadius(args) => {
            assert!(
                args.report_format.is_none(),
                "tool report format should stay unset"
            );
        }
        _ => panic!("expected blast-radius command"),
    }

    assert_eq!(cli.tool_flags.effective_format(), OutputFormat::Markdown);
}

#[test]
fn test_cli_blast_radius_report_format_flag_is_separate() {
    use crate::cli::{Cli, Command};

    let cli = Cli::try_parse_from(["julie-server", "blast-radius", "--report-format", "compact"])
        .expect("blast-radius should accept a separate report-format flag");

    match cli.command.expect("expected blast-radius command") {
        Command::BlastRadius(args) => {
            assert_eq!(args.report_format.as_deref(), Some("compact"));
        }
        _ => panic!("expected blast-radius command"),
    }

    assert_eq!(cli.tool_flags.effective_format(), OutputFormat::Text);
}

// ---------------------------------------------------------------------------
// workspace
// ---------------------------------------------------------------------------

#[test]
fn test_workspace_cmd_defaults() {
    let args = WorkspaceArgs::parse_from(["workspace", "index"]);
    assert_eq!(args.operation, "index");
    assert!(args.path.is_none());
    assert!(!args.force);
    assert!(args.name.is_none());
}

#[test]
fn test_workspace_cmd_all_flags() {
    let args = WorkspaceArgs::parse_from([
        "workspace",
        "register",
        "--path",
        "/code/myproject",
        "--force",
        "--name",
        "My Project",
    ]);
    assert_eq!(args.operation, "register");
    assert_eq!(args.path.as_deref(), Some("/code/myproject"));
    assert!(args.force);
    assert_eq!(args.name.as_deref(), Some("My Project"));
}

#[test]
fn test_workspace_cmd_short_flags() {
    let args = WorkspaceArgs::parse_from(["workspace", "open", "-p", "/tmp/proj", "-n", "test"]);
    assert_eq!(args.path.as_deref(), Some("/tmp/proj"));
    assert_eq!(args.name.as_deref(), Some("test"));
}

// ---------------------------------------------------------------------------
// tool (generic)
// ---------------------------------------------------------------------------

#[test]
fn test_generic_tool_defaults() {
    let args = GenericToolArgs::parse_from(["tool", "fast_search"]);
    assert_eq!(args.name, "fast_search");
    assert_eq!(args.params, "{}");
}

#[test]
fn test_generic_tool_with_params() {
    let args = GenericToolArgs::parse_from([
        "tool",
        "deep_dive",
        "--params",
        r#"{"symbol":"Command","depth":"full"}"#,
    ]);
    assert_eq!(args.name, "deep_dive");
    assert!(args.params.contains("Command"));
}

#[test]
fn test_generic_tool_short_flag() {
    let args =
        GenericToolArgs::parse_from(["tool", "get_symbols", "-p", r#"{"file_path":"src/cli.rs"}"#]);
    assert_eq!(args.name, "get_symbols");
    assert!(args.params.contains("cli.rs"));
}

// ---------------------------------------------------------------------------
// Global flags
// ---------------------------------------------------------------------------

#[test]
fn test_global_flags_json_shorthand() {
    let flags = GlobalToolFlags {
        json: true,
        format: None,
        standalone: false,
    };
    assert_eq!(flags.effective_format(), OutputFormat::Json);
}

#[test]
fn test_global_flags_format_overrides_json() {
    let flags = GlobalToolFlags {
        json: true,
        format: Some(OutputFormat::Markdown),
        standalone: false,
    };
    // --format takes precedence over --json
    assert_eq!(flags.effective_format(), OutputFormat::Markdown);
}

#[test]
fn test_global_flags_default_text() {
    let flags = GlobalToolFlags {
        json: false,
        format: None,
        standalone: false,
    };
    assert_eq!(flags.effective_format(), OutputFormat::Text);
}

#[test]
fn test_output_format_display() {
    assert_eq!(OutputFormat::Text.to_string(), "text");
    assert_eq!(OutputFormat::Json.to_string(), "json");
    assert_eq!(OutputFormat::Markdown.to_string(), "markdown");
}

#[test]
fn test_signals_defaults() {
    use crate::cli::{Cli, Command};
    let cli = Cli::try_parse_from(["julie-server", "signals"]).unwrap();
    let Command::Signals(args) = cli.command.unwrap() else {
        panic!("expected Signals");
    };
    assert!(!args.fresh);
    assert!(args.file_pattern.is_none());
    assert!(args.limit.is_none());
}

#[test]
fn test_signals_all_flags() {
    use crate::cli::{Cli, Command};
    let cli = Cli::try_parse_from([
        "julie-server",
        "signals",
        "--fresh",
        "--file-pattern",
        "src/api/**",
        "--limit",
        "50",
    ])
    .unwrap();
    let Command::Signals(args) = cli.command.unwrap() else {
        panic!("expected Signals");
    };
    assert!(args.fresh);
    assert_eq!(args.file_pattern.as_deref(), Some("src/api/**"));
    assert_eq!(args.limit, Some(50));
}
