use crate::symbol::Symbol;
use std::path::Path;

#[test]
fn projected_symbol_keeps_exact_unicode_source_and_extraction_facts() {
    let source = "// π\r\npub fn café() -> &'static str { \"雪\" }\r\n";
    let extracted =
        julie_extractors::extract_canonical("sample.rs", source, Path::new(".")).unwrap();
    let fact = extracted
        .symbols
        .into_iter()
        .find(|s| s.name == "café")
        .unwrap();
    let expected = source
        .get(fact.start_byte as usize..fact.end_byte as usize)
        .unwrap()
        .to_owned();
    let original = serde_json::to_value(&fact).unwrap();
    let projected = Symbol::from_extracted(fact, source).unwrap();
    assert_eq!(projected.code_context.as_deref(), Some(expected.as_str()));
    assert_eq!(
        serde_json::to_value(&projected.extracted).unwrap(),
        original
    );
    let serialized = serde_json::to_value(&projected).unwrap();
    assert!(serialized.get("extracted").is_none());
    assert_eq!(serialized["code_context"], expected);
}

#[test]
fn projected_symbol_rejects_out_of_bounds_and_non_utf8_ranges() {
    let source = "pub fn café() {}";
    let extracted =
        julie_extractors::extract_canonical("sample.rs", source, Path::new(".")).unwrap();
    let mut fact = extracted
        .symbols
        .into_iter()
        .find(|s| s.name == "café")
        .unwrap();
    fact.end_byte = u32::MAX;
    assert!(Symbol::from_extracted(fact.clone(), source).is_err());
    fact.start_byte = (source.find('é').unwrap() + 1) as u32;
    fact.end_byte = source.len() as u32;
    assert!(Symbol::from_extracted(fact, source).is_err());
}

#[test]
fn challenge_symbol_projection_unicode_crlf_emoji_complex_boundaries() {
    let source =
        "\u{feff}// Prefix 🦀\r\npub fn test_café_雪() -> &'static str {\r\n    \"👨‍👩‍👧‍👦 🚀\"\r\n}\r\n";
    let extracted =
        julie_extractors::extract_canonical("sample.rs", source, Path::new(".")).unwrap();
    let fact = extracted
        .symbols
        .into_iter()
        .find(|s| s.name == "test_café_雪")
        .unwrap();

    let start = fact.start_byte as usize;
    let end = fact.end_byte as usize;
    let expected = source.get(start..end).unwrap().to_owned();
    assert!(expected.contains("\r\n"));
    assert!(expected.contains("café"));
    assert!(expected.contains("雪"));
    assert!(expected.contains("👨‍👩‍👧‍👦"));
    assert!(expected.contains("🚀"));

    let projected = Symbol::from_extracted(fact.clone(), source).unwrap();
    assert_eq!(projected.code_context.as_deref(), Some(expected.as_str()));
    assert_eq!(projected.name, "test_café_雪");

    let serialized = serde_json::to_value(&projected).unwrap();
    assert!(serialized.get("extracted").is_none());
    assert_eq!(serialized["name"], "test_café_雪");
    assert_eq!(serialized["code_context"], expected);
}

#[test]
fn challenge_symbol_projection_inverted_and_zero_length_spans() {
    let source = "pub fn café() {}";
    let extracted =
        julie_extractors::extract_canonical("sample.rs", source, Path::new(".")).unwrap();
    let base_fact = extracted
        .symbols
        .into_iter()
        .find(|s| s.name == "café")
        .unwrap();

    let mut inverted_fact = base_fact.clone();
    inverted_fact.start_byte = 10;
    inverted_fact.end_byte = 5;
    assert!(Symbol::from_extracted(inverted_fact, source).is_err());

    let mut zero_len_fact = base_fact.clone();
    zero_len_fact.start_byte = 5;
    zero_len_fact.end_byte = 5;
    let projected_zero = Symbol::from_extracted(zero_len_fact, source).unwrap();
    assert_eq!(projected_zero.code_context, None);

    let mut eof_zero_fact = base_fact.clone();
    eof_zero_fact.start_byte = source.len() as u32;
    eof_zero_fact.end_byte = source.len() as u32;
    let projected_eof = Symbol::from_extracted(eof_zero_fact, source).unwrap();
    assert_eq!(projected_eof.code_context, None);

    let mut past_eof_fact = base_fact.clone();
    past_eof_fact.start_byte = (source.len() + 1) as u32;
    past_eof_fact.end_byte = (source.len() + 1) as u32;
    assert!(Symbol::from_extracted(past_eof_fact, source).is_err());
}

#[test]
fn challenge_symbol_projection_multi_byte_non_utf8_split_positions() {
    let source = "// 🦀\r\nfn café_雪_🚀() {}\r\n";
    let extracted =
        julie_extractors::extract_canonical("sample.rs", source, Path::new(".")).unwrap();
    let base_fact = extracted.symbols.into_iter().next().unwrap();

    let multi_byte_chars = ['🦀', 'é', '雪', '🚀'];
    for ch in multi_byte_chars {
        let ch_str = ch.to_string();
        let ch_start = source.find(ch).unwrap();
        let ch_len = ch_str.len();
        assert!(ch_len > 1);

        for offset in 1..ch_len {
            let interior_idx = (ch_start + offset) as u32;

            let mut bad_start = base_fact.clone();
            bad_start.start_byte = interior_idx;
            bad_start.end_byte = source.len() as u32;
            assert!(
                Symbol::from_extracted(bad_start, source).is_err(),
                "expected Err for start_byte {} inside char {}",
                interior_idx,
                ch
            );

            let mut bad_end = base_fact.clone();
            bad_end.start_byte = 0;
            bad_end.end_byte = interior_idx;
            assert!(
                Symbol::from_extracted(bad_end, source).is_err(),
                "expected Err for end_byte {} inside char {}",
                interior_idx,
                ch
            );
        }
    }
}

#[test]
fn challenge_symbol_projection_exhaustive_span_oracle_stress() {
    let source = "// π\r\npub fn café() -> &'static str { \"雪🦀\" }\r\n";
    let extracted =
        julie_extractors::extract_canonical("sample.rs", source, Path::new(".")).unwrap();
    let base_fact = extracted.symbols.into_iter().next().unwrap();

    let max_bound = source.len() + 3;
    for start in 0..=max_bound {
        for end in 0..=max_bound {
            let mut fact = base_fact.clone();
            fact.start_byte = start as u32;
            fact.end_byte = end as u32;

            let result = Symbol::from_extracted(fact, source);
            let valid_range = start <= end
                && end <= source.len()
                && source.is_char_boundary(start)
                && source.is_char_boundary(end);

            if valid_range {
                let symbol = result.expect("valid range must produce Symbol");
                if start == end {
                    assert_eq!(symbol.code_context, None);
                } else {
                    assert_eq!(symbol.code_context.as_deref(), Some(&source[start..end]));
                }
            } else {
                assert!(
                    result.is_err(),
                    "invalid range {}..{} must return Err",
                    start,
                    end
                );
            }
        }
    }
}

#[test]
fn challenge_symbol_projection_json_serialization_roundtrip() {
    let source = "pub fn café() -> &'static str { \"雪\" }";
    let extracted =
        julie_extractors::extract_canonical("sample.rs", source, Path::new(".")).unwrap();
    let fact = extracted
        .symbols
        .into_iter()
        .find(|s| s.name == "café")
        .unwrap();
    let projected = Symbol::from_extracted(fact, source).unwrap();

    let json_str = serde_json::to_string(&projected).unwrap();
    let json_val: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    assert!(json_val.get("extracted").is_none());
    assert!(json_val.get("name").is_some());
    assert_eq!(json_val["name"], "café");
    assert!(json_val.get("code_context").is_some());
    assert_eq!(json_val["code_context"], source);

    let deserialized: Symbol = serde_json::from_str(&json_str).unwrap();
    assert_eq!(deserialized, projected);
    assert_eq!(deserialized.name, "café");
    assert_eq!(deserialized.code_context.as_deref(), Some(source));

    let mut json_val_null = json_val.clone();
    json_val_null["code_context"] = serde_json::Value::Null;
    let des_null: Symbol = serde_json::from_value(json_val_null).unwrap();
    assert_eq!(des_null.code_context, None);

    let mut json_val_missing = json_val.clone();
    json_val_missing
        .as_object_mut()
        .unwrap()
        .remove("code_context");
    let des_missing: Symbol = serde_json::from_value(json_val_missing).unwrap();
    assert_eq!(des_missing.code_context, None);

    // Adversarially test all optional and complex fields populated
    let mut metadata = std::collections::HashMap::new();
    metadata.insert("custom_key".to_string(), serde_json::json!({"nested": 42}));

    let full_fact = julie_extractors::Symbol {
        id: "full_sym_1".to_string(),
        name: "full_func".to_string(),
        kind: julie_extractors::SymbolKind::Method,
        language: "rust".to_string(),
        file_path: "src/full.rs".to_string(),
        start_line: 10,
        start_column: 4,
        end_line: 20,
        end_column: 5,
        start_byte: 100,
        end_byte: 250,
        body_span: Some(julie_extractors::NormalizedSpan {
            start_line: 11,
            start_column: 8,
            end_line: 19,
            end_column: 4,
            start_byte: 120,
            end_byte: 240,
        }),
        body_hash: Some("sha256:abc123def456".to_string()),
        signature: Some("pub fn full_func(&self) -> bool".to_string()),
        doc_comment: Some("/// Full doc comment\n/// second line".to_string()),
        visibility: Some(julie_extractors::Visibility::Public),
        parent_id: Some("class_parent_0".to_string()),
        metadata: Some(metadata),
        annotations: vec![julie_extractors::AnnotationMarker {
            annotation: "@test_annotation".to_string(),
            annotation_key: "test_annotation".to_string(),
            raw_text: Some("@test_annotation(param = 1)".to_string()),
            carrier: Some("attribute".to_string()),
        }],
        semantic_group: Some("group_full".to_string()),
        confidence: Some(0.95),
        content_type: Some("code".to_string()),
    };

    let full_projected = Symbol {
        extracted: full_fact,
        code_context: Some("pub fn full_func(&self) -> bool { true }".to_string()),
    };

    let full_json = serde_json::to_string(&full_projected).unwrap();
    let full_val: serde_json::Value = serde_json::from_str(&full_json).unwrap();
    assert!(full_val.get("extracted").is_none());
    assert_eq!(full_val["id"], "full_sym_1");
    assert_eq!(full_val["name"], "full_func");
    assert_eq!(full_val["body_hash"], "sha256:abc123def456");
    assert_eq!(
        full_val["code_context"],
        "pub fn full_func(&self) -> bool { true }"
    );
    assert_eq!(full_val["metadata"]["custom_key"]["nested"], 42);

    let full_deserialized: Symbol = serde_json::from_str(&full_json).unwrap();
    assert_eq!(full_deserialized, full_projected);
    assert_eq!(full_deserialized.id, "full_sym_1");
    assert_eq!(full_deserialized.name, "full_func");
    assert_eq!(
        full_deserialized.body_hash.as_deref(),
        Some("sha256:abc123def456")
    );
    assert_eq!(full_deserialized.body_span, full_projected.body_span);
    assert_eq!(full_deserialized.metadata, full_projected.metadata);
    assert_eq!(full_deserialized.annotations, full_projected.annotations);
    assert_eq!(full_deserialized.code_context, full_projected.code_context);
}
