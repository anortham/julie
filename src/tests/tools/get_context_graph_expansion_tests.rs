//! Tests for get_context graph expansion behavior.

#[cfg(test)]
mod graph_expansion_tests {
    use std::collections::HashMap;

    use tempfile::TempDir;

    use crate::database::{FileInfo, SymbolDatabase};
    use crate::extractors::base::{Relationship, RelationshipKind, Symbol, SymbolKind, Visibility};
    use crate::search::index::SymbolSearchResult;
    use crate::tools::get_context::pipeline::{NeighborDirection, Pivot, expand_graph};

    fn setup_db() -> (TempDir, SymbolDatabase) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        for file in &[
            "src/main.rs",
            "src/engine.rs",
            "src/handler.rs",
            "src/utils.rs",
        ] {
            db.store_file_info(&FileInfo {
                path: file.to_string(),
                language: "rust".to_string(),
                hash: format!("hash_{}", file),
                size: 500,
                last_modified: 1000000,
                last_indexed: 0,
                symbol_count: 2,
                line_count: 0,
                content: None,
            })
            .unwrap();
        }

        (temp_dir, db)
    }

    fn make_symbol(id: &str, name: &str, kind: SymbolKind, file: &str, line: u32) -> Symbol {
        Symbol {
            id: id.to_string(),
            name: name.to_string(),
            kind,
            language: "rust".to_string(),
            file_path: file.to_string(),
            start_line: line,
            end_line: line + 10,
            start_column: 0,
            end_column: 0,
            start_byte: 0,
            end_byte: 100,
            parent_id: None,
            signature: Some(format!("fn {}()", name)),
            doc_comment: None,
            visibility: Some(Visibility::Public),
            metadata: None,
            semantic_group: None,
            confidence: Some(0.9),
            code_context: None,
            content_type: None,
        }
    }

    fn make_rel(
        id: &str,
        from: &str,
        to: &str,
        kind: RelationshipKind,
        file: &str,
        line: u32,
    ) -> Relationship {
        Relationship {
            id: id.to_string(),
            from_symbol_id: from.to_string(),
            to_symbol_id: to.to_string(),
            kind,
            file_path: file.to_string(),
            line_number: line,
            confidence: 0.9,
            metadata: None,
        }
    }

    fn make_pivot(id: &str, name: &str, score: f32) -> Pivot {
        Pivot {
            result: SymbolSearchResult {
                id: id.to_string(),
                name: name.to_string(),
                signature: format!("fn {}()", name),
                doc_comment: String::new(),
                file_path: format!("src/{}.rs", name),
                kind: "function".to_string(),
                language: "rust".to_string(),
                start_line: 1,
                score,
            },
            combined_score: score,
        }
    }

    #[test]
    fn test_expand_graph_basic_incoming_and_outgoing() {
        let (_tmp, mut db) = setup_db();

        let symbols = vec![
            make_symbol(
                "sym_engine",
                "engine_run",
                SymbolKind::Function,
                "src/engine.rs",
                10,
            ),
            make_symbol("sym_main", "main", SymbolKind::Function, "src/main.rs", 1),
            make_symbol(
                "sym_utils",
                "helper",
                SymbolKind::Function,
                "src/utils.rs",
                5,
            ),
        ];
        db.store_symbols(&symbols).unwrap();

        let rels = vec![
            make_rel(
                "r1",
                "sym_main",
                "sym_engine",
                RelationshipKind::Calls,
                "src/main.rs",
                5,
            ),
            make_rel(
                "r2",
                "sym_engine",
                "sym_utils",
                RelationshipKind::Calls,
                "src/engine.rs",
                15,
            ),
        ];
        db.store_relationships(&rels).unwrap();

        let pivots = vec![make_pivot("sym_engine", "engine_run", 10.0)];
        let expansion = expand_graph(&pivots, &db).unwrap();

        assert_eq!(expansion.neighbors.len(), 2);
        let names: Vec<&str> = expansion
            .neighbors
            .iter()
            .map(|n| n.symbol.name.as_str())
            .collect();
        assert!(names.contains(&"main"));
        assert!(names.contains(&"helper"));

        let main_neighbor = expansion
            .neighbors
            .iter()
            .find(|n| n.symbol.name == "main")
            .unwrap();
        assert_eq!(main_neighbor.direction, NeighborDirection::Incoming);
        assert_eq!(main_neighbor.relationship_kind, RelationshipKind::Calls);

        let helper_neighbor = expansion
            .neighbors
            .iter()
            .find(|n| n.symbol.name == "helper")
            .unwrap();
        assert_eq!(helper_neighbor.direction, NeighborDirection::Outgoing);
        assert_eq!(helper_neighbor.relationship_kind, RelationshipKind::Calls);
    }

    #[test]
    fn test_expand_graph_deduplicates_shared_neighbors() {
        let (_tmp, mut db) = setup_db();

        let symbols = vec![
            make_symbol(
                "sym_engine",
                "engine_run",
                SymbolKind::Function,
                "src/engine.rs",
                10,
            ),
            make_symbol(
                "sym_handler",
                "handle",
                SymbolKind::Function,
                "src/handler.rs",
                1,
            ),
            make_symbol(
                "sym_utils",
                "helper",
                SymbolKind::Function,
                "src/utils.rs",
                5,
            ),
        ];
        db.store_symbols(&symbols).unwrap();

        let rels = vec![
            make_rel(
                "r1",
                "sym_engine",
                "sym_utils",
                RelationshipKind::Calls,
                "src/engine.rs",
                15,
            ),
            make_rel(
                "r2",
                "sym_handler",
                "sym_utils",
                RelationshipKind::Calls,
                "src/handler.rs",
                10,
            ),
        ];
        db.store_relationships(&rels).unwrap();

        let pivots = vec![
            make_pivot("sym_engine", "engine_run", 10.0),
            make_pivot("sym_handler", "handle", 8.0),
        ];
        let expansion = expand_graph(&pivots, &db).unwrap();

        assert_eq!(expansion.neighbors.len(), 1);
        assert_eq!(expansion.neighbors[0].symbol.name, "helper");
    }

    #[test]
    fn test_expand_graph_ranks_by_reference_score() {
        let (_tmp, mut db) = setup_db();

        let symbols = vec![
            make_symbol(
                "sym_engine",
                "engine_run",
                SymbolKind::Function,
                "src/engine.rs",
                10,
            ),
            make_symbol("sym_main", "main", SymbolKind::Function, "src/main.rs", 1),
            make_symbol(
                "sym_utils",
                "helper",
                SymbolKind::Function,
                "src/utils.rs",
                5,
            ),
        ];
        db.store_symbols(&symbols).unwrap();

        let rels = vec![
            make_rel(
                "r1",
                "sym_main",
                "sym_engine",
                RelationshipKind::Calls,
                "src/main.rs",
                5,
            ),
            make_rel(
                "r2",
                "sym_utils",
                "sym_engine",
                RelationshipKind::Calls,
                "src/utils.rs",
                8,
            ),
            make_rel(
                "r3",
                "sym_utils",
                "sym_main",
                RelationshipKind::Calls,
                "src/utils.rs",
                9,
            ),
            make_rel(
                "r4",
                "sym_engine",
                "sym_main",
                RelationshipKind::Calls,
                "src/engine.rs",
                20,
            ),
        ];
        db.store_relationships(&rels).unwrap();
        db.compute_reference_scores().unwrap();

        let pivots = vec![make_pivot("sym_engine", "engine_run", 10.0)];
        let expansion = expand_graph(&pivots, &db).unwrap();

        assert_eq!(expansion.neighbors.len(), 2);
        assert_eq!(expansion.neighbors[0].symbol.name, "main");
        assert!(expansion.neighbors[0].reference_score > expansion.neighbors[1].reference_score);
    }

    #[test]
    fn test_expand_graph_no_relationships() {
        let (_tmp, mut db) = setup_db();

        let symbols = vec![make_symbol(
            "sym_lonely",
            "lonely_fn",
            SymbolKind::Function,
            "src/main.rs",
            1,
        )];
        db.store_symbols(&symbols).unwrap();

        let pivots = vec![make_pivot("sym_lonely", "lonely_fn", 5.0)];
        let expansion = expand_graph(&pivots, &db).unwrap();
        assert!(expansion.neighbors.is_empty());
    }

    #[test]
    fn test_expand_graph_excludes_pivots_from_neighbors() {
        let (_tmp, mut db) = setup_db();

        let symbols = vec![
            make_symbol(
                "sym_engine",
                "engine_run",
                SymbolKind::Function,
                "src/engine.rs",
                10,
            ),
            make_symbol(
                "sym_handler",
                "handle",
                SymbolKind::Function,
                "src/handler.rs",
                1,
            ),
            make_symbol(
                "sym_utils",
                "helper",
                SymbolKind::Function,
                "src/utils.rs",
                5,
            ),
        ];
        db.store_symbols(&symbols).unwrap();

        let rels = vec![
            make_rel(
                "r1",
                "sym_engine",
                "sym_handler",
                RelationshipKind::Calls,
                "src/engine.rs",
                15,
            ),
            make_rel(
                "r2",
                "sym_handler",
                "sym_engine",
                RelationshipKind::Calls,
                "src/handler.rs",
                5,
            ),
            make_rel(
                "r3",
                "sym_engine",
                "sym_utils",
                RelationshipKind::Calls,
                "src/engine.rs",
                20,
            ),
        ];
        db.store_relationships(&rels).unwrap();

        let pivots = vec![
            make_pivot("sym_engine", "engine_run", 10.0),
            make_pivot("sym_handler", "handle", 8.0),
        ];
        let expansion = expand_graph(&pivots, &db).unwrap();

        assert_eq!(expansion.neighbors.len(), 1);
        assert_eq!(expansion.neighbors[0].symbol.name, "helper");

        let neighbor_ids: Vec<&str> = expansion
            .neighbors
            .iter()
            .map(|n| n.symbol.id.as_str())
            .collect();
        assert!(!neighbor_ids.contains(&"sym_engine"));
        assert!(!neighbor_ids.contains(&"sym_handler"));
    }

    #[test]
    fn test_expand_graph_empty_pivots() {
        let (_tmp, db) = setup_db();
        let expansion = expand_graph(&[], &db).unwrap();
        assert!(expansion.neighbors.is_empty());
    }

    /// When no relationships exist but identifier refs do (the TypeScript case), expand_graph
    /// must find neighbors via identifiers. A pivot with 0 relationships but N type_usage
    /// identifier refs from other symbols should produce N neighbor entries.
    #[test]
    fn test_expand_graph_finds_neighbors_via_identifiers_when_no_relationships() {
        let (_tmp, mut db) = setup_db();

        // Pivot: an interface with no relationship edges
        let pivot_sym = make_symbol(
            "sym_iface",
            "ZodInterface",
            SymbolKind::Interface,
            "src/main.rs",
            1,
        );
        // Two symbols that reference the interface via type_usage identifiers
        let caller_a = make_symbol(
            "sym_validator",
            "validate_input",
            SymbolKind::Function,
            "src/handler.rs",
            5,
        );
        let caller_b = make_symbol(
            "sym_processor",
            "process_data",
            SymbolKind::Function,
            "src/utils.rs",
            10,
        );
        db.store_symbols(&[pivot_sym, caller_a, caller_b]).unwrap();

        // Insert type_usage identifiers (no relationships)
        for (sym_id, file, line) in &[
            ("sym_validator", "src/handler.rs", 6u32),
            ("sym_processor", "src/utils.rs", 11),
        ] {
            db.conn.execute(
                "INSERT INTO identifiers (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, containing_symbol_id, confidence)
                 VALUES (?1, 'ZodInterface', 'type_usage', 'typescript', ?2, ?3, 0, ?3, 10, 0, 100, ?4, 0.9)",
                rusqlite::params![
                    format!("ident_{}_{}", sym_id, line),
                    file, line, sym_id
                ],
            ).unwrap();
        }

        let pivots = vec![make_pivot("sym_iface", "ZodInterface", 9.0)];
        let expansion = expand_graph(&pivots, &db).unwrap();

        assert_eq!(
            expansion.neighbors.len(),
            2,
            "Expected 2 identifier-based neighbors; got {:?}",
            expansion
                .neighbors
                .iter()
                .map(|n| &n.symbol.name)
                .collect::<Vec<_>>()
        );
        let names: Vec<&str> = expansion
            .neighbors
            .iter()
            .map(|n| n.symbol.name.as_str())
            .collect();
        assert!(
            names.contains(&"validate_input"),
            "validate_input should be a neighbor"
        );
        assert!(
            names.contains(&"process_data"),
            "process_data should be a neighbor"
        );
    }

    /// Identifiers with names containing SQL wildcard chars (`_`, `%`) must NOT
    /// match unrelated identifiers. Regression test for the unescaped LIKE bug in
    /// `build_name_match_clause`.
    #[test]
    fn test_expand_graph_identifier_names_with_sql_wildcards_do_not_overmatch() {
        let (_tmp, mut db) = setup_db();

        let symbols = vec![
            make_symbol(
                "sym_foo_bar",
                "Foo_Bar",
                SymbolKind::Interface,
                "src/main.rs",
                1,
            ),
            make_symbol(
                "sym_caller_good",
                "good_caller",
                SymbolKind::Function,
                "src/handler.rs",
                5,
            ),
            make_symbol(
                "sym_caller_bad",
                "bad_caller",
                SymbolKind::Function,
                "src/utils.rs",
                10,
            ),
        ];
        db.store_symbols(&symbols).unwrap();

        // "Foo_Bar::method" — qualified ref, should match pivot "Foo_Bar" via LIKE prefix
        db.conn.execute(
            "INSERT INTO identifiers (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, containing_symbol_id, confidence)
             VALUES ('ident_good', 'Foo_Bar::method', 'call', 'rust', 'src/handler.rs', 6, 0, 6, 10, 0, 100, 'sym_caller_good', 0.9)",
            [],
        ).unwrap();

        // "FooXBar::method" — should NOT match pivot "Foo_Bar" (the _ in Foo_Bar must not wildcard-match X)
        db.conn.execute(
            "INSERT INTO identifiers (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, containing_symbol_id, confidence)
             VALUES ('ident_bad', 'FooXBar::method', 'call', 'rust', 'src/utils.rs', 11, 0, 11, 10, 0, 100, 'sym_caller_bad', 0.9)",
            [],
        ).unwrap();

        let pivots = vec![make_pivot("sym_foo_bar", "Foo_Bar", 9.0)];
        let expansion = expand_graph(&pivots, &db).unwrap();

        let names: Vec<&str> = expansion
            .neighbors
            .iter()
            .map(|n| n.symbol.name.as_str())
            .collect();
        assert!(
            names.contains(&"good_caller"),
            "Foo_Bar::method should match pivot Foo_Bar"
        );
        assert!(
            !names.contains(&"bad_caller"),
            "FooXBar::method must NOT match pivot Foo_Bar (SQL wildcard bug)"
        );
    }
}
