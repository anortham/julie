use std::time::Duration;

use anyhow::Result;

use crate::handler::JulieServerHandler;
use crate::mcp_compat::CallToolResult;
use crate::tools::spillover::SpilloverGetTool;
use crate::tools::spillover::store::{SpilloverFormat, SpilloverStore};

fn extract_text(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|item| {
            serde_json::to_value(item).ok().and_then(|json| {
                json.get("text")
                    .and_then(|value| value.as_str())
                    .map(|text| text.to_string())
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn test_spillover_store_pages_rows_and_issues_next_handle() {
    let store = SpilloverStore::new(32, Duration::from_secs(60));
    let handle = store
        .store_rows(
            "session-a",
            "br",
            "Impact overflow",
            vec![
                "row 1".to_string(),
                "row 2".to_string(),
                "row 3".to_string(),
            ],
            1,
            1,
            SpilloverFormat::Readable,
        )
        .expect("overflow handle");

    let first_page = store.page("session-a", &handle, None, None).unwrap();
    assert_eq!(first_page.rows, vec!["row 2".to_string()]);
    assert!(
        first_page.next_handle.is_some(),
        "expected follow-up handle"
    );

    let second_page = store
        .page(
            "session-a",
            first_page.next_handle.as_deref().unwrap(),
            None,
            None,
        )
        .unwrap();
    assert_eq!(second_page.rows, vec!["row 3".to_string()]);
    assert!(
        second_page.next_handle.is_none(),
        "last page should not emit another handle"
    );
}

#[test]
fn test_spillover_store_replays_identical_handles_for_same_payload_and_page() {
    let store = SpilloverStore::new(32, Duration::from_secs(60));

    let first_handle = store
        .store_rows(
            "session-a",
            "br",
            "Impact overflow",
            vec![
                "row 1".to_string(),
                "row 2".to_string(),
                "row 3".to_string(),
            ],
            1,
            1,
            SpilloverFormat::Readable,
        )
        .expect("overflow handle");
    let second_handle = store
        .store_rows(
            "session-a",
            "br",
            "Impact overflow",
            vec![
                "row 1".to_string(),
                "row 2".to_string(),
                "row 3".to_string(),
            ],
            1,
            1,
            SpilloverFormat::Readable,
        )
        .expect("overflow handle");

    assert_eq!(
        first_handle, second_handle,
        "identical overflow payloads should reuse the same root handle"
    );

    let first_page = store.page("session-a", &first_handle, None, None).unwrap();
    let replayed_page = store.page("session-a", &first_handle, None, None).unwrap();

    assert_eq!(
        first_page, replayed_page,
        "repeated paging calls should be replayable instead of minting fresh handles"
    );
}

#[test]
fn test_spillover_store_rejects_foreign_session_handles() {
    let store = SpilloverStore::new(32, Duration::from_secs(60));
    let handle = store
        .store_rows(
            "session-a",
            "br",
            "Impact overflow",
            vec!["row 1".to_string(), "row 2".to_string()],
            0,
            1,
            SpilloverFormat::Readable,
        )
        .expect("overflow handle");

    let err = store
        .page("session-b", &handle, None, None)
        .expect_err("foreign session must fail");
    assert!(
        err.to_string().contains("does not belong to this session"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_spillover_store_expires_handles() {
    let store = SpilloverStore::new(32, Duration::from_millis(1));
    let handle = store
        .store_rows(
            "session-a",
            "br",
            "Impact overflow",
            vec!["row 1".to_string(), "row 2".to_string()],
            0,
            1,
            SpilloverFormat::Readable,
        )
        .expect("overflow handle");

    std::thread::sleep(Duration::from_millis(10));

    let err = store
        .page("session-a", &handle, None, None)
        .expect_err("expired handle must fail");
    assert!(
        err.to_string().contains("expired"),
        "unexpected error: {err}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_spillover_get_tool_formats_page_and_more_marker() -> Result<()> {
    let handler = JulieServerHandler::new_for_test().await?;
    let handle = handler
        .spillover_store
        .store_rows(
            &handler.session_metrics.session_id,
            "br",
            "Impact overflow",
            vec!["row 1".to_string(), "row 2".to_string()],
            0,
            1,
            SpilloverFormat::Readable,
        )
        .expect("overflow handle");

    let result = SpilloverGetTool {
        spillover_handle: handle,
        limit: Some(1),
        format: Some("readable".to_string()),
    }
    .call_tool(&handler)
    .await?;

    let text = extract_text(&result);
    assert!(text.contains("Impact overflow"), "missing title: {text}");
    assert!(text.contains("row 1"), "missing row body: {text}");
    assert!(
        text.contains("More available: spillover_handle="),
        "missing follow-up handle: {text}"
    );

    Ok(())
}
