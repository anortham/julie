use crate::manager::ExtractorManager;
use md5;
use std::collections::HashSet;
use std::path::PathBuf;

fn expected_id(file_path: &str, name: &str, start_line: u32, start_column: u32) -> String {
    let input = format!("{file_path}:{name}:{start_line}:{start_column}");
    format!("{:x}", md5::compute(input.as_bytes()))
}

#[test]
fn test_extract_all_jsonl_emits_file_global_positions_and_unique_ids() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let file_path = "fixtures/events.jsonl";
    let content = concat!(
        r#"{"type":"feature","message":"one"}"#,
        "\n",
        r#"{"type":"bug","message":"two"}"#,
    );

    let results = ExtractorManager::new()
        .extract_all(file_path, content, &workspace_root)
        .expect("jsonl extraction should succeed through the canonical path");

    let type_symbols: Vec<_> = results
        .symbols
        .iter()
        .filter(|symbol| symbol.name == "type")
        .collect();

    assert_eq!(type_symbols.len(), 2, "expected one type key per record");
    assert_eq!(type_symbols[0].start_line, 1);
    assert_eq!(type_symbols[1].start_line, 2);

    let expected_offsets: Vec<u32> = content
        .match_indices("\"type\"")
        .map(|(offset, _)| offset as u32)
        .collect();
    let actual_offsets: Vec<u32> = type_symbols
        .iter()
        .map(|symbol| symbol.start_byte)
        .collect();
    assert_eq!(
        actual_offsets, expected_offsets,
        "JSONL symbols should use file-global byte offsets"
    );

    let ids: HashSet<_> = type_symbols
        .iter()
        .map(|symbol| symbol.id.as_str())
        .collect();
    assert_eq!(
        ids.len(),
        2,
        "duplicate keys on different lines need unique IDs"
    );

    for symbol in type_symbols {
        assert_eq!(
            symbol.id,
            expected_id(
                symbol.file_path.as_str(),
                symbol.name.as_str(),
                symbol.start_line,
                symbol.start_column,
            ),
            "JSONL IDs should hash the normalized stored location"
        );
    }
}
