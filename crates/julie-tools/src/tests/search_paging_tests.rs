use crate::search::FastSearchParams;
use crate::shared::next_line;

#[test]
fn fast_search_defaults_to_compact_format_and_zero_offset() {
    let p: FastSearchParams = serde_json::from_value(serde_json::json!({ "query": "x" })).unwrap();
    assert_eq!(p.search.return_format, "compact");
    assert_eq!(p.search.offset, 0);
}

#[test]
fn fast_search_rejects_the_removed_locations_format() {
    let p: FastSearchParams =
        serde_json::from_value(serde_json::json!({ "query": "x", "return_format": "locations" }))
            .unwrap();
    let err = p.search.validated_format().unwrap_err();
    assert!(err.to_string().contains("compact"), "{err}");
}

#[test]
fn next_line_names_the_tool_and_the_next_offset() {
    assert_eq!(
        next_line("fast_search", &serde_json::json!({"query": "a b"}), 16),
        "next: fast_search {\"offset\":16,\"query\":\"a b\"}"
    );
    assert_eq!(
        next_line("fast_refs", &serde_json::json!({"symbol": "Foo"}), 10),
        "next: fast_refs {\"offset\":10,\"symbol\":\"Foo\"}"
    );
}
