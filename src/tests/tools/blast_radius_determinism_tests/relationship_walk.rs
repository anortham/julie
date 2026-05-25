use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn test_walk_impacts_traverses_extends_relationships() -> Result<()> {
    let (_temp_dir, handler, workspace_id) = setup_handler().await?;

    let files = vec![
        make_file("src/base.ts", "hash_base"),
        make_file("src/derived.ts", "hash_derived"),
    ];
    let symbols = vec![
        make_symbol_with_kind("base", "BaseService", "src/base.ts", SymbolKind::Class),
        make_symbol_with_kind(
            "derived",
            "DerivedService",
            "src/derived.ts",
            SymbolKind::Class,
        ),
    ];
    let relationships = vec![make_relationship(
        "derived_extends_base",
        "derived",
        "base",
        RelationshipKind::Extends,
        "src/derived.ts",
    )];

    let db = handler.primary_database().await?;
    {
        let mut guard = db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.bulk_store_fresh_atomic(
            &files,
            &symbols,
            &relationships,
            &Vec::<Identifier>::new(),
            &[],
            workspace_id.as_str(),
        )?;
        guard.compute_reference_scores()?;

        let seed_symbols = guard.get_symbols_by_ids(&["base".to_string()])?;
        let impacts = walk_impacts(&guard, &seed_symbols, 1)?;

        let derived = impacts
            .iter()
            .find(|candidate| candidate.symbol.id == "derived")
            .expect("DerivedService should be discovered through Extends");
        assert_eq!(derived.relationship_kind, RelationshipKind::Extends);
        assert_eq!(derived.via_symbol_name, "BaseService");
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_rank_impacts_prioritizes_and_labels_extends_relationships() -> Result<()> {
    let extends_candidate = ImpactCandidate {
        symbol: make_symbol_with_kind(
            "derived",
            "DerivedService",
            "src/derived.ts",
            SymbolKind::Class,
        ),
        distance: 1,
        relationship_kind: RelationshipKind::Extends,
        reference_score: 0.0,
        via_symbol_name: "BaseService".to_string(),
    };
    let instantiates_candidate = ImpactCandidate {
        symbol: make_symbol("factory", "serviceFactory", "src/factory.ts", None),
        distance: 1,
        relationship_kind: RelationshipKind::Instantiates,
        reference_score: 100.0,
        via_symbol_name: "BaseService".to_string(),
    };

    let ranked = rank_impacts(vec![instantiates_candidate, extends_candidate], true);

    assert_eq!(
        ranked[0].relationship_kind,
        RelationshipKind::Extends,
        "Extends should rank near Implements instead of falling behind constructor paths"
    );
    assert!(
        ranked[0].why.contains("subclass, 1 hop"),
        "Extends should render a meaningful relationship label: {:?}",
        ranked[0].why
    );

    Ok(())
}
