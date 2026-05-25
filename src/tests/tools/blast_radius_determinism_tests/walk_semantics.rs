use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn test_walk_impacts_identifier_edges_choose_strongest_kind_without_replacing_relationships()
-> Result<()> {
    let (_temp_dir, handler, workspace_id) = setup_handler().await?;

    let files = vec![
        make_file("src/target.ts", "hash_target"),
        make_file("src/identifier_caller.ts", "hash_identifier_caller"),
        make_file("src/relationship_caller.ts", "hash_relationship_caller"),
    ];
    let symbols = vec![
        make_symbol("target", "ImpactTarget", "src/target.ts", None),
        make_symbol(
            "identifier_caller",
            "identifierCaller",
            "src/identifier_caller.ts",
            None,
        ),
        make_symbol(
            "relationship_caller",
            "relationshipCaller",
            "src/relationship_caller.ts",
            None,
        ),
    ];
    let relationships = vec![make_relationship(
        "relationship_reference",
        "relationship_caller",
        "target",
        RelationshipKind::References,
        "src/relationship_caller.ts",
    )];
    let identifiers = vec![
        make_identifier(
            "identifier_import",
            "ImpactTarget",
            "src/identifier_caller.ts",
            Some("identifier_caller"),
            Some("target"),
            IdentifierKind::VariableRef,
            4,
            0.70,
        ),
        make_identifier(
            "identifier_type_usage",
            "ImpactTarget",
            "src/identifier_caller.ts",
            Some("identifier_caller"),
            Some("target"),
            IdentifierKind::TypeUsage,
            5,
            0.95,
        ),
        make_identifier(
            "relationship_identifier_call",
            "ImpactTarget",
            "src/relationship_caller.ts",
            Some("relationship_caller"),
            Some("target"),
            IdentifierKind::Call,
            6,
            0.95,
        ),
    ];

    let db = handler.primary_database().await?;
    {
        let mut guard = db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.bulk_store_fresh_atomic(
            &files,
            &symbols,
            &relationships,
            &identifiers,
            &[],
            workspace_id.as_str(),
        )?;
        guard.conn.execute(
            "UPDATE identifiers SET kind = 'import' WHERE id = 'identifier_import'",
            [],
        )?;
        guard.compute_reference_scores()?;

        let seed_symbols = guard.get_symbols_by_ids(&["target".to_string()])?;
        let impacts = walk_impacts(&guard, &seed_symbols, 1)?;

        let identifier_caller = impacts
            .iter()
            .find(|candidate| candidate.symbol.id == "identifier_caller")
            .expect("identifierCaller should be discovered through identifiers");
        assert_eq!(
            identifier_caller.relationship_kind,
            RelationshipKind::References,
            "identifier fallback should prefer type usage over import for the same container"
        );

        let relationship_caller = impacts
            .iter()
            .find(|candidate| candidate.symbol.id == "relationship_caller")
            .expect("relationshipCaller should be discovered through relationships");
        assert_eq!(
            relationship_caller.relationship_kind,
            RelationshipKind::References,
            "relationship table edges must outrank identifier fallback edges"
        );
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_walk_impacts_caps_identifier_fanout_for_common_names() -> Result<()> {
    let (_temp_dir, handler, workspace_id) = setup_handler().await?;

    let mut files = vec![make_file("src/seed.ts", "hash_seed")];
    let mut symbols = vec![make_symbol("seed", "new", "src/seed.ts", None)];
    let mut relationships = Vec::new();
    let mut identifiers = Vec::new();

    for i in 0..12 {
        let caller_id = format!("caller_{i:02}");
        let file_path = format!("src/caller_{i:02}.ts");
        files.push(make_file(&file_path, &format!("hash_{i:02}")));
        symbols.push(make_symbol(
            &caller_id,
            &format!("caller{i:02}"),
            &file_path,
            None,
        ));
        identifiers.push(make_identifier(
            &format!("ident_{i:02}"),
            "new",
            &file_path,
            Some(&caller_id),
            Some("seed"),
            IdentifierKind::Call,
            (i + 1) as u32,
            0.80,
        ));
        if i < 5 {
            relationships.push(make_relationship(
                &format!("rel_{i:02}"),
                &caller_id,
                "seed",
                RelationshipKind::Calls,
                &file_path,
            ));
        }
    }

    let db = handler.primary_database().await?;
    {
        let mut guard = db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.bulk_store_fresh_atomic(
            &files,
            &symbols,
            &relationships,
            &identifiers,
            &[],
            workspace_id.as_str(),
        )?;
        guard.compute_reference_scores()?;

        let seed_symbols = guard.get_symbols_by_ids(&["seed".to_string()])?;
        let (impacts, _stats) = walk_impacts_with_budget(
            &guard,
            &seed_symbols,
            1,
            WalkBudget {
                max_frontier_per_depth: 128,
                max_identifier_fanout_per_name: 5,
            },
        )?;

        assert_eq!(
            impacts.len(),
            10,
            "relationship-backed callers should not consume the identifier fallback fanout budget"
        );
        for i in 0..5 {
            let relationship_id = format!("caller_{i:02}");
            assert!(
                impacts
                    .iter()
                    .any(|candidate| candidate.symbol.id == relationship_id),
                "relationship caller {relationship_id} should be retained"
            );
        }
        for i in 5..10 {
            let identifier_id = format!("caller_{i:02}");
            assert!(
                impacts
                    .iter()
                    .any(|candidate| candidate.symbol.id == identifier_id),
                "identifier-only caller {identifier_id} should be retained within the fallback cap"
            );
        }
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_blast_radius_limit_bounds_depth_frontier() -> Result<()> {
    let (_temp_dir, handler, workspace_id) = setup_handler().await?;

    let mut files = vec![make_file("src/seed.ts", "hash_seed")];
    let mut symbols = vec![make_symbol("seed", "seedFn", "src/seed.ts", None)];
    let mut relationships = Vec::new();

    for i in 0..130 {
        let first_id = format!("first_{i:03}");
        let second_id = format!("second_{i:03}");
        let first_path = format!("src/first_{i:03}.ts");
        let second_path = format!("src/second_{i:03}.ts");

        files.push(make_file(&first_path, &format!("hash_first_{i:03}")));
        files.push(make_file(&second_path, &format!("hash_second_{i:03}")));
        symbols.push(make_symbol(
            &first_id,
            &format!("first{i:03}"),
            &first_path,
            None,
        ));
        symbols.push(make_symbol(
            &second_id,
            &format!("second{i:03}"),
            &second_path,
            None,
        ));
        relationships.push(make_relationship(
            &format!("rel_first_{i:03}"),
            &first_id,
            "seed",
            RelationshipKind::Calls,
            &first_path,
        ));
        relationships.push(make_relationship(
            &format!("rel_second_{i:03}"),
            &second_id,
            &first_id,
            RelationshipKind::Calls,
            &second_path,
        ));
    }

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

        let seed_symbols = guard.get_symbols_by_ids(&["seed".to_string()])?;
        let (impacts, _stats) = walk_impacts_with_budget(
            &guard,
            &seed_symbols,
            2,
            WalkBudget {
                max_frontier_per_depth: 40,
                max_identifier_fanout_per_name: 500,
            },
        )?;

        let depth_one = impacts
            .iter()
            .filter(|candidate| candidate.distance == 1)
            .count();
        let depth_two = impacts
            .iter()
            .filter(|candidate| candidate.distance == 2)
            .count();

        assert_eq!(
            depth_one, 40,
            "depth-1 frontier should be clipped to the configured limit"
        );
        assert_eq!(
            depth_two, 40,
            "depth-2 frontier should stay bounded by the same limit"
        );
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_walk_impacts_preserves_identifier_call_kind_and_resolved_target() -> Result<()> {
    let (_temp_dir, handler, workspace_id) = setup_handler().await?;

    let files = vec![
        make_file("src/alpha.ts", "hash_alpha"),
        make_file("src/beta.ts", "hash_beta"),
        make_file("src/alpha_adapter.ts", "hash_alpha_adapter"),
        make_file("src/beta_adapter.ts", "hash_beta_adapter"),
    ];
    let symbols = vec![
        make_symbol("seed_alpha", "AlphaStore", "src/alpha.ts", None),
        make_symbol("seed_beta", "BetaStore", "src/beta.ts", None),
        make_symbol(
            "alpha_adapter",
            "alphaAdapter",
            "src/alpha_adapter.ts",
            None,
        ),
        make_symbol("beta_adapter", "betaAdapter", "src/beta_adapter.ts", None),
    ];
    let identifiers = vec![
        make_identifier(
            "alpha_ident",
            "AlphaStore",
            "src/alpha_adapter.ts",
            Some("alpha_adapter"),
            Some("seed_alpha"),
            IdentifierKind::TypeUsage,
            4,
            0.90,
        ),
        make_identifier(
            "beta_ident",
            "BetaStore",
            "src/beta_adapter.ts",
            Some("beta_adapter"),
            Some("seed_beta"),
            IdentifierKind::Call,
            7,
            0.95,
        ),
    ];

    let db = handler.primary_database().await?;
    {
        let mut guard = db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.bulk_store_fresh_atomic(
            &files,
            &symbols,
            &Vec::<Relationship>::new(),
            &identifiers,
            &[],
            workspace_id.as_str(),
        )?;
        guard.compute_reference_scores()?;

        let seed_symbols =
            guard.get_symbols_by_ids(&["seed_alpha".to_string(), "seed_beta".to_string()])?;
        let impacts = walk_impacts(&guard, &seed_symbols, 1)?;

        let beta_adapter = impacts
            .iter()
            .find(|candidate| candidate.symbol.id == "beta_adapter")
            .expect("beta_adapter should be discovered via identifiers");
        assert_eq!(
            beta_adapter.relationship_kind,
            crate::extractors::RelationshipKind::Calls,
            "identifier kind=call should rank as a direct caller, not a generic reference"
        );
        assert_eq!(
            beta_adapter.via_symbol_name, "BetaStore",
            "identifier-derived impacts should use the resolved target symbol, not the first seed"
        );

        let alpha_adapter = impacts
            .iter()
            .find(|candidate| candidate.symbol.id == "alpha_adapter")
            .expect("alpha_adapter should be discovered via identifiers");
        assert_eq!(
            alpha_adapter.relationship_kind,
            crate::extractors::RelationshipKind::References,
            "identifier kind=type_usage should map to a References edge"
        );
        assert_eq!(
            alpha_adapter.via_symbol_name, "AlphaStore",
            "multi-seed identifier walks should resolve each target via target_symbol_id, not fall back to frontier-first"
        );
    }

    Ok(())
}
