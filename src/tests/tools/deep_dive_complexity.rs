use crate::database::SymbolDatabase;
use crate::database::bulk::atomic::{AtomicPersistenceMetadata, CanonicalWriteSet};
use crate::extractors::SymbolKind;
use crate::extractors::base::ComplexityMetric;
use crate::tests::helpers::db::{file_info_builder, symbol_builder};
use crate::tools::deep_dive::deep_dive_query;
use tempfile::TempDir;

fn seeded_db(complexity_metrics: &[ComplexityMetric]) -> (TempDir, SymbolDatabase) {
    let temp = TempDir::new().unwrap();
    let db_path = temp.path().join("deep-dive-complexity.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();
    let file = file_info_builder("src/lib.rs").language("rust").build();
    let symbol = symbol_builder("symbol-process", "process", "src/lib.rs")
        .kind(SymbolKind::Function)
        .span(10, 0, 17, 1)
        .bytes(100, 220)
        .signature("fn process(input: Request, retry: bool)")
        .build();
    let write_set = CanonicalWriteSet {
        files: std::slice::from_ref(&file),
        symbols: std::slice::from_ref(&symbol),
        complexity_metrics,
        ..Default::default()
    };

    db.incremental_update_atomic_with_metadata(
        &[],
        &write_set,
        "deep-dive-complexity-test",
        AtomicPersistenceMetadata::default(),
    )
    .unwrap();

    (temp, db)
}

#[test]
fn deep_dive_prints_stored_complexity_for_selected_symbol() {
    let metric = ComplexityMetric {
        id: "metric-1".into(),
        file_path: "src/lib.rs".into(),
        language: "rust".into(),
        scope: "function".into(),
        symbol_id: Some("symbol-process".into()),
        algorithm_id: "structural-v1".into(),
        covered_lines: 8,
        covered_bytes: 120,
        decision_count: 4,
        loop_count: 2,
        max_nesting_depth: 3,
        parameter_count: Some(2),
        start_line: 10,
        start_column: 0,
        end_line: 17,
        end_column: 1,
        start_byte: 100,
        end_byte: 220,
        metadata: None,
    };
    let (_temp, db) = seeded_db(std::slice::from_ref(&metric));

    for depth in ["overview", "context", "full"] {
        let output = deep_dive_query(&db, "process", Some("src/lib.rs"), depth, 20, 20).unwrap();

        assert!(
            output.contains("complexity: decisions=4 loops=2 nesting=3 params=2 lines=8"),
            "missing stored complexity at {depth} depth:\n{output}"
        );
    }
}

#[test]
fn deep_dive_omits_complexity_line_when_metric_is_absent() {
    let (_temp, db) = seeded_db(&[]);

    let output = deep_dive_query(&db, "process", Some("src/lib.rs"), "overview", 20, 20).unwrap();

    assert!(!output.contains("complexity:"));
}
