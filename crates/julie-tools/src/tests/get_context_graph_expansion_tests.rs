//! Tests for get_context graph expansion behavior.

#[cfg(test)]
mod graph_expansion_tests {
    use crate::get_context::pipeline::{GraphExpansion, NeighborDirection, Pivot, expand_graph};
    use crate::tests::get_context_tests::snapshot_fixture;
    use julie_extractors::RelationshipKind;
    use julie_index::graph::Graph;
    use julie_index::search::index::SymbolSearchResult;

    fn make_pivot(graph: &Graph, name: &str, score: f32) -> Pivot {
        let id = graph.find_by_name(name)[0];
        let row = graph.symbol(id);
        Pivot {
            result: SymbolSearchResult {
                id: row.id.clone(),
                name: row.name.clone(),
                signature: row.signature.clone().unwrap_or_default(),
                doc_comment: String::new(),
                file_path: row.path.clone(),
                kind: row.kind.to_string(),
                language: row.language.clone(),
                start_line: row.span.start_line,
                score,
                role: String::new(),
                test_role: String::new(),
            },
            combined_score: score,
        }
    }

    fn neighbor_names(expansion: &GraphExpansion) -> Vec<&str> {
        expansion
            .neighbors
            .iter()
            .map(|n| n.symbol.name.as_str())
            .collect()
    }

    #[test]
    fn test_expand_graph_basic_incoming_and_outgoing() {
        let (_dir, fixture) = snapshot_fixture(&[
            ("src/main.rs", "fn main() {\n    engine_run();\n}\n"),
            ("src/engine.rs", "fn engine_run() {\n    helper();\n}\n"),
            ("src/utils.rs", "fn helper() {}\n"),
        ]);
        let snapshot = fixture.snapshot();
        let graph = snapshot.graph();

        let pivots = vec![make_pivot(graph, "engine_run", 10.0)];
        let expansion = expand_graph(&pivots, graph);

        assert_eq!(expansion.neighbors.len(), 2);
        let names = neighbor_names(&expansion);
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
        let (_dir, fixture) = snapshot_fixture(&[
            ("src/engine.rs", "fn engine_run() {\n    helper();\n}\n"),
            ("src/handler.rs", "fn handle() {\n    helper();\n}\n"),
            ("src/utils.rs", "fn helper() {}\n"),
        ]);
        let snapshot = fixture.snapshot();
        let graph = snapshot.graph();

        let pivots = vec![
            make_pivot(graph, "engine_run", 10.0),
            make_pivot(graph, "handle", 8.0),
        ];
        let expansion = expand_graph(&pivots, graph);

        assert_eq!(expansion.neighbors.len(), 1);
        assert_eq!(expansion.neighbors[0].symbol.name, "helper");
    }

    #[test]
    fn test_expand_graph_ranks_by_reference_score() {
        let (_dir, fixture) = snapshot_fixture(&[
            ("src/main.rs", "fn main() {\n    engine_run();\n}\n"),
            ("src/engine.rs", "fn engine_run() {\n    main();\n}\n"),
            (
                "src/utils.rs",
                "fn helper() {\n    engine_run();\n    main();\n}\n",
            ),
        ]);
        let snapshot = fixture.snapshot();
        let graph = snapshot.graph();

        let pivots = vec![make_pivot(graph, "engine_run", 10.0)];
        let expansion = expand_graph(&pivots, graph);

        assert_eq!(expansion.neighbors.len(), 2);
        assert_eq!(expansion.neighbors[0].symbol.name, "main");
        assert!(expansion.neighbors[0].reference_score > expansion.neighbors[1].reference_score);
    }

    #[test]
    fn test_expand_graph_no_relationships() {
        let (_dir, fixture) = snapshot_fixture(&[("src/main.rs", "fn lonely_fn() {}\n")]);
        let snapshot = fixture.snapshot();
        let graph = snapshot.graph();

        let pivots = vec![make_pivot(graph, "lonely_fn", 5.0)];
        let expansion = expand_graph(&pivots, graph);
        assert!(expansion.neighbors.is_empty());
    }

    #[test]
    fn test_expand_graph_orders_equal_scores_by_location_and_name() {
        let (_dir, fixture) = snapshot_fixture(&[
            ("src/main.rs", "fn target_fn() {}\n"),
            ("src/utils.rs", "fn zeta_handler() {\n    target_fn();\n}\n"),
            (
                "src/handler.rs",
                "fn alpha_handler() {\n    target_fn();\n}\n",
            ),
            (
                "src/engine.rs",
                "fn engine_handler() {\n    target_fn();\n}\n",
            ),
        ]);
        let snapshot = fixture.snapshot();
        let graph = snapshot.graph();

        let pivots = vec![make_pivot(graph, "target_fn", 5.0)];
        let expansion = expand_graph(&pivots, graph);

        assert_eq!(
            neighbor_names(&expansion),
            vec!["engine_handler", "alpha_handler", "zeta_handler"],
            "equal reference scores should use stable file/line/name ordering"
        );
    }

    #[test]
    fn test_expand_graph_excludes_pivots_from_neighbors() {
        let (_dir, fixture) = snapshot_fixture(&[
            (
                "src/engine.rs",
                "fn engine_run() {\n    handle();\n    helper();\n}\n",
            ),
            ("src/handler.rs", "fn handle() {\n    engine_run();\n}\n"),
            ("src/utils.rs", "fn helper() {}\n"),
        ]);
        let snapshot = fixture.snapshot();
        let graph = snapshot.graph();

        let pivots = vec![
            make_pivot(graph, "engine_run", 10.0),
            make_pivot(graph, "handle", 8.0),
        ];
        let expansion = expand_graph(&pivots, graph);

        assert_eq!(expansion.neighbors.len(), 1);
        assert_eq!(expansion.neighbors[0].symbol.name, "helper");

        let neighbor_ids: Vec<&str> = expansion
            .neighbors
            .iter()
            .map(|n| n.symbol.id.as_str())
            .collect();
        for pivot in &pivots {
            assert!(!neighbor_ids.contains(&pivot.result.id.as_str()));
        }
    }

    #[test]
    fn test_expand_graph_empty_pivots() {
        let (_dir, fixture) = snapshot_fixture(&[("src/main.rs", "fn lonely_fn() {}\n")]);
        let snapshot = fixture.snapshot();
        let expansion = expand_graph(&[], snapshot.graph());
        assert!(expansion.neighbors.is_empty());
    }

    #[test]
    fn test_expand_graph_finds_neighbors_via_identifiers_when_no_relationships() {
        let (_dir, fixture) = snapshot_fixture(&[
            ("src/main.rs", "pub struct ZodInterface {}\n"),
            (
                "src/handler.rs",
                "fn validate_input(input: ZodInterface) -> bool {\n    true\n}\n",
            ),
            ("src/utils.rs", "fn process_data(input: ZodInterface) {}\n"),
        ]);
        let snapshot = fixture.snapshot();
        let graph = snapshot.graph();

        let pivots = vec![make_pivot(graph, "ZodInterface", 9.0)];
        let expansion = expand_graph(&pivots, graph);

        assert_eq!(
            expansion.neighbors.len(),
            2,
            "Expected 2 identifier-based neighbors; got {:?}",
            neighbor_names(&expansion)
        );
        let names = neighbor_names(&expansion);
        assert!(
            names.contains(&"validate_input"),
            "validate_input should be a neighbor"
        );
        assert!(
            names.contains(&"process_data"),
            "process_data should be a neighbor"
        );
    }

    #[test]
    fn test_expand_graph_preserves_identifier_call_relationship_kind() {
        let (_dir, fixture) = snapshot_fixture(&[
            ("src/main.rs", "fn BuildPipeline() {}\n"),
            (
                "src/handler.rs",
                "fn setup_handler() {\n    BuildPipeline();\n}\n",
            ),
        ]);
        let snapshot = fixture.snapshot();
        let graph = snapshot.graph();

        let pivots = vec![make_pivot(graph, "BuildPipeline", 9.0)];
        let expansion = expand_graph(&pivots, graph);

        let caller_neighbor = expansion
            .neighbors
            .iter()
            .find(|neighbor| neighbor.symbol.name == "setup_handler")
            .expect("identifier call should surface the caller as a neighbor");
        assert_eq!(
            caller_neighbor.direction,
            NeighborDirection::Incoming,
            "identifier-based callers should remain incoming neighbors"
        );
        assert_eq!(
            caller_neighbor.relationship_kind,
            RelationshipKind::Calls,
            "identifier kind=call should preserve call semantics for get_context ranking"
        );
    }
}
