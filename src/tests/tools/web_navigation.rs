use anyhow::Result;
use tempfile::TempDir;

use crate::tests::helpers::mcp::call_tool_result_text;
use crate::tests::helpers::snapshot::snapshot_context;
use crate::tools::{BlastRadiusTool, CallPathTool};

fn write_tree(files: &[(&str, &str)]) -> Result<TempDir> {
    let tree = TempDir::new()?;
    for (path, content) in files {
        let full = tree.path().join(path);
        std::fs::create_dir_all(full.parent().unwrap())?;
        std::fs::write(full, content)?;
    }
    Ok(tree)
}

const CLIENT_TS: &str = "export async function fetchUser() {\n  const r = await fetch(\"/api/users/123\");\n  return r.json();\n}\nexport async function fetchUnknown() {\n  const r = await fetch(\"/api/unknown\", { method: \"POST\" });\n  return r.json();\n}\n";
const CONTROLLER_PHP: &str = "<?php\nuse Symfony\\Component\\Routing\\Attribute\\Route;\nclass UserController {\n    #[Route('/api/users/{id}', methods: ['GET'])]\n    public function showUser(int $id) { return $id; }\n}\n";

fn seeded_context() -> Result<(TempDir, julie_test_support::FakeToolContext)> {
    let tree = write_tree(&[
        ("src/client.ts", CLIENT_TS),
        ("src/Controller.php", CONTROLLER_PHP),
    ])?;
    let context = snapshot_context(tree.path())?;
    Ok((tree, context))
}

fn seeded_sql_context() -> Result<(TempDir, julie_test_support::FakeToolContext)> {
    let tree = write_tree(&[
        (
            "schema/tables.sql",
            "CREATE TABLE users (id INT, name TEXT);\n",
        ),
        (
            "schema/routines.sql",
            "CREATE PROCEDURE touch_users() BEGIN UPDATE users SET name = 'x' WHERE id = 1; END;\n",
        ),
    ])?;
    let context = snapshot_context(tree.path())?;
    Ok((tree, context))
}

#[tokio::test]
async fn trace_web_mode_follows_http_call_edge_to_handler() -> Result<()> {
    let (_temp, context) = seeded_context()?;

    let result = CallPathTool {
        from: "fetchUser".into(),
        to: "showUser".into(),
        mode: Some("web".into()),
        ..Default::default()
    }
    .call_tool(&context)
    .await?;
    let text = call_tool_result_text(&result);

    assert!(
        text.contains("found=true"),
        "expected found=true, got: {text}"
    );
    assert!(
        text.contains("--http_call-->"),
        "expected an http_call hop, got: {text}"
    );
    assert!(
        text.contains("showUser"),
        "expected target showUser, got: {text}"
    );
    Ok(())
}

#[tokio::test]
async fn trace_web_mode_reports_external_endpoint_for_unmatched_call() -> Result<()> {
    let (_temp, context) = seeded_context()?;

    let result = CallPathTool {
        from: "fetchUnknown".into(),
        to: "showUser".into(),
        mode: Some("web".into()),
        ..Default::default()
    }
    .call_tool(&context)
    .await?;
    let text = call_tool_result_text(&result);

    assert!(
        text.contains("found=false"),
        "expected found=false, got: {text}"
    );
    assert!(
        text.contains("external_endpoint: POST /api/unknown"),
        "expected external endpoint label, got: {text}"
    );
    Ok(())
}

#[tokio::test]
async fn trace_default_mode_is_byte_identical_no_web_markers() -> Result<()> {
    let (_temp, context) = seeded_context()?;

    let result = CallPathTool {
        from: "fetchUser".into(),
        to: "showUser".into(),
        ..Default::default()
    }
    .call_tool(&context)
    .await?;
    let text = call_tool_result_text(&result);

    assert!(
        text.contains("found=false"),
        "expected found=false, got: {text}"
    );
    assert!(
        !text.contains("http_call"),
        "default mode must not emit http_call: {text}"
    );
    assert!(
        !text.contains("external_endpoint"),
        "default mode must not emit external_endpoint: {text}"
    );
    Ok(())
}

#[tokio::test]
async fn impact_web_mode_lists_calling_frontend_symbols() -> Result<()> {
    let tree = write_tree(&[
        ("src/client.ts", CLIENT_TS),
        ("src/Controller.php", CONTROLLER_PHP),
        (
            "tests/client.test.ts",
            "import { fetchUser } from \"../src/client\";\nexport function fetchUserReturnsProfile() { return fetchUser(); }\n",
        ),
    ])?;
    let context = snapshot_context(tree.path())?;

    let result = BlastRadiusTool {
        symbol_ids: vec!["showUser".into()],
        mode: Some("web".into()),
        ..Default::default()
    }
    .call_tool(&context)
    .await?;
    let text = call_tool_result_text(&result);

    assert!(
        text.contains("Web callers"),
        "expected Web callers section: {text}"
    );
    assert!(
        text.contains("fetchUser"),
        "expected caller fetchUser: {text}"
    );
    assert!(
        text.contains("http_call"),
        "expected http_call label: {text}"
    );
    let ranked = text.split("Web callers").next().unwrap();
    assert!(
        ranked.contains("fetchUser"),
        "expected fetchUser in ranked impacts: {text}"
    );
    assert!(
        !ranked.contains("showUser"),
        "seed showUser must not be ranked as its own web impact: {text}"
    );
    assert!(
        text.contains("tests/client.test.ts"),
        "expected caller-linked likely test: {text}"
    );

    let depth_zero_result = BlastRadiusTool {
        symbol_ids: vec!["showUser".into()],
        mode: Some("web".into()),
        max_depth: 0,
        ..Default::default()
    }
    .call_tool(&context)
    .await?;
    let depth_zero_text = call_tool_result_text(&depth_zero_result);
    assert!(depth_zero_text.contains("Web callers"));
    assert!(
        !depth_zero_text
            .split("Web callers")
            .next()
            .unwrap()
            .contains("fetchUser"),
        "max_depth=0 must not rank web callers: {depth_zero_text}"
    );
    Ok(())
}

#[tokio::test]
async fn impact_default_mode_omits_web_callers_section() -> Result<()> {
    let (_temp, context) = seeded_context()?;

    let result = BlastRadiusTool {
        symbol_ids: vec!["showUser".into()],
        ..Default::default()
    }
    .call_tool(&context)
    .await?;
    let text = call_tool_result_text(&result);

    assert!(
        !text.contains("Web callers"),
        "default mode must not emit Web callers: {text}"
    );
    Ok(())
}

#[tokio::test]
async fn trace_web_mode_follows_sql_query_edge_to_table() -> Result<()> {
    let (_temp, context) = seeded_sql_context()?;

    let result = CallPathTool {
        from: "touch_users".into(),
        to: "users".into(),
        mode: Some("web".into()),
        ..Default::default()
    }
    .call_tool(&context)
    .await?;
    let text = call_tool_result_text(&result);

    assert!(
        text.contains("found=true"),
        "expected found=true, got: {text}"
    );
    assert!(
        text.contains("--sql_query-->"),
        "expected an sql_query hop, got: {text}"
    );
    assert!(text.contains("users"), "expected target users, got: {text}");
    Ok(())
}

#[tokio::test]
async fn impact_web_mode_lists_routines_querying_table() -> Result<()> {
    let (_temp, context) = seeded_sql_context()?;

    let result = BlastRadiusTool {
        symbol_ids: vec!["users".into()],
        mode: Some("web".into()),
        ..Default::default()
    }
    .call_tool(&context)
    .await?;
    let text = call_tool_result_text(&result);

    assert!(
        text.contains("Web callers"),
        "expected Web callers section: {text}"
    );
    assert!(
        text.contains("touch_users"),
        "expected caller touch_users: {text}"
    );
    assert!(
        text.contains("sql_query"),
        "expected sql_query label: {text}"
    );
    assert!(
        text.contains("table:users"),
        "expected table:users endpoint: {text}"
    );
    Ok(())
}

#[tokio::test]
async fn trace_default_mode_ignores_sql_query_edge() -> Result<()> {
    let (_temp, context) = seeded_sql_context()?;

    let result = CallPathTool {
        from: "touch_users".into(),
        to: "users".into(),
        ..Default::default()
    }
    .call_tool(&context)
    .await?;
    let text = call_tool_result_text(&result);

    assert!(
        text.contains("found=false"),
        "expected found=false, got: {text}"
    );
    assert!(
        !text.contains("sql_query"),
        "default mode must not emit sql_query: {text}"
    );
    Ok(())
}

#[tokio::test]
async fn impact_default_mode_ignores_sql_query_edge() -> Result<()> {
    let (_temp, context) = seeded_sql_context()?;

    let result = BlastRadiusTool {
        symbol_ids: vec!["users".into()],
        ..Default::default()
    }
    .call_tool(&context)
    .await?;
    let text = call_tool_result_text(&result);

    assert!(
        !text.contains("Web callers"),
        "default mode must not emit Web callers: {text}"
    );
    assert!(
        !text.contains("sql_query"),
        "default mode must not emit sql_query: {text}"
    );
    assert!(
        !text.contains("table:users"),
        "default mode must not emit table: endpoint marker: {text}"
    );
    Ok(())
}
