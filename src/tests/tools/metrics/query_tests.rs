use crate::database::SymbolDatabase;
use rusqlite::params;
use tempfile::TempDir;

#[path = "../../../tools/metrics/query.rs"]
mod metrics_query;

use metrics_query::{format_metrics_output, query_by_metrics};

fn test_db_with_metric_symbols() -> (TempDir, SymbolDatabase) {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("metrics.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    db.conn
        .execute_batch(
            "
            INSERT INTO files (path, language, hash, size, last_modified)
            VALUES
                ('src/high.rs', 'rust', 'h1', 100, 0),
                ('src/low.rs', 'rust', 'h2', 100, 0);
            ",
        )
        .unwrap();

    (tmp, db)
}

fn insert_metric_symbol(
    db: &SymbolDatabase,
    id: &str,
    name: &str,
    file_path: &str,
    reference_score: f64,
    metadata: &str,
) {
    db.conn
        .execute(
            "
            INSERT INTO symbols (
                id, name, kind, language, file_path, start_line, start_col,
                end_line, end_col, start_byte, end_byte, reference_score, metadata
            )
            VALUES (?1, ?2, 'function', 'rust', ?3, 10, 1, 12, 1, 0, 20, ?4, ?5)
            ",
            params![id, name, file_path, reference_score, metadata],
        )
        .unwrap();
}

#[test]
fn query_metrics_uses_centrality_for_legacy_or_unknown_sorts() {
    let (_tmp, db) = test_db_with_metric_symbols();
    let legacy_key = ["security", "risk"].join("_");
    let mut legacy_meta = serde_json::Map::new();
    legacy_meta.insert(
        legacy_key.clone(),
        serde_json::json!({
            "label": "HIGH",
            "score": 0.99
        }),
    );
    let legacy_metadata = serde_json::Value::Object(legacy_meta).to_string();

    insert_metric_symbol(
        &db,
        "high",
        "high_centrality",
        "src/high.rs",
        42.0,
        r#"{"change_risk":{"label":"LOW","score":0.2},"test_linkage":{"best_tier":"direct","test_count":2}}"#,
    );
    insert_metric_symbol(
        &db,
        "legacy",
        "legacy_label",
        "src/low.rs",
        1.0,
        &legacy_metadata,
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
