use julie_core::database::SymbolDatabase;
use julie_extractors::SymbolKind;
use julie_test_support::db::{file_info_builder, set_symbol_reference_scores, symbol_builder};

use std::collections::HashMap;
use tempfile::TempDir;

#[path = "../../../tools/metrics/query.rs"]
mod metrics_query;

use metrics_query::{format_metrics_output, query_by_metrics};

fn test_db_with_metric_symbols() -> (TempDir, SymbolDatabase) {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("metrics.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    for (path, hash) in [("src/high.rs", "h1"), ("src/low.rs", "h2")] {
        db.store_file_info(
            &file_info_builder(path)
                .language("rust")
                .hash(hash)
                .size(100)
                .last_modified(0)
                .symbol_count(0)
                .line_count(0)
                .build(),
        )
        .unwrap();
    }

    (tmp, db)
}

fn metadata_object(value: serde_json::Value) -> HashMap<String, serde_json::Value> {
    match value {
        serde_json::Value::Object(map) => map.into_iter().collect(),
        other => panic!("expected object metadata, got {other:?}"),
    }
}

fn insert_metric_symbol(
    db: &mut SymbolDatabase,
    id: &str,
    name: &str,
    file_path: &str,
    reference_score: f64,
    metadata: HashMap<String, serde_json::Value>,
) {
    db.store_symbols(&[symbol_builder(id, name, file_path)
        .kind(SymbolKind::Function)
        .language("rust")
        .span(10, 1, 12, 1)
        .bytes(0, 20)
        .metadata(metadata)
        .confidence(1.0)
        .build()])
        .unwrap();
    set_symbol_reference_scores(db, &[(id, reference_score)]).unwrap();
}

#[test]
fn query_metrics_uses_centrality_for_legacy_or_unknown_sorts() {
    let (_tmp, mut db) = test_db_with_metric_symbols();
    let legacy_key = ["security", "risk"].join("_");
    let mut legacy_meta = serde_json::Map::new();
    legacy_meta.insert(
        legacy_key.clone(),
        serde_json::json!({
            "label": "HIGH",
            "score": 0.99
        }),
    );

    insert_metric_symbol(
        &mut db,
        "high",
        "high_centrality",
        "src/high.rs",
        42.0,
        metadata_object(serde_json::json!({
            "change_risk": {"label": "LOW", "score": 0.2},
            "test_linkage": {"best_tier": "direct", "test_count": 2}
        })),
    );
    insert_metric_symbol(
        &mut db,
        "legacy",
        "legacy_label",
        "src/low.rs",
        1.0,
        legacy_meta.into_iter().collect(),
    );

    for sort_by in [&legacy_key, "unknown_metric"] {
        let results = query_by_metrics(
            &db,
            sort_by,
            "desc",
            None,
            None,
            Some("function"),
            None,
            None,
            false,
            10,
        )
        .unwrap();
        let output = format_metrics_output(&results, sort_by, "desc");

        assert!(
            !output.contains("Security:"),
            "metrics output should not expose legacy metadata rows:\n{output}"
        );
        assert_eq!(
            results
                .iter()
                .map(|result| result.name.as_str())
                .collect::<Vec<_>>(),
            vec!["high_centrality", "legacy_label"],
            "{sort_by} should use centrality ordering"
        );
    }
}
