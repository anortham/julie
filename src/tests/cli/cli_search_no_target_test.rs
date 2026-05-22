//! T8 CLI cutover tests: `--target` flag removed from the `search` subcommand.
//!
//! Tests verify clap behaviour directly (no subprocess) so they run in the
//! unit tier and are deterministic.

use clap::Parser;

use crate::cli::Cli;

/// `julie-server search "main" --workspace /tmp` should parse successfully
/// without a `--target` flag.
#[test]
fn no_target_flag() {
    let result = Cli::try_parse_from([
        "julie-server",
        "search",
        "main",
        "--workspace",
        "/tmp",
    ]);
    assert!(
        result.is_ok(),
        "search without --target should parse cleanly, got error: {}",
        result
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default()
    );
}

/// `julie-server search "main" --target definitions` should be rejected by
/// clap because `--target` no longer exists on the search subcommand.
#[test]
fn target_flag_rejected() {
    let result = Cli::try_parse_from([
        "julie-server",
        "search",
        "main",
        "--target",
        "definitions",
    ]);
    assert!(
        result.is_err(),
        "search --target should be rejected after T8 cutover, but parsing succeeded"
    );
    let err_kind = result.err().map(|e| e.kind());
    assert_eq!(
        err_kind,
        Some(clap::error::ErrorKind::UnknownArgument),
        "expected UnknownArgument error, got: {:?}",
        err_kind
    );
}
