use super::*;
use julie_core::database::StructuralFactQuery;
use julie_extractors::base::SourceRegionKind;

#[tokio::test]
async fn watcher_replaces_all_extractor_enrichment_domains() {
    let temp_dir = julie_test_support::unique_temp_dir("watcher_extractor_enrichments");
    let workspace_root = temp_dir.path().canonicalize().unwrap();
    let test_file = workspace_root.join("rust_http_client.rs");
    fs::write(
        &test_file,
        include_str!("../../../../../fixtures/extraction/consumer-upgrade/rust_http_client.rs"),
    )
    .unwrap();

    let db_path = workspace_root.join("test.db");
    let db = Arc::new(Mutex::new(
        SymbolDatabase::new(&db_path).expect("create test database"),
    ));
    let extractor_manager = Arc::new(ExtractorManager::new());
    let guard = acquire_gate("watcher_replaces_extractor_enrichments").await;

    handle_file_created_or_modified_static(
        test_file.canonicalize().unwrap(),
        &db,
        &extractor_manager,
        &workspace_root,
        None,
        &guard,
    )
    .await
    .expect("initial indexing succeeds");

    {
        let db = db.lock().unwrap();
        assert!(
            db.get_source_regions_for_file("rust_http_client.rs", &[])
                .unwrap()
                .iter()
                .any(|region| region.kind == SourceRegionKind::DocComment)
        );
        assert!(
            !db.search_structural_facts(&StructuralFactQuery {
                pattern_ids: vec!["http.client_request.v1".into()],
                ..Default::default()
            })
            .unwrap()
            .is_empty()
        );
        let symbol = db
            .find_symbols_by_name("fetch_user")
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        let metric = db
            .get_complexity_metric_for_symbol(&symbol.id)
            .unwrap()
            .unwrap();
        assert!(metric.decision_count >= 1);
        assert!(metric.loop_count >= 1);
    }

    fs::write(
        &test_file,
        "pub async fn fetch_user(_enabled: bool, _retries: usize) -> Result<(), reqwest::Error> {\n    Ok(())\n}\n",
    )
    .unwrap();
    handle_file_created_or_modified_static(
        test_file.canonicalize().unwrap(),
        &db,
        &extractor_manager,
        &workspace_root,
        None,
        &guard,
    )
    .await
    .expect("replacement indexing succeeds");

    let db = db.lock().unwrap();
    assert!(
        db.get_source_regions_for_file("rust_http_client.rs", &[])
            .unwrap()
            .iter()
            .all(|region| region.kind != SourceRegionKind::DocComment)
    );
    assert!(
        db.search_structural_facts(&StructuralFactQuery {
            pattern_ids: vec!["http.client_request.v1".into()],
            ..Default::default()
        })
        .unwrap()
        .is_empty()
    );
    let symbol = db
        .find_symbols_by_name("fetch_user")
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let metric = db
        .get_complexity_metric_for_symbol(&symbol.id)
        .unwrap()
        .unwrap();
    assert_eq!(metric.decision_count, 0);
    assert_eq!(metric.loop_count, 0);
    assert_eq!(metric.parameter_count, Some(2));
}
