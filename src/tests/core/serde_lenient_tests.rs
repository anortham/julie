//! Tests for lenient MCP parameter deserializers.
//!
//! These lock in the contract that MCP clients can send Vec<String> params
//! either as a raw JSON array or as a stringified JSON array. Without both
//! shapes, blast_radius and get_context reject valid tool calls from stricter
//! clients.

use serde::Deserialize;

use crate::utils::serde_lenient::{
    deserialize_option_i64_lenient, deserialize_option_vec_string_lenient,
    deserialize_vec_string_lenient,
};

#[derive(Debug, Deserialize)]
struct Strict {
    #[serde(default, deserialize_with = "deserialize_vec_string_lenient")]
    names: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Optional {
    #[serde(default, deserialize_with = "deserialize_option_vec_string_lenient")]
    names: Option<Vec<String>>,
}

#[test]
fn vec_string_accepts_raw_array() {
    let parsed: Strict = serde_json::from_str(r#"{"names": ["a", "b"]}"#).unwrap();
    assert_eq!(parsed.names, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn vec_string_accepts_stringified_array() {
    let parsed: Strict = serde_json::from_str(r#"{"names": "[\"a\", \"b\"]"}"#).unwrap();
    assert_eq!(parsed.names, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn vec_string_accepts_empty_string_as_empty() {
    let parsed: Strict = serde_json::from_str(r#"{"names": ""}"#).unwrap();
    assert!(parsed.names.is_empty());
}

#[test]
fn vec_string_default_when_missing() {
    let parsed: Strict = serde_json::from_str(r#"{}"#).unwrap();
    assert!(parsed.names.is_empty());
}

#[test]
fn vec_string_rejects_non_string_items_inside_stringified_array() {
    let result: Result<Strict, _> = serde_json::from_str(r#"{"names": "[1, 2]"}"#);
    assert!(
        result.is_err(),
        "numeric items in stringified array should error"
    );
}

#[test]
fn vec_string_rejects_bare_scalar() {
    let result: Result<Strict, _> = serde_json::from_str(r#"{"names": 42}"#);
    assert!(result.is_err(), "bare scalars are not a valid Vec<String>");
}

#[test]
fn option_vec_string_accepts_null() {
    let parsed: Optional = serde_json::from_str(r#"{"names": null}"#).unwrap();
    assert!(parsed.names.is_none());
}

#[test]
fn option_vec_string_accepts_raw_array() {
    let parsed: Optional = serde_json::from_str(r#"{"names": ["x"]}"#).unwrap();
    assert_eq!(parsed.names, Some(vec!["x".to_string()]));
}

#[test]
fn option_vec_string_accepts_stringified_array() {
    let parsed: Optional = serde_json::from_str(r#"{"names": "[\"x\", \"y\"]"}"#).unwrap();
    assert_eq!(parsed.names, Some(vec!["x".to_string(), "y".to_string()]));
}

#[test]
fn option_vec_string_empty_string_is_none() {
    // Some clients send "" to mean "no value"; treat as None rather than
    // erroring, so tools degrade gracefully.
    let parsed: Optional = serde_json::from_str(r#"{"names": ""}"#).unwrap();
    assert!(parsed.names.is_none());
}

#[test]
fn option_vec_string_missing_field_is_none() {
    let parsed: Optional = serde_json::from_str(r#"{}"#).unwrap();
    assert!(parsed.names.is_none());
}

#[derive(Debug, Deserialize)]
struct OptI64 {
    #[serde(default, deserialize_with = "deserialize_option_i64_lenient")]
    rev: Option<i64>,
}

#[test]
fn option_i64_accepts_raw_number() {
    let parsed: OptI64 = serde_json::from_str(r#"{"rev": 42}"#).unwrap();
    assert_eq!(parsed.rev, Some(42));
}

#[test]
fn option_i64_accepts_stringified_number() {
    let parsed: OptI64 = serde_json::from_str(r#"{"rev": "42"}"#).unwrap();
    assert_eq!(parsed.rev, Some(42));
}

#[test]
fn option_i64_accepts_negative_stringified_number() {
    let parsed: OptI64 = serde_json::from_str(r#"{"rev": "-1"}"#).unwrap();
    assert_eq!(parsed.rev, Some(-1));
}

#[test]
fn option_i64_accepts_null_and_empty_string_and_missing() {
    let null: OptI64 = serde_json::from_str(r#"{"rev": null}"#).unwrap();
    assert!(null.rev.is_none());
    let empty: OptI64 = serde_json::from_str(r#"{"rev": ""}"#).unwrap();
    assert!(empty.rev.is_none());
    let missing: OptI64 = serde_json::from_str(r#"{}"#).unwrap();
    assert!(missing.rev.is_none());
}

#[test]
fn option_i64_rejects_invalid_strings() {
    let result: Result<OptI64, _> = serde_json::from_str(r#"{"rev": "abc"}"#);
    assert!(result.is_err());
}

#[test]
fn blast_radius_tool_accepts_stringified_revisions() {
    use crate::tools::impact::BlastRadiusTool;

    let payload = r#"{
        "file_paths": ["src/foo.rs"],
        "from_revision": "7",
        "to_revision": "12"
    }"#;
    let tool: BlastRadiusTool = serde_json::from_str(payload).unwrap();
    assert_eq!(tool.from_revision, Some(7));
    assert_eq!(tool.to_revision, Some(12));
}

#[test]
fn blast_radius_tool_accepts_stringified_arrays() {
    use crate::tools::impact::BlastRadiusTool;

    let payload = r#"{
        "symbol_ids": "[\"alpha\", \"beta\"]",
        "file_paths": "[\"src/foo.rs\"]"
    }"#;
    let tool: BlastRadiusTool = serde_json::from_str(payload).unwrap();
    assert_eq!(
        tool.symbol_ids,
        vec!["alpha".to_string(), "beta".to_string()]
    );
    assert_eq!(tool.file_paths, vec!["src/foo.rs".to_string()]);
}

#[test]
fn blast_radius_tool_accepts_raw_arrays() {
    use crate::tools::impact::BlastRadiusTool;

    let payload = r#"{
        "symbol_ids": ["alpha"],
        "file_paths": []
    }"#;
    let tool: BlastRadiusTool = serde_json::from_str(payload).unwrap();
    assert_eq!(tool.symbol_ids, vec!["alpha".to_string()]);
    assert!(tool.file_paths.is_empty());
}

#[test]
fn blast_radius_tool_accepts_stringified_include_tests() {
    use crate::tools::impact::BlastRadiusTool;

    let payload = r#"{
        "symbol_ids": ["alpha"],
        "include_tests": "true"
    }"#;
    let tool: BlastRadiusTool = serde_json::from_str(payload).unwrap();
    assert!(tool.include_tests);
}

#[test]
fn blast_radius_tool_schema_describes_inputs() {
    use crate::tools::impact::BlastRadiusTool;

    let schema = serde_json::to_value(schemars::schema_for!(BlastRadiusTool)).unwrap();
    let properties = schema
        .get("properties")
        .and_then(serde_json::Value::as_object)
        .expect("BlastRadiusTool schema should expose input properties");

    let expected_fragments: [(&str, &[&str]); 9] = [
        ("symbol_ids", &["symbol ids"]),
        ("file_paths", &["changed files"]),
        ("from_revision", &["julie database revision"]),
        ("to_revision", &["julie database revision"]),
        ("max_depth", &["relationship hops"]),
        ("limit", &["visible impact rows"]),
        ("include_tests", &["likely tests"]),
        ("format", &["compact", "readable"]),
        ("workspace", &["workspace target"]),
    ];

    for (field, fragments) in expected_fragments {
        let description = properties
            .get(field)
            .and_then(|property| property.get("description"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let description_lower = description.to_ascii_lowercase();
        for fragment in fragments {
            assert!(
                description_lower.contains(fragment),
                "schema description for {field} should mention `{fragment}`: {description}"
            );
        }
    }
}

#[test]
fn call_path_tool_schema_describes_inputs() {
    use crate::tools::navigation::call_path::CallPathTool;

    let schema = serde_json::to_value(schemars::schema_for!(CallPathTool)).unwrap();
    let properties = schema
        .get("properties")
        .and_then(serde_json::Value::as_object)
        .expect("CallPathTool schema should expose input properties");

    let expected_fragments: [(&str, &[&str]); 6] = [
        ("from", &["start", "symbol"]),
        ("to", &["target", "symbol"]),
        ("from_file_path", &["file", "hint"]),
        ("to_file_path", &["file", "hint"]),
        ("workspace", &["workspace"]),
        ("max_hops", &["1", "32"]),
    ];

    for (field, fragments) in expected_fragments {
        let description = properties
            .get(field)
            .and_then(|property| property.get("description"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let description_lower = description.to_ascii_lowercase();
        for fragment in fragments {
            assert!(
                description_lower.contains(fragment),
                "schema description for {field} should mention `{fragment}`: {description}"
            );
        }
    }

    let max_hops_schema = properties
        .get("max_hops")
        .expect("max_hops should be in the CallPathTool schema");
    assert_eq!(
        max_hops_schema
            .get("minimum")
            .and_then(|value| value.as_u64()),
        Some(1),
        "schema should expose the lower max_hops bound"
    );
    assert_eq!(
        max_hops_schema
            .get("maximum")
            .and_then(|value| value.as_u64()),
        Some(32),
        "schema should expose the upper max_hops bound"
    );
}

#[test]
fn get_context_tool_accepts_stringified_task_signals() {
    use crate::tools::get_context::GetContextTool;

    let payload = r#"{
        "query": "investigate failure",
        "edited_files": "[\"src/a.rs\", \"src/b.rs\"]",
        "entry_symbols": "[\"foo\"]"
    }"#;
    let tool: GetContextTool = serde_json::from_str(payload).unwrap();
    assert_eq!(
        tool.edited_files,
        Some(vec!["src/a.rs".to_string(), "src/b.rs".to_string()])
    );
    assert_eq!(tool.entry_symbols, Some(vec!["foo".to_string()]));
}

#[test]
fn get_context_tool_accepts_stringified_prefer_tests() {
    use crate::tools::get_context::GetContextTool;

    let payload = r#"{
        "query": "investigate failure",
        "prefer_tests": "false"
    }"#;
    let tool: GetContextTool = serde_json::from_str(payload).unwrap();
    assert_eq!(tool.prefer_tests, Some(false));
}
