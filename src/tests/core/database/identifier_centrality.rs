use super::*;

/// Verify that TypeUsage identifiers contribute to centrality even without relationships.
/// This fixes the GDScript pattern where classes are referenced via type annotations
/// (var x: PandoraEntity, func f() -> PandoraEntity) but no call relationships exist.
#[test]
fn test_compute_reference_scores_includes_type_usage_identifiers() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Insert two files
    for (path, lang) in [
        ("model/entity.gd", "gdscript"),
        ("backend/api.gd", "gdscript"),
    ] {
        db.store_file_info(
            &file_info_builder(path)
                .language(lang)
                .hash("abc123")
                .size(100)
                .last_modified(1234567890)
                .last_indexed(0)
                .symbol_count(2)
                .line_count(0)
                .build(),
        )
        .unwrap();
    }

    let symbols = [
        symbol_builder("entity_class", "PandoraEntity", "model/entity.gd")
            .kind(SymbolKind::Class)
            .language("gdscript")
            .span(1, 0, 100, 1)
            .bytes(0, 500)
            .confidence(1.0)
            .build(),
        symbol_builder("api_func", "create_entity", "backend/api.gd")
            .kind(SymbolKind::Function)
            .language("gdscript")
            .span(10, 0, 20, 1)
            .bytes(0, 200)
            .confidence(1.0)
            .build(),
    ];
    db.store_symbols(&symbols).unwrap();

    // NO relationships — this is the key. Only identifiers.
    // Insert TypeUsage identifiers pointing to PandoraEntity by name from api.gd
    let identifiers: Vec<_> = [("id1", 10), ("id2", 15), ("id3", 18)]
        .iter()
        .map(|(id, line)| {
            identifier_builder(*id, "PandoraEntity", "backend/api.gd")
                .kind(IdentifierKind::TypeUsage)
                .language("gdscript")
                .line(*line)
                .column(0, 15)
                .containing_symbol_id("api_func")
                .build()
        })
        .collect();
    db.bulk_store_identifiers(&identifiers, "").unwrap();

    // Compute scores
    db.compute_reference_scores().unwrap();

    // PandoraEntity should now have non-zero centrality from type usage identifiers
    let entity_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'entity_class'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        entity_score > 0.0,
        "PandoraEntity should have non-zero centrality from TypeUsage identifiers, got {}",
        entity_score
    );

    // create_entity function has no incoming refs — should stay at 0
    let func_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'api_func'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        (func_score - 0.0).abs() < f64::EPSILON,
        "create_entity should have 0.0 centrality (no incoming refs), got {}",
        func_score
    );
}

/// Verify that Zig-style type constants (const Server = @This()) get centrality from TypeUsage.
/// In Zig, types are `constant` kind, not `class`/`struct`.
#[test]
fn test_compute_reference_scores_includes_constants_with_type_usage() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    for (path, lang) in [("src/Server.zig", "zig"), ("src/main.zig", "zig")] {
        db.store_file_info(
            &file_info_builder(path)
                .language(lang)
                .hash("abc123")
                .size(100)
                .last_modified(1234567890)
                .last_indexed(0)
                .symbol_count(2)
                .line_count(0)
                .build(),
        )
        .unwrap();
    }

    let symbols = [
        // Zig type constant: const Server = @This()
        symbol_builder("server_const", "Server", "src/Server.zig")
            .kind(SymbolKind::Constant)
            .language("zig")
            .span(1, 0, 1, 1)
            .bytes(0, 30)
            .confidence(1.0)
            .build(),
        // A plain constant that's NOT used as a type (should NOT get boosted)
        symbol_builder("max_const", "max_retries", "src/Server.zig")
            .kind(SymbolKind::Constant)
            .language("zig")
            .span(5, 0, 5, 1)
            .bytes(0, 30)
            .confidence(1.0)
            .build(),
    ];
    db.store_symbols(&symbols).unwrap();

    // TypeUsage identifiers referencing Server from main.zig
    let identifiers: Vec<_> = [("id1", 10), ("id2", 20), ("id3", 30)]
        .iter()
        .map(|(id, line)| {
            identifier_builder(*id, "Server", "src/main.zig")
                .kind(IdentifierKind::TypeUsage)
                .language("zig")
                .line(*line)
                .column(0, 10)
                .build()
        })
        .collect();
    db.bulk_store_identifiers(&identifiers, "").unwrap();

    db.compute_reference_scores().unwrap();

    // Server constant should get centrality from TypeUsage identifiers
    let server_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'server_const'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        server_score > 0.0,
        "Zig type constant 'Server' should have non-zero centrality from TypeUsage, got {}",
        server_score
    );

    // max_retries has no TypeUsage identifiers — should stay at 0
    let max_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'max_const'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        (max_score - 0.0).abs() < f64::EPSILON,
        "Plain constant 'max_retries' should have 0.0 centrality (no TypeUsage refs), got {}",
        max_score
    );
}

/// Verify that import identifiers contribute to centrality.
/// In Zig, cross-file references are primarily @import() which produce import-kind identifiers.
/// A symbol imported in 15 files should have significant centrality.
#[test]
fn test_compute_reference_scores_includes_import_identifiers() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    // Create files
    for path in ["src/Store.zig", "src/a.zig", "src/b.zig", "src/c.zig"] {
        db.store_file_info(&FileInfo {
            path: path.to_string(),
            language: "zig".to_string(),
            hash: "abc123".to_string(),
            size: 100,
            last_modified: 1234567890,
            last_indexed: 0,
            symbol_count: 1,
            line_count: 0,
            content: None,
        })
        .unwrap();
    }

    // The type constant being imported
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path, start_line, end_line, start_col, end_col, start_byte, end_byte)
             VALUES ('store', 'DocumentStore', 'constant', 'zig', 'src/Store.zig', 1, 1, 0, 1, 0, 30)",
            [],
        )
        .unwrap();

    // Import identifiers from 3 different files (weight 2.0 each)
    for (id, file) in [
        ("imp1", "src/a.zig"),
        ("imp2", "src/b.zig"),
        ("imp3", "src/c.zig"),
    ] {
        db.conn
            .execute(
                "INSERT INTO identifiers (id, name, kind, language, file_path, start_line, start_col, end_line, end_col)
                 VALUES (?1, 'DocumentStore', 'import', 'zig', ?2, 1, 0, 1, 30)",
                rusqlite::params![id, file],
            )
            .unwrap();
    }

    db.compute_reference_scores().unwrap();

    let score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'store'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    // 3 imports × 2.0 weight = 6.0
    assert!(
        (score - 6.0).abs() < f64::EPSILON,
        "DocumentStore should have centrality 6.0 from 3 imports (3 × 2.0), got {}",
        score
    );
}

/// Verify that qualified-name identifiers (e.g. Kirigami.ScrollablePage) match
/// unqualified symbol names (ScrollablePage) for centrality computation.
/// QML uses namespace-qualified references heavily: `Kirigami.ScrollablePage {}`.
/// Without this, all QML components have centrality 0.00 despite heavy usage.
#[test]
fn test_compute_reference_scores_qualified_name_identifiers() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    for (path, lang) in [
        ("src/controls/ScrollablePage.qml", "qml"),
        ("src/controls/AboutPage.qml", "qml"),
        ("examples/SimplePage.qml", "qml"),
    ] {
        db.store_file_info(
            &file_info_builder(path)
                .language(lang)
                .hash("abc123")
                .size(100)
                .last_modified(1234567890)
                .last_indexed(0)
                .symbol_count(2)
                .line_count(0)
                .build(),
        )
        .unwrap();
    }

    let symbols = [
        // QML component: file-derived name "ScrollablePage"
        symbol_builder(
            "scrollable_page",
            "ScrollablePage",
            "src/controls/ScrollablePage.qml",
        )
        .kind(SymbolKind::Class)
        .language("qml")
        .span(1, 0, 100, 1)
        .bytes(0, 2000)
        .confidence(1.0)
        .build(),
        // Another component that should NOT be matched
        symbol_builder("about_page", "AboutPage", "src/controls/AboutPage.qml")
            .kind(SymbolKind::Class)
            .language("qml")
            .span(1, 0, 50, 1)
            .bytes(0, 1000)
            .confidence(1.0)
            .build(),
    ];
    db.store_symbols(&symbols).unwrap();

    // TypeUsage identifiers using QUALIFIED name: "Kirigami.ScrollablePage"
    // These reference ScrollablePage but through a namespace prefix
    let identifiers: Vec<_> = [("id1", 10), ("id2", 20), ("id3", 30)]
        .iter()
        .map(|(id, line)| {
            identifier_builder(*id, "Kirigami.ScrollablePage", "examples/SimplePage.qml")
                .kind(IdentifierKind::TypeUsage)
                .language("qml")
                .line(*line)
                .column(0, 30)
                .build()
        })
        .collect();
    db.bulk_store_identifiers(&identifiers, "").unwrap();

    db.compute_reference_scores().unwrap();

    // ScrollablePage should have non-zero centrality from qualified type usage identifiers
    let score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'scrollable_page'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    // 3 type_usage × 1.0 weight = 3.0
    assert!(
        (score - 3.0).abs() < f64::EPSILON,
        "ScrollablePage should have centrality 3.0 from 3 qualified TypeUsage refs (Kirigami.ScrollablePage), got {}",
        score
    );

    // AboutPage should NOT be boosted — no identifiers reference it
    let about_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'about_page'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        (about_score - 0.0).abs() < f64::EPSILON,
        "AboutPage should have 0.0 centrality (no matching identifiers), got {}",
        about_score
    );
}

#[test]
fn test_compute_reference_scores_escapes_like_wildcards() {
    // Symbol names with underscores must NOT act as SQL LIKE wildcards.
    // `_` in LIKE matches any single character, so `user_id` would match
    // `userXid` without escaping. This test verifies proper escaping.
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    for path in ["src/models.py", "src/views.py"] {
        db.store_file_info(
            &file_info_builder(path)
                .language("python")
                .hash("abc123")
                .size(100)
                .last_modified(1234567890)
                .last_indexed(0)
                .symbol_count(2)
                .line_count(0)
                .build(),
        )
        .unwrap();
    }

    let symbols = [
        // Symbol with underscore: user_id
        symbol_builder("sym_user_id", "user_id", "src/models.py")
            .kind(SymbolKind::Class)
            .language("python")
            .span(1, 0, 10, 1)
            .bytes(0, 200)
            .confidence(1.0)
            .build(),
        // Symbol WITHOUT underscore that would match if _ is a wildcard: userXid
        symbol_builder("sym_userXid", "userXid", "src/models.py")
            .kind(SymbolKind::Class)
            .language("python")
            .span(20, 0, 30, 1)
            .bytes(0, 200)
            .confidence(1.0)
            .build(),
    ];
    db.store_symbols(&symbols).unwrap();

    // Qualified identifier referencing "models.userXid"
    // This should match userXid (exact suffix) but NOT user_id (would only
    // match if underscore acts as wildcard)
    let identifiers = [identifier_builder("ref1", "models.userXid", "src/views.py")
        .kind(IdentifierKind::TypeUsage)
        .language("python")
        .line(5)
        .column(0, 20)
        .build()];
    db.bulk_store_identifiers(&identifiers, "").unwrap();

    db.compute_reference_scores().unwrap();

    let user_id_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'sym_user_id'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    let userxid_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'sym_userXid'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    // user_id must NOT be boosted — "models.userXid" should not match "user_id"
    assert!(
        (user_id_score - 0.0).abs() < f64::EPSILON,
        "user_id must NOT match 'models.userXid' via underscore wildcard. Got score: {}",
        user_id_score
    );

    // userXid SHOULD be boosted — "models.userXid" matches via suffix
    assert!(
        userxid_score > 0.0,
        "userXid should be boosted by 'models.userXid' qualified ref. Got score: {}",
        userxid_score
    );
}
