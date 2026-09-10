use super::*;

#[tokio::test]
async fn test_blast_radius_surfaces_identifier_only_callers() -> Result<()> {
    let (_tree, context) = identifier_walk_fixture()?;

    let result = seed_tool(Some("readable")).call_tool(&context).await?;

    let text = call_tool_result_text(&result);
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

#[tokio::test]
async fn test_blast_radius_is_deterministic_across_repeated_calls() -> Result<()> {
    let (_tree, context) = identifier_walk_fixture()?;
    let tool = seed_tool(Some("readable"));

    let first = call_tool_result_text(&tool.call_tool(&context).await?);
    let second = call_tool_result_text(&tool.call_tool(&context).await?);

    assert_eq!(
        first, second,
        "two identical back-to-back calls must produce byte-identical output"
    );
    Ok(())
}

#[tokio::test]
async fn test_blast_radius_renders_paths_and_related_symbol_headings() -> Result<()> {
    let (_tree, context) = identifier_walk_fixture()?;

    let result = seed_tool(Some("readable")).call_tool(&context).await?;

    let text = call_tool_result_text(&result);
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

#[tokio::test]
async fn test_blast_radius_defaults_to_compact_format() -> Result<()> {
    let (_tree, context) = identifier_walk_fixture()?;

    let readable = call_tool_result_text(&seed_tool(Some("readable")).call_tool(&context).await?);
    let defaulted = call_tool_result_text(&seed_tool(None).call_tool(&context).await?);
    let compact = call_tool_result_text(&seed_tool(Some("compact")).call_tool(&context).await?);

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
