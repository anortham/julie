use crate::base::relationship_resolution::{StructuredPendingRelationship, UnresolvedTarget};
use crate::base::{
    ExtractionResults, Identifier, ParseDiagnostic, PendingRelationship, Relationship, Symbol,
    TypeInfo,
};
use crate::pipeline::{detect_language_for_path, extract_canonical};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
struct CapabilityMatrix {
    languages: Vec<CapabilityRow>,
}

#[derive(Debug, Deserialize)]
struct CapabilityRow {
    language: String,
    fixtures: Vec<FixtureRow>,
}

#[derive(Debug, Deserialize)]
struct FixtureRow {
    name: String,
    source: String,
    expected: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct NormalizedExtraction {
    symbols: Vec<NormalizedSymbol>,
    relationships: Vec<NormalizedRelationship>,
    pending_relationships: Vec<NormalizedPendingRelationship>,
    structured_pending_relationships: Vec<NormalizedStructuredPendingRelationship>,
    identifiers: Vec<NormalizedIdentifier>,
    types: Vec<NormalizedTypeInfo>,
    #[serde(default)]
    parse_diagnostics: Vec<NormalizedParseDiagnostic>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct NormalizedSymbol {
    key: String,
    name: String,
    kind: String,
    language: String,
    file_path: String,
    start_line: u32,
    start_column: u32,
    end_line: u32,
    end_column: u32,
    start_byte: u32,
    end_byte: u32,
    body_span: Option<NormalizedBodySpan>,
    body_hash: Option<String>,
    signature: Option<String>,
    doc_comment: Option<String>,
    visibility: Option<String>,
    parent_key: Option<String>,
    metadata: Option<Value>,
    annotations: Value,
    semantic_group: Option<String>,
    confidence: Option<String>,
    content_type: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct NormalizedBodySpan {
    start_line: u32,
    start_column: u32,
    end_line: u32,
    end_column: u32,
    start_byte: u32,
    end_byte: u32,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct NormalizedRelationship {
    from_key: String,
    to_key: String,
    kind: String,
    file_path: String,
    line_number: u32,
    confidence: String,
    metadata: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct NormalizedPendingRelationship {
    from_key: String,
    callee_name: String,
    kind: String,
    file_path: String,
    line_number: u32,
    confidence: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct NormalizedStructuredPendingRelationship {
    pending: NormalizedPendingRelationship,
    target: NormalizedUnresolvedTarget,
    caller_scope_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct NormalizedUnresolvedTarget {
    display_name: String,
    terminal_name: String,
    receiver: Option<String>,
    namespace_path: Vec<String>,
    import_context: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct NormalizedIdentifier {
    key: String,
    name: String,
    kind: String,
    language: String,
    file_path: String,
    start_line: u32,
    start_column: u32,
    end_line: u32,
    end_column: u32,
    start_byte: u32,
    end_byte: u32,
    containing_key: Option<String>,
    target_key: Option<String>,
    confidence: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct NormalizedTypeInfo {
    symbol_key: String,
    resolved_type: String,
    generic_params: Option<Vec<String>>,
    constraints: Option<Vec<String>>,
    is_inferred: bool,
    language: String,
    metadata: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct NormalizedParseDiagnostic {
    kind: String,
    start_line: u32,
    start_column: u32,
    end_line: u32,
    end_column: u32,
    start_byte: u32,
    end_byte: u32,
}

#[test]
fn golden_fixtures_match_canonical_extraction() {
    let root = workspace_root();
    let matrix = load_matrix(&root);
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    let mut seen = BTreeSet::new();

    for row in matrix.languages {
        assert!(
            !row.fixtures.is_empty(),
            "language {} has no golden fixtures",
            row.language
        );

        for fixture in row.fixtures {
            let case_key = format!("{}:{}", row.language, fixture.name);
            assert!(
                seen.insert(case_key.clone()),
                "duplicate fixture {case_key}"
            );

            let source_path = root.join(&fixture.source);
            let expected_path = root.join(&fixture.expected);
            let source = fs::read_to_string(&source_path).unwrap_or_else(|err| {
                panic!(
                    "failed to read source for {} at {}: {}",
                    case_key,
                    source_path.display(),
                    err
                )
            });
            let source = normalize_fixture_line_endings(source);
            let detected = detect_language_for_path(&fixture.source)
                .unwrap_or_else(|err| panic!("failed to detect language for {case_key}: {err}"));
            assert_eq!(
                detected, row.language,
                "fixture {case_key} must route through its registry language"
            );

            let actual = extract_canonical(&fixture.source, &source, &root)
                .unwrap_or_else(|err| panic!("extract_canonical failed for {case_key}: {err}"));
            let normalized = normalize(actual);
            let actual_json = serde_json::to_string_pretty(&normalized).unwrap();

            if update {
                if let Some(parent) = expected_path.parent() {
                    fs::create_dir_all(parent).unwrap();
                }
                fs::write(&expected_path, format!("{actual_json}\n")).unwrap();
                continue;
            }

            let expected_json = fs::read_to_string(&expected_path).unwrap_or_else(|err| {
                panic!(
                    "missing expected golden output for {} at {}: {}. Run UPDATE_GOLDEN=1 cargo nextest run -p julie-extractors golden",
                    case_key,
                    expected_path.display(),
                    err
                )
            });
            let expected: NormalizedExtraction = serde_json::from_str(&expected_json)
                .unwrap_or_else(|err| {
                    panic!(
                        "invalid expected golden output for {} at {}: {}",
                        case_key,
                        expected_path.display(),
                        err
                    )
                });

            assert_eq!(
                expected,
                normalized,
                "golden mismatch for {case_key}\nexpected file: {}\nactual:\n{}",
                expected_path.display(),
                actual_json
            );
        }
    }
}

fn normalize_fixture_line_endings(source: String) -> String {
    source.replace("\r\n", "\n").replace('\r', "\n")
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("julie-extractors crate should live under crates/")
        .to_path_buf()
}

fn load_matrix(root: &Path) -> CapabilityMatrix {
    let matrix_path = root.join("fixtures/extraction/capabilities.json");
    let json = fs::read_to_string(&matrix_path).unwrap_or_else(|err| {
        panic!(
            "failed to read capability matrix at {}: {}",
            matrix_path.display(),
            err
        )
    });
    serde_json::from_str(&json).unwrap_or_else(|err| {
        panic!(
            "failed to parse capability matrix at {}: {}",
            matrix_path.display(),
            err
        )
    })
}

fn normalize(results: ExtractionResults) -> NormalizedExtraction {
    let symbol_keys = symbol_key_map(&results.symbols);
    let mut symbols: Vec<_> = results
        .symbols
        .iter()
        .map(|symbol| normalize_symbol(symbol, &symbol_keys))
        .collect();
    let mut relationships: Vec<_> = results
        .relationships
        .iter()
        .map(|relationship| normalize_relationship(relationship, &symbol_keys))
        .collect();
    let mut pending_relationships: Vec<_> = results
        .pending_relationships
        .iter()
        .map(|pending| normalize_pending(pending, &symbol_keys))
        .collect();
    let mut structured_pending_relationships: Vec<_> = results
        .structured_pending_relationships
        .iter()
        .map(|pending| normalize_structured_pending(pending, &symbol_keys))
        .collect();
    let mut identifiers: Vec<_> = results
        .identifiers
        .iter()
        .map(|identifier| normalize_identifier(identifier, &symbol_keys))
        .collect();
    let mut types: Vec<_> = results
        .types
        .values()
        .map(|type_info| normalize_type(type_info, &symbol_keys))
        .collect();
    let mut parse_diagnostics: Vec<_> = results
        .parse_diagnostics
        .iter()
        .map(normalize_parse_diagnostic)
        .collect();

    sort_json(&mut symbols);
    sort_json(&mut relationships);
    sort_json(&mut pending_relationships);
    sort_json(&mut structured_pending_relationships);
    sort_json(&mut identifiers);
    sort_json(&mut types);
    sort_json(&mut parse_diagnostics);

    NormalizedExtraction {
        symbols,
        relationships,
        pending_relationships,
        structured_pending_relationships,
        identifiers,
        types,
        parse_diagnostics,
    }
}

fn symbol_key_map(symbols: &[Symbol]) -> HashMap<String, String> {
    symbols
        .iter()
        .map(|symbol| (symbol.id.clone(), symbol_key(symbol)))
        .collect()
}

fn symbol_key(symbol: &Symbol) -> String {
    format!(
        "{}:{}:{}:{}",
        symbol.file_path, symbol.name, symbol.start_line, symbol.start_column
    )
}

fn normalize_symbol(symbol: &Symbol, symbol_keys: &HashMap<String, String>) -> NormalizedSymbol {
    NormalizedSymbol {
        key: symbol_key(symbol),
        name: symbol.name.clone(),
        kind: symbol.kind.to_string(),
        language: symbol.language.clone(),
        file_path: symbol.file_path.clone(),
        start_line: symbol.start_line,
        start_column: symbol.start_column,
        end_line: symbol.end_line,
        end_column: symbol.end_column,
        start_byte: symbol.start_byte,
        end_byte: symbol.end_byte,
        body_span: symbol.body_span.map(normalize_body_span),
        body_hash: symbol.body_hash.clone(),
        signature: symbol.signature.clone(),
        doc_comment: symbol.doc_comment.clone(),
        visibility: symbol.visibility.as_ref().map(ToString::to_string),
        parent_key: symbol
            .parent_id
            .as_ref()
            .map(|id| lookup_symbol_key(id, symbol_keys)),
        metadata: symbol.metadata.as_ref().map(sorted_json_map),
        annotations: serde_json::to_value(&symbol.annotations).unwrap(),
        semantic_group: symbol.semantic_group.clone(),
        confidence: symbol.confidence.map(normalize_confidence),
        content_type: symbol.content_type.clone(),
    }
}

fn normalize_body_span(span: crate::base::BodySpan) -> NormalizedBodySpan {
    NormalizedBodySpan {
        start_line: span.start_line,
        start_column: span.start_column,
        end_line: span.end_line,
        end_column: span.end_column,
        start_byte: span.start_byte,
        end_byte: span.end_byte,
    }
}

fn normalize_relationship(
    relationship: &Relationship,
    symbol_keys: &HashMap<String, String>,
) -> NormalizedRelationship {
    NormalizedRelationship {
        from_key: lookup_symbol_key(&relationship.from_symbol_id, symbol_keys),
        to_key: lookup_symbol_key(&relationship.to_symbol_id, symbol_keys),
        kind: relationship.kind.to_string(),
        file_path: relationship.file_path.clone(),
        line_number: relationship.line_number,
        confidence: normalize_confidence(relationship.confidence),
        metadata: relationship.metadata.as_ref().map(sorted_json_map),
    }
}

fn normalize_pending(
    pending: &PendingRelationship,
    symbol_keys: &HashMap<String, String>,
) -> NormalizedPendingRelationship {
    NormalizedPendingRelationship {
        from_key: lookup_symbol_key(&pending.from_symbol_id, symbol_keys),
        callee_name: pending.callee_name.clone(),
        kind: pending.kind.to_string(),
        file_path: pending.file_path.clone(),
        line_number: pending.line_number,
        confidence: normalize_confidence(pending.confidence),
    }
}

fn normalize_structured_pending(
    pending: &StructuredPendingRelationship,
    symbol_keys: &HashMap<String, String>,
) -> NormalizedStructuredPendingRelationship {
    NormalizedStructuredPendingRelationship {
        pending: normalize_pending(&pending.pending, symbol_keys),
        target: normalize_target(&pending.target),
        caller_scope_key: pending
            .caller_scope_symbol_id
            .as_ref()
            .map(|id| lookup_symbol_key(id, symbol_keys)),
    }
}

fn normalize_target(target: &UnresolvedTarget) -> NormalizedUnresolvedTarget {
    NormalizedUnresolvedTarget {
        display_name: target.display_name.clone(),
        terminal_name: target.terminal_name.clone(),
        receiver: target.receiver.clone(),
        namespace_path: target.namespace_path.clone(),
        import_context: target.import_context.clone(),
    }
}

fn normalize_identifier(
    identifier: &Identifier,
    symbol_keys: &HashMap<String, String>,
) -> NormalizedIdentifier {
    NormalizedIdentifier {
        key: format!(
            "{}:{}:{}:{}:{}",
            identifier.file_path,
            identifier.name,
            identifier.kind,
            identifier.start_line,
            identifier.start_column
        ),
        name: identifier.name.clone(),
        kind: identifier.kind.to_string(),
        language: identifier.language.clone(),
        file_path: identifier.file_path.clone(),
        start_line: identifier.start_line,
        start_column: identifier.start_column,
        end_line: identifier.end_line,
        end_column: identifier.end_column,
        start_byte: identifier.start_byte,
        end_byte: identifier.end_byte,
        containing_key: identifier
            .containing_symbol_id
            .as_ref()
            .map(|id| lookup_symbol_key(id, symbol_keys)),
        target_key: identifier
            .target_symbol_id
            .as_ref()
            .map(|id| lookup_symbol_key(id, symbol_keys)),
        confidence: normalize_confidence(identifier.confidence),
    }
}

fn normalize_type(
    type_info: &TypeInfo,
    symbol_keys: &HashMap<String, String>,
) -> NormalizedTypeInfo {
    NormalizedTypeInfo {
        symbol_key: lookup_symbol_key(&type_info.symbol_id, symbol_keys),
        resolved_type: type_info.resolved_type.clone(),
        generic_params: type_info.generic_params.clone(),
        constraints: type_info.constraints.clone(),
        is_inferred: type_info.is_inferred,
        language: type_info.language.clone(),
        metadata: type_info.metadata.as_ref().map(sorted_json_map),
    }
}

fn normalize_parse_diagnostic(diagnostic: &ParseDiagnostic) -> NormalizedParseDiagnostic {
    NormalizedParseDiagnostic {
        kind: format!("{:?}", diagnostic.kind),
        start_line: diagnostic.start_line,
        start_column: diagnostic.start_column,
        end_line: diagnostic.end_line,
        end_column: diagnostic.end_column,
        start_byte: diagnostic.start_byte,
        end_byte: diagnostic.end_byte,
    }
}

fn lookup_symbol_key(id: &str, symbol_keys: &HashMap<String, String>) -> String {
    symbol_keys
        .get(id)
        .cloned()
        .unwrap_or_else(|| format!("unresolved:{id}"))
}

fn sorted_json_map(map: &HashMap<String, Value>) -> Value {
    let sorted: BTreeMap<_, _> = map
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    serde_json::to_value(sorted).unwrap()
}

fn normalize_confidence(confidence: f32) -> String {
    format!("{confidence:.3}")
}

fn sort_json<T: Serialize>(items: &mut [T]) {
    items.sort_by_key(|item| serde_json::to_string(item).unwrap());
}
