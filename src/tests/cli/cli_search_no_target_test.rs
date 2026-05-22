//! `--target` regression tests for the `search` subcommand.
//!
//! After T8 unified-search cutover the flag became a no-op; the eros bakeoff
//! comparator and other external harnesses still pass `--target definitions`
//! et al., so we keep the flag parseable but ignore its value (every query
//! routes through the unified path now). These tests pin both shapes — with
//! and without `--target` — to make sure that contract does not regress.

use clap::Parser;

use crate::cli::Cli;

#[test]
fn no_target_flag() {
    let result = Cli::try_parse_from(["julie-server", "search", "main", "--workspace", "/tmp"]);
    assert!(
        result.is_ok(),
        "search without --target should parse cleanly, got error: {}",
        result.err().map(|e| e.to_string()).unwrap_or_default()
    );
}

#[test]
fn target_flag_accepted_as_no_op() {
    for target_value in ["definitions", "files", "content"] {
        let result =
            Cli::try_parse_from(["julie-server", "search", "main", "--target", target_value]);
        assert!(
            result.is_ok(),
            "search --target {target_value} should parse cleanly (deprecated no-op), got error: {}",
            result.err().map(|e| e.to_string()).unwrap_or_default()
        );
    }
}
