use super::*;

/// C# DI pattern: interface gets all the centrality, concrete implementation gets zero.
/// After propagation, the implementing class should inherit a fraction of the interface's score.
#[test]
fn test_compute_reference_scores_propagates_interface_centrality_to_implementations() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    db.store_file_info(&FileInfo {
        path: "test.cs".to_string(),
        language: "csharp".to_string(),
        hash: "abc123".to_string(),
        size: 1000,
        last_modified: 1234567890,
        last_indexed: 0,
        symbol_count: 5,
        line_count: 0,
        content: None,
    })
    .unwrap();

    // IService (interface), ServiceImpl (class), 3 consumers
    let symbols: Vec<_> = [
        ("iservice", "IService", SymbolKind::Interface),
        ("service_impl", "ServiceImpl", SymbolKind::Class),
        ("consumer1", "Consumer1", SymbolKind::Class),
        ("consumer2", "Consumer2", SymbolKind::Class),
        ("consumer3", "Consumer3", SymbolKind::Class),
    ]
    .iter()
    .map(|(id, name, kind)| {
        symbol_builder(*id, *name, "test.cs")
            .kind(kind.clone())
            .language("csharp")
            .span(1, 0, 10, 1)
            .bytes(0, 100)
            .confidence(1.0)
            .build()
    })
    .collect();
    db.store_symbols(&symbols).unwrap();

    // ServiceImpl implements IService; consumers reference IService.
    let relationships: Vec<_> = [
        (
            "r_impl",
            "service_impl",
            "iservice",
            RelationshipKind::Implements,
        ),
        ("r1", "consumer1", "iservice", RelationshipKind::Uses),
        ("r2", "consumer2", "iservice", RelationshipKind::Uses),
        ("r3", "consumer3", "iservice", RelationshipKind::Uses),
    ]
    .iter()
    .map(|(rel_id, from_id, to_id, kind)| {
        relationship_builder(*rel_id, *from_id, *to_id)
            .kind(kind.clone())
            .line_number(0)
            .build()
    })
    .collect();
    db.store_relationships(&relationships).unwrap();

    db.compute_reference_scores().unwrap();

    // IService: 3 × uses(1) + 1 × implements(2) = 5.0
    let iservice_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'iservice'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        (iservice_score - 5.0).abs() < f64::EPSILON,
        "IService should have ref_score 5.0, got {}",
        iservice_score
    );

    // ServiceImpl should inherit centrality from IService via implements relationship
    let impl_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'service_impl'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        impl_score > 0.0,
        "ServiceImpl should inherit centrality from IService via implements, got {}",
        impl_score
    );
    // Should be a meaningful fraction of the interface's score
    assert!(
        impl_score >= iservice_score * 0.5,
        "ServiceImpl should get at least 50% of IService's score ({}), got {}",
        iservice_score,
        impl_score
    );
}

/// Same propagation should work for extends (class inheritance)
#[test]
fn test_compute_reference_scores_propagates_base_class_centrality() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    db.store_file_info(&FileInfo {
        path: "test.cs".to_string(),
        language: "csharp".to_string(),
        hash: "abc123".to_string(),
        size: 1000,
        last_modified: 1234567890,
        last_indexed: 0,
        symbol_count: 4,
        line_count: 0,
        content: None,
    })
    .unwrap();

    let symbols: Vec<_> = [
        ("base_class", "BaseService"),
        ("derived", "DerivedService"),
        ("caller1", "Caller1"),
        ("caller2", "Caller2"),
    ]
    .iter()
    .map(|(id, name)| {
        symbol_builder(*id, *name, "test.cs")
            .kind(SymbolKind::Class)
            .language("csharp")
            .span(1, 0, 10, 1)
            .bytes(0, 100)
            .confidence(1.0)
            .build()
    })
    .collect();
    db.store_symbols(&symbols).unwrap();

    let relationships: Vec<_> = [
        ("r_ext", "derived", "base_class", RelationshipKind::Extends),
        ("r1", "caller1", "base_class", RelationshipKind::Calls),
        ("r2", "caller2", "base_class", RelationshipKind::Calls),
    ]
    .iter()
    .map(|(rel_id, from_id, to_id, kind)| {
        relationship_builder(*rel_id, *from_id, *to_id)
            .kind(kind.clone())
            .line_number(0)
            .build()
    })
    .collect();
    db.store_relationships(&relationships).unwrap();

    db.compute_reference_scores().unwrap();

    // BaseService: 2 × calls(3) + 1 × extends(2) = 8.0
    let base_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'base_class'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        (base_score - 8.0).abs() < f64::EPSILON,
        "BaseService should have ref_score 8.0, got {}",
        base_score
    );

    // DerivedService should inherit some centrality
    let derived_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'derived'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        derived_score > 0.0,
        "DerivedService should inherit centrality from BaseService via extends, got {}",
        derived_score
    );
}
/// Task 6: Constructor centrality propagation to parent class.
/// In C# / Java / TypeScript with DI, all references target the constructor,
/// leaving the class itself with zero centrality.
#[test]
fn test_compute_reference_scores_propagates_constructor_centrality() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    db.store_file_info(&FileInfo {
        path: "src/services.cs".to_string(),
        language: "csharp".to_string(),
        hash: "abc123".to_string(),
        size: 1000,
        last_modified: 1234567890,
        last_indexed: 0,
        symbol_count: 5,
        line_count: 0,
        content: None,
    })
    .unwrap();

    db.store_file_info(&FileInfo {
        path: "src/program.cs".to_string(),
        language: "csharp".to_string(),
        hash: "def456".to_string(),
        size: 500,
        last_modified: 1234567890,
        last_indexed: 0,
        symbol_count: 2,
        line_count: 0,
        content: None,
    })
    .unwrap();

    let symbols = vec![
        symbol_builder("class_1", "LabTestService", "src/services.cs")
            .kind(SymbolKind::Class)
            .language("csharp")
            .span(1, 0, 100, 1)
            .bytes(0, 5000)
            .visibility(Visibility::Public)
            .confidence(1.0)
            .build(),
        symbol_builder("ctor_1", "LabTestService", "src/services.cs")
            .kind(SymbolKind::Constructor)
            .language("csharp")
            .span(10, 0, 15, 1)
            .bytes(200, 400)
            .visibility(Visibility::Public)
            .parent_id("class_1")
            .confidence(1.0)
            .build(),
        symbol_builder("caller_1", "ConfigureServices", "src/program.cs")
            .kind(SymbolKind::Method)
            .language("csharp")
            .span(50, 0, 80, 1)
            .bytes(0, 500)
            .visibility(Visibility::Public)
            .confidence(1.0)
            .build(),
        symbol_builder("caller_2", "TestSetup", "src/program.cs")
            .kind(SymbolKind::Method)
            .language("csharp")
            .span(50, 0, 80, 1)
            .bytes(0, 500)
            .visibility(Visibility::Public)
            .confidence(1.0)
            .build(),
    ];
    db.store_symbols(&symbols).unwrap();

    // Relationships: callers -> constructor (DI pattern)
    let relationships = vec![
        relationship_builder("rel_1", "caller_1", "ctor_1")
            .kind(RelationshipKind::Instantiates)
            .line_number(0)
            .build(),
        relationship_builder("rel_2", "caller_2", "ctor_1")
            .kind(RelationshipKind::Uses)
            .line_number(0)
            .build(),
    ];
    db.store_relationships(&relationships).unwrap();

    db.compute_reference_scores().unwrap();

    // Constructor should have centrality from DI references:
    // instantiates=2 + uses=1 = 3.0
    let ctor_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'ctor_1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        ctor_score > 0.0,
        "Constructor should have centrality from DI references, got {}",
        ctor_score
    );

    // Class should inherit constructor centrality
    let class_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'class_1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        class_score > 0.0,
        "Class should inherit constructor centrality, got {}",
        class_score
    );
    assert!(
        class_score >= ctor_score * 0.5,
        "Class should get at least 50% of constructor centrality (ctor={}), got {}",
        ctor_score,
        class_score
    );
}
/// C/C++ header/implementation centrality split: header declarations accumulate all
/// reference_score (via #include) while implementations in .c/.cpp get zero.
/// Step 5 propagates 70% of header centrality to same-named implementations.
#[test]
fn test_compute_reference_scores_propagates_header_to_implementation() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    for (path, lang) in [
        ("jq.h", "c"),
        ("execute.c", "c"),
        ("main.c", "c"),
        ("parser.c", "c"),
    ] {
        db.store_file_info(
            &file_info_builder(path)
                .language(lang)
                .hash("abc123")
                .size(1000)
                .last_modified(1234567890)
                .last_indexed(0)
                .symbol_count(5)
                .line_count(0)
                .build(),
        )
        .unwrap();
    }

    let mut symbols = vec![
        symbol_builder("jq_next_h", "jq_next", "jq.h")
            .kind(SymbolKind::Function)
            .language("c")
            .span(10, 0, 10, 30)
            .bytes(100, 130)
            .confidence(1.0)
            .build(),
        symbol_builder("jq_next_c", "jq_next", "execute.c")
            .kind(SymbolKind::Function)
            .language("c")
            .span(50, 0, 120, 1)
            .bytes(500, 2500)
            .confidence(1.0)
            .build(),
    ];

    for (id, name, file) in [
        ("caller_main", "main", "main.c"),
        ("caller_parse", "parse_input", "parser.c"),
        ("caller_run", "run_program", "main.c"),
    ] {
        symbols.push(
            symbol_builder(id, name, file)
                .kind(SymbolKind::Function)
                .language("c")
                .span(1, 0, 20, 1)
                .bytes(0, 300)
                .confidence(1.0)
                .build(),
        );
    }

    db.store_symbols(&symbols).unwrap();

    let relationships: Vec<_> = [
        ("rel_1", "caller_main", RelationshipKind::Calls),
        ("rel_2", "caller_parse", RelationshipKind::Calls),
        ("rel_3", "caller_run", RelationshipKind::Uses),
    ]
    .into_iter()
    .map(|(rel_id, from_id, kind)| {
        relationship_builder(rel_id, from_id, "jq_next_h")
            .kind(kind)
            .line_number(0)
            .build()
    })
    .collect();
    db.store_relationships(&relationships).unwrap();

    db.compute_reference_scores().unwrap();

    let header_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'jq_next_h'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        header_score > 0.0,
        "Header declaration should have centrality from callers, got {}",
        header_score
    );

    let impl_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'jq_next_c'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        impl_score > 0.0,
        "Implementation should get propagated centrality from header, got {}",
        impl_score
    );

    let expected_impl_score = header_score * 0.7;
    assert!(
        (impl_score - expected_impl_score).abs() < 0.01,
        "Implementation should get exactly 70% of header score (header={}, expected={}, got={})",
        header_score,
        expected_impl_score,
        impl_score
    );
}
