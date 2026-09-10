use super::*;

#[tokio::test]
async fn test_walk_impacts_keeps_the_strongest_edge_per_caller() -> Result<()> {
    let (_tree, context) = snapshot_context_from_files(&[
        ("src/target.ts", "export class ImpactTarget {}\n"),
        (
            "src/caller.ts",
            "import { ImpactTarget } from \"./target\";\nexport function identifierCaller(t: ImpactTarget) { return new ImpactTarget(); }\n",
        ),
        (
            "src/reference_caller.ts",
            "import { ImpactTarget } from \"./target\";\nexport function referenceCaller(t: ImpactTarget) { return t; }\n",
        ),
    ])?;
    let snapshot = snapshot_of(&context).await?;
    let graph = snapshot.graph();

    let impacts = walk_impacts(graph, &[definition(graph, "ImpactTarget")], 1);

    assert_eq!(
        find_impact(&impacts, "identifierCaller")
            .expect("identifierCaller should be discovered")
            .relationship_kind,
        RelationshipKind::Calls,
        "a caller with both a call and a type usage keeps the call edge"
    );
    assert_eq!(
        find_impact(&impacts, "referenceCaller")
            .expect("referenceCaller should be discovered")
            .relationship_kind,
        RelationshipKind::References,
        "a type-usage-only caller maps to a References edge"
    );
    assert_eq!(impacts.len(), 2, "each caller appears once: {impacts:?}");
    Ok(())
}

#[tokio::test]
async fn test_blast_radius_limit_bounds_depth_frontier() -> Result<()> {
    let mut files: Vec<(String, String)> = vec![(
        "src/seed.ts".to_string(),
        "export function seedFn() {}\n".to_string(),
    )];
    for i in 0..130 {
        files.push((
            format!("src/first_{i:03}.ts"),
            format!("import {{ seedFn }} from \"./seed\";\nexport function first{i:03}() {{ return seedFn(); }}\n"),
        ));
        files.push((
            format!("src/second_{i:03}.ts"),
            format!("import {{ first{i:03} }} from \"./first_{i:03}\";\nexport function second{i:03}() {{ return first{i:03}(); }}\n"),
        ));
    }
    let borrowed: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, content)| (path.as_str(), content.as_str()))
        .collect();
    let (_tree, context) = snapshot_context_from_files(&borrowed)?;
    let snapshot = snapshot_of(&context).await?;
    let graph = snapshot.graph();

    let (impacts, stats) = walk_impacts_with_budget(
        graph,
        &[definition(graph, "seedFn")],
        2,
        WalkBudget {
            max_frontier_per_depth: 40,
        },
    );

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
    assert_eq!(stats.capped_depths, 1);
    assert_eq!(stats.depths_visited, 2);
    Ok(())
}

#[tokio::test]
async fn test_walk_impacts_resolves_via_symbol_per_seed() -> Result<()> {
    let (_tree, context) = snapshot_context_from_files(&[
        ("src/alpha.ts", "export class AlphaStore {}\n"),
        ("src/beta.ts", "export class BetaStore {}\n"),
        (
            "src/alpha_adapter.ts",
            "import { AlphaStore } from \"./alpha\";\nexport function alphaAdapter(store: AlphaStore) { return store; }\n",
        ),
        (
            "src/beta_adapter.ts",
            "import { BetaStore } from \"./beta\";\nexport function betaAdapter() { return new BetaStore(); }\n",
        ),
    ])?;
    let snapshot = snapshot_of(&context).await?;
    let graph = snapshot.graph();

    let impacts = walk_impacts(
        graph,
        &[
            definition(graph, "AlphaStore"),
            definition(graph, "BetaStore"),
        ],
        1,
    );

    let beta_adapter =
        find_impact(&impacts, "betaAdapter").expect("beta_adapter should be discovered");
    assert_eq!(
        beta_adapter.relationship_kind,
        RelationshipKind::Calls,
        "identifier kind=call should rank as a direct caller, not a generic reference"
    );
    assert_eq!(
        beta_adapter.via_symbol_name, "BetaStore",
        "identifier-derived impacts should use the resolved target symbol, not the first seed"
    );

    let alpha_adapter =
        find_impact(&impacts, "alphaAdapter").expect("alpha_adapter should be discovered");
    assert_eq!(
        alpha_adapter.relationship_kind,
        RelationshipKind::References,
        "identifier kind=type_usage should map to a References edge"
    );
    assert_eq!(
        alpha_adapter.via_symbol_name, "AlphaStore",
        "multi-seed identifier walks should resolve each target via target_symbol_id, not fall back to frontier-first"
    );
    Ok(())
}
