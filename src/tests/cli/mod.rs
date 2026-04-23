//! End-to-end CLI integration tests.
//!
//! These tests invoke the compiled `julie-server` binary via `std::process::Command`
//! and verify exit codes, stdout structure, and output format behavior.
//!
//! All tests use `--standalone` mode to avoid daemon dependency.
//! Tests are marked `#[ignore]` because they require a pre-built debug binary
//! (`cargo build` before running). Run them with:
//!
//! ```sh
//! cargo build && cargo nextest run --lib tests::cli:: -- --ignored
//! ```

use std::path::PathBuf;
use std::process::Command;

use tempfile::TempDir;

/// Returns the path to the debug binary. Panics if the binary does not exist.
fn julie_binary() -> PathBuf {
    let binary = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/julie-server");
    assert!(
        binary.exists(),
        "Debug binary not found at {:?}. Run `cargo build` first.",
        binary
    );
    binary
}

/// Creates a temporary workspace with a small Rust source file so standalone
/// mode has something to index. Returns the `TempDir` (keeps it alive while
/// the caller holds the handle) and the path to the source file.
fn create_temp_workspace() -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("failed to create temp dir");
    let src = dir.path().join("example.rs");
    std::fs::write(
        &src,
        r#"
/// A greeting function.
pub fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}

struct Config {
    verbose: bool,
    retries: u32,
}
"#,
    )
    .expect("failed to write fixture file");
    (dir, src)
}

// ---------------------------------------------------------------------------
// --help tests
// ---------------------------------------------------------------------------

#[test]
#[ignore] // requires pre-built binary
fn test_help_shows_lifecycle_and_tool_commands() {
    let output = Command::new(julie_binary())
        .arg("--help")
        .output()
        .expect("failed to run julie-server --help");

    assert!(output.status.success(), "exit code should be 0");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Lifecycle commands
    assert!(stdout.contains("daemon"), "help should list 'daemon'");
    assert!(stdout.contains("stop"), "help should list 'stop'");
    assert!(stdout.contains("status"), "help should list 'status'");
    assert!(stdout.contains("restart"), "help should list 'restart'");

    // Tool commands (named wrappers)
    assert!(stdout.contains("search"), "help should list 'search'");
    assert!(stdout.contains("refs"), "help should list 'refs'");
    assert!(stdout.contains("symbols"), "help should list 'symbols'");
    assert!(stdout.contains("context"), "help should list 'context'");
    assert!(
        stdout.contains("blast-radius"),
        "help should list 'blast-radius'"
    );
    assert!(stdout.contains("workspace"), "help should list 'workspace'");

    // Generic tool path
    assert!(stdout.contains("tool"), "help should list 'tool'");
}

#[test]
#[ignore]
fn test_version_flag() {
    let output = Command::new(julie_binary())
        .arg("--version")
        .output()
        .expect("failed to run julie-server --version");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("julie-server"),
        "version output should contain binary name"
    );
    // Version string should match Cargo.toml
    assert!(
        stdout.contains(env!("CARGO_PKG_VERSION")),
        "version should match CARGO_PKG_VERSION ({})",
        env!("CARGO_PKG_VERSION")
    );
}

// ---------------------------------------------------------------------------
// Named wrapper: search (text output)
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn test_search_named_wrapper_text_output() {
    let (workspace, _src) = create_temp_workspace();
    let output = Command::new(julie_binary())
        .args([
            "search",
            "greet",
            "--workspace",
            workspace.path().to_str().unwrap(),
            "--standalone",
        ])
        .output()
        .expect("failed to run search command");

    // Should succeed regardless of whether the query found results; the binary
    // should not crash and should exit 0.
    assert!(
        output.status.success(),
        "search should exit 0. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Text output should NOT be wrapped in JSON braces
    assert!(
        !stdout.starts_with('{'),
        "text output should not start with '{{' (that would be JSON)"
    );
    // Should have some output (either results or the zero-hit diagnostic)
    assert!(!stdout.is_empty(), "stdout should not be empty");
}

// ---------------------------------------------------------------------------
// Named wrapper: search (JSON output)
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn test_search_named_wrapper_json_output() {
    let (workspace, _src) = create_temp_workspace();
    let output = Command::new(julie_binary())
        .args([
            "search",
            "greet",
            "--workspace",
            workspace.path().to_str().unwrap(),
            "--standalone",
            "--json",
        ])
        .output()
        .expect("failed to run search --json");

    assert!(
        output.status.success(),
        "search --json should exit 0. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON");

    // Verify CallToolResult structure
    assert!(
        json.get("content").is_some(),
        "JSON should have 'content' field"
    );
    let content = json["content"].as_array().expect("content should be array");
    assert!(!content.is_empty(), "content array should not be empty");

    // Each content item should have "type" and "text"
    for item in content {
        assert!(
            item.get("type").is_some(),
            "content item should have 'type' field"
        );
        assert!(
            item.get("text").is_some(),
            "content item should have 'text' field"
        );
        assert_eq!(
            item["type"].as_str().unwrap(),
            "text",
            "content type should be 'text'"
        );
    }

    // isError field
    assert!(
        json.get("isError").is_some(),
        "JSON should have 'isError' field"
    );
    assert_eq!(
        json["isError"].as_bool().unwrap(),
        false,
        "isError should be false for a successful search"
    );
}

// ---------------------------------------------------------------------------
// Named wrapper: search (markdown output)
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn test_search_named_wrapper_markdown_output() {
    let (workspace, _src) = create_temp_workspace();
    let output = Command::new(julie_binary())
        .args([
            "search",
            "greet",
            "--workspace",
            workspace.path().to_str().unwrap(),
            "--standalone",
            "--format",
            "markdown",
        ])
        .output()
        .expect("failed to run search --format markdown");

    assert!(
        output.status.success(),
        "search --format markdown should exit 0. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Markdown output should start with a heading
    assert!(
        stdout.starts_with("# "),
        "markdown output should start with '# ' heading, got: {:?}",
        &stdout[..stdout.len().min(40)]
    );
}

// ---------------------------------------------------------------------------
// Generic tool path
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn test_generic_tool_path_json_output() {
    let (workspace, _src) = create_temp_workspace();
    let output = Command::new(julie_binary())
        .args([
            "tool",
            "fast_search",
            "--params",
            r#"{"query":"greet"}"#,
            "--workspace",
            workspace.path().to_str().unwrap(),
            "--standalone",
            "--json",
        ])
        .output()
        .expect("failed to run generic tool command");

    assert!(
        output.status.success(),
        "generic tool should exit 0. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("generic tool JSON output should parse");

    // Same CallToolResult structure as named wrappers
    assert!(json.get("content").is_some(), "should have 'content'");
    assert!(json.get("isError").is_some(), "should have 'isError'");
}

#[test]
#[ignore]
fn test_generic_tool_path_text_output() {
    let (workspace, _src) = create_temp_workspace();
    let output = Command::new(julie_binary())
        .args([
            "tool",
            "fast_search",
            "--params",
            r#"{"query":"greet"}"#,
            "--workspace",
            workspace.path().to_str().unwrap(),
            "--standalone",
        ])
        .output()
        .expect("failed to run generic tool (text)");

    assert!(
        output.status.success(),
        "generic tool text should exit 0. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Text mode: no JSON wrapping
    assert!(
        !stdout.starts_with('{'),
        "text output from generic tool should not be JSON"
    );
    assert!(!stdout.is_empty(), "text output should not be empty");
}

// ---------------------------------------------------------------------------
// Exit code tests
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn test_nonexistent_workspace_exits_nonzero() {
    let output = Command::new(julie_binary())
        .args([
            "search",
            "hello",
            "--workspace",
            "/nonexistent/path/that/does/not/exist",
            "--standalone",
        ])
        .output()
        .expect("failed to run with bad workspace");

    assert!(
        !output.status.success(),
        "should exit non-zero for nonexistent workspace"
    );
}

#[test]
#[ignore]
fn test_invalid_subcommand_exits_nonzero() {
    let output = Command::new(julie_binary())
        .arg("not-a-real-command")
        .output()
        .expect("failed to run with invalid subcommand");

    assert!(
        !output.status.success(),
        "invalid subcommand should exit non-zero"
    );
}

// ---------------------------------------------------------------------------
// Format flag interactions
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn test_json_shorthand_flag_equivalent_to_format_json() {
    let (workspace, _src) = create_temp_workspace();

    let output_shorthand = Command::new(julie_binary())
        .args([
            "search",
            "greet",
            "--workspace",
            workspace.path().to_str().unwrap(),
            "--standalone",
            "--json",
        ])
        .output()
        .expect("failed with --json");

    let output_explicit = Command::new(julie_binary())
        .args([
            "search",
            "greet",
            "--workspace",
            workspace.path().to_str().unwrap(),
            "--standalone",
            "--format",
            "json",
        ])
        .output()
        .expect("failed with --format json");

    let stdout_short = String::from_utf8_lossy(&output_shorthand.stdout);
    let stdout_explicit = String::from_utf8_lossy(&output_explicit.stdout);

    // Both should be valid JSON
    let json_short: serde_json::Value =
        serde_json::from_str(&stdout_short).expect("--json output should be valid JSON");
    let json_explicit: serde_json::Value =
        serde_json::from_str(&stdout_explicit).expect("--format json output should be valid JSON");

    // Both should have the same structure (content may differ due to timing,
    // but both should be CallToolResult shaped)
    assert!(json_short.get("content").is_some());
    assert!(json_explicit.get("content").is_some());
    assert!(json_short.get("isError").is_some());
    assert!(json_explicit.get("isError").is_some());
}

// ---------------------------------------------------------------------------
// Symbols named wrapper (exercises a different tool)
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn test_symbols_named_wrapper_json() {
    let (workspace, src) = create_temp_workspace();
    let output = Command::new(julie_binary())
        .args([
            "symbols",
            src.to_str().unwrap(),
            "--workspace",
            workspace.path().to_str().unwrap(),
            "--standalone",
            "--json",
        ])
        .output()
        .expect("failed to run symbols command");

    assert!(
        output.status.success(),
        "symbols should exit 0. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("symbols JSON should be valid");

    assert!(json.get("content").is_some(), "should have 'content'");
    assert!(json.get("isError").is_some(), "should have 'isError'");
}

// ---------------------------------------------------------------------------
// Diagnostic stderr (standalone mode logs to stderr)
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn test_standalone_stderr_does_not_leak_into_stdout() {
    let (workspace, _src) = create_temp_workspace();
    let output = Command::new(julie_binary())
        .args([
            "search",
            "greet",
            "--workspace",
            workspace.path().to_str().unwrap(),
            "--standalone",
            "--json",
        ])
        .output()
        .expect("failed to run search");

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    // JSON output must be parseable; if stderr leaked in, this would fail.
    let _json: serde_json::Value = serde_json::from_str(&stdout)
        .expect("stdout must be clean JSON with no stderr contamination");
}

// ---------------------------------------------------------------------------
// Generic tool path with get_symbols (different tool than fast_search)
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn test_generic_tool_path_get_symbols() {
    let (workspace, src) = create_temp_workspace();
    let params = serde_json::json!({
        "file_path": src.to_str().unwrap()
    });
    let output = Command::new(julie_binary())
        .args([
            "tool",
            "get_symbols",
            "--params",
            &params.to_string(),
            "--workspace",
            workspace.path().to_str().unwrap(),
            "--standalone",
            "--json",
        ])
        .output()
        .expect("failed to run generic tool get_symbols");

    assert!(
        output.status.success(),
        "generic tool get_symbols should exit 0. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("get_symbols JSON should be valid");
    assert!(json.get("content").is_some());
    assert!(json.get("isError").is_some());
}
