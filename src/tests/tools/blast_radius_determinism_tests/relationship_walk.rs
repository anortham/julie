use super::*;

#[tokio::test]
async fn test_walk_impacts_traverses_extends_relationships() -> Result<()> {
    let (_tree, context) = snapshot_context_from_files(&[
        ("src/base.ts", "export class BaseService {}\n"),
        (
            "src/derived.ts",
            "import { BaseService } from \"./base\";\nexport class DerivedService extends BaseService {}\n",
        ),
    ])?;
    let snapshot = snapshot_of(&context).await?;
    let graph = snapshot.graph();

    let impacts = walk_impacts(graph, &[definition(graph, "BaseService")], 1);

    let derived = find_impact(&impacts, "DerivedService")
        .expect("DerivedService should be discovered through Extends");
    assert_eq!(derived.relationship_kind, RelationshipKind::Extends);
    assert_eq!(derived.via_symbol_name, "BaseService");
    Ok(())
}

#[test]
fn test_rank_impacts_prioritizes_and_labels_extends_relationships() {
    let extends_candidate = ImpactCandidate {
        symbol: make_symbol(
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
        symbol: make_symbol(
            "factory",
            "serviceFactory",
            "src/factory.ts",
            SymbolKind::Function,
        ),
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
}
