use anyhow::{Result, bail};
use std::path::Path;
use std::process::Command;

use crate::tests::helpers::mcp::call_tool_result_text;
use crate::tests::helpers::snapshot::snapshot_context_from_files;
use crate::tools::impact::BlastRadiusTool;

fn git_in(root: &Path, args: &[&str]) -> Result<()> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

fn init_clean_git_repo(root: &Path) -> Result<()> {
    git_in(root, &["init", "-b", "main"])?;
    git_in(root, &["add", "-A"])?;
    git_in(
        root,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "init",
        ],
    )?;
    Ok(())
}

fn readable(symbol: &str, max_depth: u32, limit: u32) -> BlastRadiusTool {
    BlastRadiusTool {
        symbol_ids: vec![symbol.to_string()],
        max_depth,
        limit,
        format: Some("readable".to_string()),
        ..Default::default()
    }
}

#[tokio::test]
async fn test_blast_radius_ranks_direct_callers_and_truncates() -> Result<()> {
    let (_tree, context) = snapshot_context_from_files(&[
        ("src/worker.rs", "pub fn run_pipeline() {}\n"),
        (
            "src/api.rs",
            "pub fn handle_request() { run_pipeline(); }\n",
        ),
        ("src/app.rs", "pub fn app_entry() { handle_request(); }\n"),
        (
            "tests/request_tests.rs",
            "#[test]\nfn test_request_flow() { handle_request(); }\n",
        ),
    ])?;

    let result = readable("run_pipeline", 2, 1).call_tool(&context).await?;

    let text = call_tool_result_text(&result);
    assert!(
        text.contains("handle_request"),
        "first page should show direct caller first: {text}"
    );
    assert!(
        text.contains("tests/request_tests.rs"),
        "linked tests should be listed: {text}"
    );
    assert!(
        text.trim_end()
            .ends_with("next: blast_radius symbol_ids=run_pipeline offset=1"),
        "overflowing impacts must end with the next line: {text}"
    );
    Ok(())
}

#[tokio::test]
async fn test_blast_radius_likely_tests_include_resolved_refs_to_impacted_symbols() -> Result<()> {
    let (_tree, context) = snapshot_context_from_files(&[
        ("src/service.rs", "pub fn run_service() {}\n"),
        ("src/api.rs", "pub fn handle_request() { run_service(); }\n"),
        (
            "src/helper.rs",
            "pub fn build_helper() { run_service(); }\n",
        ),
        (
            "tests/request_flow.rs",
            "#[test]\nfn test_request_flow() { handle_request(); }\n",
        ),
    ])?;

    let result = readable("run_service", 1, 5).call_tool(&context).await?;

    let text = call_tool_result_text(&result);
    assert!(
        text.contains("handle_request"),
        "impacted symbol should appear in blast radius: {text}"
    );
    assert!(
        text.contains("tests/request_flow.rs"),
        "resolved identifier refs to impacted symbols should produce likely tests: {text}"
    );
    assert!(
        text.contains("test_request_flow"),
        "related test symbol should be surfaced with the likely path: {text}"
    );
    Ok(())
}

fn generated_tests(count: usize) -> Vec<(String, String)> {
    (0..count)
        .map(|index| {
            (
                format!("tests/generated/test_{index:02}.rs"),
                format!("#[test]\nfn test_generated_case_{index:02}() {{ run_pipeline(); }}\n"),
            )
        })
        .collect()
}

async fn overflow_text() -> Result<String> {
    let generated = generated_tests(12);
    let mut files: Vec<(&str, &str)> = vec![("src/worker.rs", "pub fn run_pipeline() {}\n")];
    files.extend(
        generated
            .iter()
            .map(|(path, content)| (path.as_str(), content.as_str())),
    );
    let (_tree, context) = snapshot_context_from_files(&files)?;
    let result = readable("run_pipeline", 1, 5).call_tool(&context).await?;
    Ok(call_tool_result_text(&result))
}

#[tokio::test]
async fn test_blast_radius_likely_test_path_overflow_is_counted() -> Result<()> {
    let text = overflow_text().await?;

    assert!(
        text.contains("tests/generated/test_09.rs"),
        "visible likely tests should include the capped prefix: {text}"
    );
    assert!(
        !text.contains("tests/generated/test_10.rs"),
        "overflow likely tests should stay out of the first page: {text}"
    );
    assert!(
        text.contains("…and 2 more"),
        "hidden likely-test paths must be counted: {text}"
    );
    Ok(())
}

#[tokio::test]
async fn test_blast_radius_related_test_symbol_overflow_is_counted() -> Result<()> {
    let text = overflow_text().await?;

    assert!(
        text.contains("test_generated_case_09"),
        "visible related test symbols should include the capped prefix: {text}"
    );
    assert!(
        !text.contains("test_generated_case_10"),
        "overflow related test symbols should stay out of the first page: {text}"
    );
    assert!(
        text.contains("…and 2 more"),
        "hidden related test symbols must be counted: {text}"
    );
    Ok(())
}

#[tokio::test]
async fn test_blast_radius_rejects_unknown_seed_and_empty_request() -> Result<()> {
    let (tree, context) =
        snapshot_context_from_files(&[("src/worker.rs", "pub fn run_pipeline() {}\n")])?;

    let unknown = readable("no_such_symbol", 1, 5)
        .call_tool(&context)
        .await
        .unwrap_err()
        .to_string();
    assert_eq!(
        unknown,
        "Unknown symbol ids for blast_radius: no_such_symbol"
    );

    let nongit = BlastRadiusTool::default()
        .call_tool(&context)
        .await
        .unwrap_err()
        .to_string();
    assert!(
        nongit.contains("git") && nongit.contains("failed"),
        "empty request in a non-git workspace should return the git error, got: {nongit}"
    );

    init_clean_git_repo(tree.path())?;
    let empty = BlastRadiusTool::default().call_tool(&context).await?;
    let text = call_tool_result_text(&empty);
    assert_eq!(text.trim(), "No changed files in the working tree.");
    Ok(())
}
