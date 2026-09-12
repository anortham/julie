use std::path::Path;
use std::process::Command;

use anyhow::{Result, bail};
use serde::de::DeserializeOwned;

use crate::tests::helpers::mcp::call_tool_result_text;
use crate::tests::helpers::snapshot::{snapshot_context, snapshot_context_from_files};
use crate::tools::GetSymbolsTool;
use crate::tools::impact::BlastRadiusTool;
use crate::tools::navigation::FastRefsTool;
use crate::tools::search::{FastSearchParams, FastSearchTool};

fn next_args<T: DeserializeOwned>(text: &str, tool: &str) -> T {
    let prefix = format!("next: {tool} ");
    let json = text
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .unwrap_or_else(|| panic!("missing {tool} next request in: {text}"));
    serde_json::from_str(json)
        .unwrap_or_else(|error| panic!("invalid next request: {error}: {json}"))
}

fn git(root: &Path, args: &[&str]) -> Result<()> {
    let output = Command::new("git").args(args).current_dir(root).output()?;
    if output.status.success() {
        return Ok(());
    }
    bail!(
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn impact_names(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let (_, row) = line.trim_start().split_once(". ")?;
            row.split_whitespace().next().map(str::to_string)
        })
        .collect()
}

#[tokio::test]
async fn fast_search_next_replays_all_effective_filters_and_workspace() -> Result<()> {
    let (_tree, context) = snapshot_context_from_files(&[
        (
            "src/page.rs",
            "// paging \"quoted\" \\ slash 雪 snow needle\n// paging \"quoted\" \\ slash 雪 snow needle\n",
        ),
        (
            "src/page_two.rs",
            "// paging \"quoted\" \\ slash 雪 snow needle\n",
        ),
        ("tests/page.rs", "// paging quoted slash snow needle\n"),
    ])?;
    let first = FastSearchParams {
        search: FastSearchTool {
            query: "paging \"quoted\" \\ slash 雪 snow needle".into(),
            language: Some("rust".into()),
            file_pattern: Some("src/**".into()),
            limit: 1,
            context_lines: Some(0),
            exclude_tests: Some(true),
            backend: None,
            workspace: None,
            return_format: "compact".into(),
            offset: 0,
            semantics: Some(julie_core::embeddings_contract::SemanticMode::Off),
        },
        regions: Some("comment".into()),
    };

    let first_text = call_tool_result_text(&first.call_tool(&context).await?);
    let replay: FastSearchParams = next_args(&first_text, "fast_search");

    assert_eq!(replay.search.query, first.search.query);
    assert_eq!(replay.search.language, first.search.language);
    assert_eq!(replay.search.file_pattern, first.search.file_pattern);
    assert_eq!(replay.search.limit, 1);
    assert_eq!(replay.search.context_lines, Some(0));
    assert_eq!(replay.search.exclude_tests, Some(true));
    assert_eq!(replay.search.backend, None);
    assert_eq!(replay.search.workspace.as_deref(), Some("snapshot-fixture"));
    assert_eq!(replay.search.return_format, "compact");
    assert_eq!(replay.search.offset, 1);
    assert_eq!(
        replay.search.semantics,
        Some(julie_core::embeddings_contract::SemanticMode::Off)
    );
    assert_eq!(replay.regions.as_deref(), Some("comment"));
    assert!(!call_tool_result_text(&replay.call_tool(&context).await?).is_empty());

    for format in ["compact", "full"] {
        let mut region_request = first.clone();
        region_request.search.return_format = format.into();
        let region_text = call_tool_result_text(&region_request.call_tool(&context).await?);
        let parsed: FastSearchParams = next_args(&region_text, "fast_search");
        assert_eq!(parsed.regions.as_deref(), Some("comment"));
        assert_eq!(parsed.search.return_format, format);

        let mut ordinary_request = first.clone();
        ordinary_request.search.query = "paging".into();
        ordinary_request.search.return_format = format.into();
        ordinary_request.regions = None;
        let ordinary_text = call_tool_result_text(&ordinary_request.call_tool(&context).await?);
        let parsed: FastSearchParams = next_args(&ordinary_text, "fast_search");
        assert_eq!(parsed.regions, None);
        assert_eq!(parsed.search.backend, None);
        assert_eq!(parsed.search.return_format, format);
    }
    Ok(())
}

#[tokio::test]
async fn fast_refs_next_replays_kind_definition_semantics_and_workspace() -> Result<()> {
    let (_tree, context) = snapshot_context_from_files(&[
        ("src/target.rs", "pub fn page_target() {}\n"),
        (
            "src/calls.rs",
            "pub fn caller_one() { page_target(); }\npub fn caller_two() { page_target(); }\npub fn caller_three() { page_target(); }\n",
        ),
    ])?;
    let first = FastRefsTool {
        symbol: "page_target".into(),
        include_definition: false,
        limit: 0,
        offset: 0,
        workspace: None,
        reference_kind: Some("call".into()),
        semantics: Some(julie_core::embeddings_contract::SemanticMode::Off),
    };

    let first_result = first.call_tool(&context).await?;
    let replay: FastRefsTool = next_args(&call_tool_result_text(&first_result), "fast_refs");

    assert_eq!(replay.symbol, first.symbol);
    assert!(!replay.include_definition);
    assert_eq!(replay.limit, 1);
    assert_eq!(replay.offset, 1);
    assert_eq!(replay.workspace.as_deref(), Some("snapshot-fixture"));
    assert_eq!(replay.reference_kind.as_deref(), Some("call"));
    assert_eq!(replay.semantics, first.semantics);
    assert_eq!(
        first_result.structured_content.as_ref().unwrap()["definitions"],
        serde_json::json!([])
    );
    assert_eq!(
        replay
            .call_tool(&context)
            .await?
            .structured_content
            .as_ref()
            .unwrap()["definitions"],
        serde_json::json!([])
    );
    Ok(())
}

#[tokio::test]
async fn get_symbols_next_replays_target_mode_depth_limit_and_workspace() -> Result<()> {
    let (_tree, context) = snapshot_context_from_files(&[(
        "src/page.rs",
        "pub fn page_target_one() {}\npub fn page_target_two() {}\npub fn unrelated() {}\n",
    )])?;
    let first = GetSymbolsTool {
        file_path: "src/page.rs".into(),
        max_depth: 0,
        target: Some("page_target".into()),
        limit: Some(1),
        offset: 0,
        mode: Some("structure".into()),
        workspace: None,
    };

    let first_text = call_tool_result_text(&first.call_tool(&context).await?);
    let replay: GetSymbolsTool = next_args(&first_text, "get_symbols");

    assert_eq!(replay.file_path, first.file_path);
    assert_eq!(replay.max_depth, 0);
    assert_eq!(replay.target, first.target);
    assert_eq!(replay.limit, Some(1));
    assert_eq!(replay.offset, 1);
    assert_eq!(replay.mode.as_deref(), Some("structure"));
    assert_eq!(replay.workspace.as_deref(), Some("snapshot-fixture"));
    let second_text = call_tool_result_text(&replay.call_tool(&context).await?);
    assert!(second_text.contains("page_target_two"), "{second_text}");
    assert!(!second_text.contains("unrelated"), "{second_text}");
    Ok(())
}

#[tokio::test]
async fn blast_radius_next_replays_resolved_seed_and_workspace() -> Result<()> {
    let tree = tempfile::TempDir::new()?;
    std::fs::create_dir_all(tree.path().join("src"))?;
    std::fs::write(
        tree.path().join("src/seed.rs"),
        "pub fn changed_seed() {}\n",
    )?;
    std::fs::write(
        tree.path().join("src/calls.rs"),
        "pub fn caller_one() { changed_seed(); }\npub fn caller_two() { changed_seed(); }\n",
    )?;
    git(tree.path(), &["init", "--quiet"])?;
    git(
        tree.path(),
        &["config", "user.email", "julie@example.invalid"],
    )?;
    git(tree.path(), &["config", "user.name", "Julie Test"])?;
    git(tree.path(), &["add", "."])?;
    git(tree.path(), &["commit", "--quiet", "-m", "fixture"])?;
    std::fs::write(
        tree.path().join("src/seed.rs"),
        "pub fn changed_seed() { }\n",
    )?;
    let context = snapshot_context(tree.path())?;
    let first = BlastRadiusTool {
        max_depth: 1,
        limit: 1,
        include_tests: false,
        format: Some("compact".into()),
        mode: Some("default".into()),
        git: true,
        ..Default::default()
    };

    let first_text = call_tool_result_text(&first.call_tool(&context).await?);
    let replay: BlastRadiusTool = next_args(&first_text, "blast_radius");

    assert_eq!(replay.file_paths, vec!["src/seed.rs"]);
    assert!(replay.symbol_ids.is_empty());
    assert!(!replay.git);
    assert_eq!(replay.max_depth, 1);
    assert_eq!(replay.limit, 1);
    assert_eq!(replay.offset, 1);
    assert!(!replay.include_tests);
    assert_eq!(replay.format.as_deref(), Some("compact"));
    assert_eq!(replay.mode.as_deref(), Some("default"));
    assert_eq!(replay.workspace.as_deref(), Some("snapshot-fixture"));
    assert!(!call_tool_result_text(&replay.call_tool(&context).await?).is_empty());
    Ok(())
}

#[tokio::test]
async fn paged_request_matches_single_call_slice_on_unchanged_snapshot() -> Result<()> {
    let (_tree, context) = snapshot_context_from_files(&[
        ("src/target.rs", "pub fn page_target() {}\n"),
        (
            "src/calls.rs",
            "pub fn caller_one() { page_target(); }\npub fn caller_two() { page_target(); }\npub fn caller_three() { page_target(); }\n",
        ),
    ])?;
    let page = FastRefsTool {
        symbol: "page_target".into(),
        include_definition: false,
        limit: 1,
        offset: 0,
        workspace: None,
        reference_kind: Some("call".into()),
        semantics: Some(julie_core::embeddings_contract::SemanticMode::Off),
    };
    let first = page.call_tool(&context).await?;
    let second_args: FastRefsTool = next_args(&call_tool_result_text(&first), "fast_refs");
    let second = second_args.call_tool(&context).await?;
    let third_args: FastRefsTool = next_args(&call_tool_result_text(&second), "fast_refs");
    let third = third_args.call_tool(&context).await?;
    let whole = FastRefsTool { limit: 3, ..page }
        .call_tool(&context)
        .await?;
    let ids = |result: &crate::mcp_compat::CallToolResult| {
        result.structured_content.as_ref().unwrap()["references"]
            .as_array()
            .unwrap()
            .iter()
            .map(|reference| reference["id"].as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    };
    let mut paged = ids(&first);
    paged.extend(ids(&second));
    paged.extend(ids(&third));
    assert_eq!(paged, ids(&whole));
    assert!(!call_tool_result_text(&whole).contains("next:"));

    let impact_page = BlastRadiusTool {
        symbol_ids: vec!["page_target".into()],
        max_depth: 1,
        limit: 1,
        include_tests: false,
        format: Some("compact".into()),
        workspace: None,
        ..Default::default()
    };
    let impact_first = call_tool_result_text(&impact_page.call_tool(&context).await?);
    let impact_second_args: BlastRadiusTool = next_args(&impact_first, "blast_radius");
    let impact_second = call_tool_result_text(&impact_second_args.call_tool(&context).await?);
    let impact_third_args: BlastRadiusTool = next_args(&impact_second, "blast_radius");
    let impact_third = call_tool_result_text(&impact_third_args.call_tool(&context).await?);
    let impact_whole = call_tool_result_text(
        &BlastRadiusTool {
            limit: 3,
            ..impact_page
        }
        .call_tool(&context)
        .await?,
    );
    let mut impact_paged = impact_names(&impact_first);
    impact_paged.extend(impact_names(&impact_second));
    impact_paged.extend(impact_names(&impact_third));
    assert_eq!(impact_paged, impact_names(&impact_whole));
    assert!(!impact_whole.contains("next:"));

    let mut wide_files = vec![(
        "src/seed.ts".to_string(),
        "export function wideSeed() {}\n".to_string(),
    )];
    for index in 0..105 {
        wide_files.push((
            format!("src/first_{index:03}.ts"),
            format!(
                "import {{ wideSeed }} from \"./seed\";\nexport function first{index:03}() {{ return wideSeed(); }}\n"
            ),
        ));
        wide_files.push((
            format!("src/second_{index:03}.ts"),
            format!(
                "import {{ first{index:03} }} from \"./first_{index:03}\";\nexport function second{index:03}() {{ return first{index:03}(); }}\n"
            ),
        ));
    }
    let wide_borrowed = wide_files
        .iter()
        .map(|(path, content)| (path.as_str(), content.as_str()))
        .collect::<Vec<_>>();
    let (_wide_tree, wide_context) = snapshot_context_from_files(&wide_borrowed)?;
    let mut wide_page = BlastRadiusTool {
        symbol_ids: vec!["wideSeed".into()],
        max_depth: 2,
        limit: 10,
        include_tests: false,
        format: Some("compact".into()),
        ..Default::default()
    };
    let mut wide_paged_names = Vec::new();
    loop {
        let text = call_tool_result_text(&wide_page.call_tool(&wide_context).await?);
        wide_paged_names.extend(impact_names(&text));
        if !text.contains("next:") {
            break;
        }
        wide_page = next_args(&text, "blast_radius");
    }
    let wide_whole = call_tool_result_text(
        &BlastRadiusTool {
            symbol_ids: vec!["wideSeed".into()],
            max_depth: 2,
            limit: 210,
            include_tests: false,
            format: Some("compact".into()),
            ..Default::default()
        }
        .call_tool(&wide_context)
        .await?,
    );
    assert_eq!(wide_paged_names, impact_names(&wide_whole));

    let (_web_tree, web_context) = snapshot_context_from_files(&[
        (
            "src/Controller.php",
            "<?php\nuse Symfony\\Component\\Routing\\Attribute\\Route;\nclass UserController {\n    #[Route('/api/users/{id}', methods: ['GET'])]\n    public function showUser(int $id) { return $id; }\n}\n",
        ),
        (
            "tests/client.test.ts",
            "export async function fetchOne() { return fetch(\"/api/users/1\"); }\nexport async function fetchTwo() { return fetch(\"/api/users/2\"); }\n",
        ),
    ])?;
    let web_first = call_tool_result_text(
        &BlastRadiusTool {
            symbol_ids: vec!["showUser".into()],
            max_depth: 0,
            limit: 1,
            include_tests: false,
            format: Some("compact".into()),
            mode: Some("web".into()),
            ..Default::default()
        }
        .call_tool(&web_context)
        .await?,
    );
    let web_second_args: BlastRadiusTool = next_args(&web_first, "blast_radius");
    let web_second = call_tool_result_text(&web_second_args.call_tool(&web_context).await?);
    assert!(web_first.contains("fetchOne") || web_first.contains("fetchTwo"));
    assert!(web_second.contains("fetchOne") || web_second.contains("fetchTwo"));
    assert!(!web_second.contains("next:"));
    Ok(())
}
