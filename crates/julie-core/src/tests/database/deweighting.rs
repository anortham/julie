use super::*;

/// Test that symbols in test files get de-weighted during centrality computation.
/// Reproduces the Flask problem: tests/test_config.py defines `class Flask(flask.Flask)`
/// which steals centrality from the real `src/flask/app.py::Flask` because both share
/// the name and the test subclass accumulates references from test files.
#[test]
fn test_centrality_deweights_test_file_symbols() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    db.store_file_info(
        &file_info_builder("src/flask/app.py")
            .language("python")
            .hash("real")
            .size(5000)
            .last_modified(1)
            .last_indexed(0)
            .symbol_count(1)
            .line_count(0)
            .build(),
    )
    .unwrap();

    db.store_file_info(
        &file_info_builder("tests/test_config.py")
            .language("python")
            .hash("test")
            .size(1000)
            .last_modified(1)
            .last_indexed(0)
            .symbol_count(1)
            .line_count(0)
            .build(),
    )
    .unwrap();

    for i in 0..5 {
        db.store_file_info(
            &file_info_builder(format!("tests/test_app_{}.py", i))
                .language("python")
                .hash(format!("testhash{}", i))
                .size(500)
                .last_modified(1)
                .last_indexed(0)
                .symbol_count(1)
                .line_count(0)
                .build(),
        )
        .unwrap();
    }

    db.store_file_info(
        &file_info_builder("src/flask/views.py")
            .language("python")
            .hash("views")
            .size(2000)
            .last_modified(1)
            .last_indexed(0)
            .symbol_count(1)
            .line_count(0)
            .build(),
    )
    .unwrap();

    let mut symbols = vec![
        symbol_builder("real_flask", "Flask", "src/flask/app.py")
            .kind(SymbolKind::Class)
            .language("python")
            .span(1, 0, 500, 1)
            .bytes(0, 10000)
            .confidence(1.0)
            .build(),
        symbol_builder("test_flask", "Flask", "tests/test_config.py")
            .kind(SymbolKind::Class)
            .language("python")
            .span(1, 0, 50, 1)
            .bytes(0, 1000)
            .confidence(1.0)
            .build(),
    ];

    for i in 0..5 {
        symbols.push(
            symbol_builder(
                format!("test_caller_{}", i),
                format!("test_func_{}", i),
                format!("tests/test_app_{}.py", i),
            )
            .kind(SymbolKind::Function)
            .language("python")
            .span(1, 0, 10, 1)
            .bytes(0, 100)
            .confidence(1.0)
            .build(),
        );
    }

    symbols.push(
        symbol_builder("real_caller", "create_app", "src/flask/views.py")
            .kind(SymbolKind::Function)
            .language("python")
            .span(1, 0, 10, 1)
            .bytes(0, 200)
            .confidence(1.0)
            .build(),
    );

    db.store_symbols(&symbols).unwrap();

    let mut relationships: Vec<_> = (0..5)
        .map(|i| {
            relationship_builder(
                format!("r_test_{}", i),
                format!("test_caller_{}", i),
                "test_flask",
            )
            .kind(RelationshipKind::Instantiates)
            .line_number(0)
            .build()
        })
        .collect();
    relationships.push(
        relationship_builder("r_real", "real_caller", "real_flask")
            .kind(RelationshipKind::Instantiates)
            .line_number(0)
            .build(),
    );
    db.store_relationships(&relationships).unwrap();

    db.compute_reference_scores().unwrap();

    let real_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'real_flask'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let test_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'test_flask'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    // Real Flask should have HIGHER centrality than test Flask
    assert!(
        real_score > test_score,
        "Real Flask (score={}) should have higher centrality than test Flask (score={}). \
         Test-file symbols should be de-weighted.",
        real_score,
        test_score
    );

    // Test Flask score should be significantly reduced (at most 50% of raw score)
    assert!(
        test_score < 5.0,
        "Test Flask score ({}) should be significantly reduced from raw 10.0",
        test_score
    );
}

/// Step 1b excludes test-file symbols from the name-based identifier boost.
/// Two class symbols named "Flask" — one in production, one in tests — should
/// receive very different centrality when cross-file type_usage identifiers exist.
#[test]
fn test_step1b_identifier_boost_excludes_test_file_symbols() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    for (path, lang) in [
        ("src/app.py", "python"),
        ("tests/test_config.py", "python"),
        ("src/routes.py", "python"),
        ("src/views.py", "python"),
        ("src/auth.py", "python"),
        ("src/models.py", "python"),
        ("src/forms.py", "python"),
        ("src/cli.py", "python"),
        ("src/utils.py", "python"),
        ("src/admin.py", "python"),
        ("src/api.py", "python"),
        ("src/middleware.py", "python"),
    ] {
        db.store_file_info(
            &file_info_builder(path)
                .language(lang)
                .hash("abc")
                .size(100)
                .last_modified(0)
                .last_indexed(0)
                .symbol_count(1)
                .line_count(0)
                .build(),
        )
        .unwrap();
    }

    let symbols = [
        // Production Flask class
        symbol_builder("flask_prod", "Flask", "src/app.py")
            .kind(SymbolKind::Class)
            .language("python")
            .span(109, 0, 500, 1)
            .bytes(0, 10000)
            .confidence(1.0)
            .build(),
        // Test Flask class
        symbol_builder("flask_test", "Flask", "tests/test_config.py")
            .kind(SymbolKind::Class)
            .language("python")
            .span(202, 0, 250, 1)
            .bytes(0, 5000)
            .confidence(1.0)
            .build(),
    ];
    db.store_symbols(&symbols).unwrap();

    // Add type_usage identifiers named "Flask" from 10 non-test files
    let source_files = [
        "src/routes.py",
        "src/views.py",
        "src/auth.py",
        "src/models.py",
        "src/forms.py",
        "src/cli.py",
        "src/utils.py",
        "src/admin.py",
        "src/api.py",
        "src/middleware.py",
    ];
    let identifiers: Vec<_> = source_files
        .iter()
        .enumerate()
        .map(|(i, file)| {
            identifier_builder(format!("id_type_{}", i), "Flask", *file)
                .kind(IdentifierKind::TypeUsage)
                .language("python")
                .line(1)
                .column(0, 10)
                .build()
        })
        .collect();
    db.bulk_store_identifiers(&identifiers, "").unwrap();

    db.compute_reference_scores().unwrap();

    let prod_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'flask_prod'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let test_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'flask_test'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert!(
        prod_score > 0.0,
        "Production Flask should receive Step 1b identifier boost, got {}",
        prod_score
    );
    assert!(
        (test_score - 0.0).abs() < f64::EPSILON,
        "Test-file Flask should NOT receive Step 1b identifier boost, got {}",
        test_score
    );
}
