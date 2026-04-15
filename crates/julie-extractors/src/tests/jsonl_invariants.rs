use crate::extract_canonical;

#[test]
fn test_jsonl_repeated_keys_keep_unique_ids_and_file_global_lines() {
    let workspace_root = std::path::PathBuf::from("/test/workspace");
    let jsonl = "{\"type\":\"feature\"}\n{\"type\":\"bug\"}";

    let results = extract_canonical("memories.jsonl", jsonl, &workspace_root)
        .expect("jsonl extraction should succeed");

    let type_symbols: Vec<_> = results
        .symbols
        .iter()
        .filter(|symbol| symbol.name == "type")
        .collect();
    assert_eq!(type_symbols.len(), 2);
    assert_eq!(type_symbols[0].start_line, 1);
    assert_eq!(type_symbols[1].start_line, 2);
    assert_ne!(type_symbols[0].id, type_symbols[1].id);
}

#[test]
fn test_jsonl_empty_lines_preserve_file_global_positions() {
    let workspace_root = std::path::PathBuf::from("/test/workspace");
    let jsonl = "{\"name\":\"first\"}\n\n{\"name\":\"second\"}\n\n{\"name\":\"third\"}";

    let results = extract_canonical("events.jsonl", jsonl, &workspace_root)
        .expect("jsonl extraction should succeed");

    let name_lines: Vec<_> = results
        .symbols
        .iter()
        .filter(|symbol| symbol.name == "name")
        .map(|symbol| symbol.start_line)
        .collect();
    assert_eq!(name_lines, vec![1, 3, 5]);
}
