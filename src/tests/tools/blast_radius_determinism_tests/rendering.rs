use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn test_blast_radius_surfaces_identifier_only_callers() -> Result<()> {
    let (_temp_dir, handler, workspace_id) = setup_handler().await?;
    seed_identifier_walk_fixture(&handler, &workspace_id).await?;

    let result = BlastRadiusTool {
        symbol_ids: vec!["seed".to_string()],
        file_paths: vec![],
        from_revision: None,
        to_revision: None,
        max_depth: 2,
        limit: 10,
        include_tests: true,
        // Explicit readable so we can assert on section headings.
        format: Some("readable".to_string()),
        workspace: Some("primary".to_string()),
        ..Default::default()
    }
    .call_tool(&handler)
    .await?;

    let text = extract_text(&result);

    assert!(
        !text.contains("No impacted symbols found"),
        "identifier-based callers must be reported: {text}"
    );
    for caller in ["setupHandler", "buildPipeline", "configureServer"] {
        assert!(
            text.contains(caller),
            "expected identifier-derived caller `{caller}` in output: {text}"
        );
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_blast_radius_is_deterministic_across_repeated_calls() -> Result<()> {
    let (_temp_dir, handler, workspace_id) = setup_handler().await?;
    seed_identifier_walk_fixture(&handler, &workspace_id).await?;

    let tool = BlastRadiusTool {
        symbol_ids: vec!["seed".to_string()],
        file_paths: vec![],
        from_revision: None,
        to_revision: None,
        max_depth: 2,
        limit: 10,
        include_tests: true,
        format: Some("readable".to_string()),
        workspace: Some("primary".to_string()),
        ..Default::default()
    };

    let first = extract_text(&tool.call_tool(&handler).await?);
    let second = extract_text(&tool.call_tool(&handler).await?);

    assert_eq!(
        first, second,
        "two identical back-to-back calls must produce byte-identical output"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_blast_radius_renders_paths_and_related_symbol_headings() -> Result<()> {
    let (_temp_dir, handler, workspace_id) = setup_handler().await?;
    seed_identifier_walk_fixture(&handler, &workspace_id).await?;

    let result = BlastRadiusTool {
        symbol_ids: vec!["seed".to_string()],
        file_paths: vec![],
        from_revision: None,
        to_revision: None,
        max_depth: 2,
        limit: 10,
        include_tests: true,
        format: Some("readable".to_string()),
        workspace: Some("primary".to_string()),
        ..Default::default()
    }
    .call_tool(&handler)
    .await?;

    let text = extract_text(&result);

    assert!(
        text.contains("Likely tests"),
        "expected Likely tests heading: {text}"
    );
    assert!(
        text.contains("tests/store_tests.ts"),
        "expected test path under Likely tests: {text}"
    );
    assert!(
        text.contains("Related test symbols"),
        "expected Related test symbols heading: {text}"
    );
    assert!(
        text.contains("testStoreSnapshot"),
        "expected linked test name under Related test symbols: {text}"
    );

    // Bare names must not leak into the "Likely tests" section. Parse the
    // block between the two headings to check it contains only path-like
    // entries (has a '/' or ends with an extension).
    let likely_start = text.find("Likely tests\n").expect("Likely tests heading");
    let slice_after_heading = &text[likely_start + "Likely tests\n".len()..];
    let likely_block_end = slice_after_heading
        .find("\n\n")
        .unwrap_or(slice_after_heading.len());
    let likely_block = &slice_after_heading[..likely_block_end];
    for line in likely_block.lines() {
        let entry = line.trim_start_matches("- ").trim();
        if entry.is_empty() {
            continue;
        }
        assert!(
            entry.contains('/') || entry.contains('.'),
            "Likely tests block must contain only paths, saw `{entry}`: {text}"
        );
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_blast_radius_defaults_to_compact_format() -> Result<()> {
    let (_temp_dir, handler, workspace_id) = setup_handler().await?;
    seed_identifier_walk_fixture(&handler, &workspace_id).await?;

    let readable = extract_text(
        &BlastRadiusTool {
            symbol_ids: vec!["seed".to_string()],
            file_paths: vec![],
            from_revision: None,
            to_revision: None,
            max_depth: 2,
            limit: 10,
            include_tests: true,
            format: Some("readable".to_string()),
            workspace: Some("primary".to_string()),
            ..Default::default()
        }
        .call_tool(&handler)
        .await?,
    );

    let defaulted = extract_text(
        &BlastRadiusTool {
            symbol_ids: vec!["seed".to_string()],
            file_paths: vec![],
            from_revision: None,
            to_revision: None,
            max_depth: 2,
            limit: 10,
            include_tests: true,
            format: None,
            workspace: Some("primary".to_string()),
            ..Default::default()
        }
        .call_tool(&handler)
        .await?,
    );

    let compact = extract_text(
        &BlastRadiusTool {
            symbol_ids: vec!["seed".to_string()],
            file_paths: vec![],
            from_revision: None,
            to_revision: None,
            max_depth: 2,
            limit: 10,
            include_tests: true,
            format: Some("compact".to_string()),
            workspace: Some("primary".to_string()),
            ..Default::default()
        }
        .call_tool(&handler)
        .await?,
    );

    assert_eq!(
        defaulted, compact,
        "format=None must match compact, not readable — saw `{defaulted}` vs compact `{compact}`"
    );
    assert_ne!(
        defaulted, readable,
        "compact default should differ from readable (blank-line separators)"
    );

    Ok(())
}
