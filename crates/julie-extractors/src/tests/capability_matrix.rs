use crate::language::language_spec;
use crate::registry::{capabilities_for_language, supported_languages};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
struct CapabilityMatrix {
    languages: Vec<CapabilityRow>,
}

#[derive(Debug, Deserialize)]
struct CapabilityRow {
    language: String,
    parser_crate: String,
    extensions: Vec<String>,
    dependency_status: String,
    target_capabilities: CapabilityFlags,
    capabilities: CapabilityFlags,
    fixtures: Vec<FixtureRow>,
    #[serde(default)]
    capability_gaps: Vec<CapabilityGap>,
}

#[derive(Debug, Deserialize)]
struct CapabilityFlags {
    symbols: bool,
    relationships: bool,
    pending_relationships: bool,
    identifiers: bool,
    types: bool,
}

#[derive(Debug, Deserialize)]
struct FixtureRow {
    name: String,
    source: String,
    expected: String,
}

#[derive(Debug, Deserialize)]
struct CapabilityGap {
    capability: String,
    status: String,
    reason: String,
    required_closure: String,
    evidence: String,
}

#[test]
fn capability_matrix_matches_registry_entries() {
    let root = workspace_root();
    let matrix = load_matrix(&root);
    let matrix_languages: BTreeSet<_> = matrix
        .languages
        .iter()
        .map(|row| row.language.as_str())
        .collect();
    let registry_languages: BTreeSet<_> = supported_languages().into_iter().collect();

    assert_eq!(
        registry_languages, matrix_languages,
        "fixtures/extraction/capabilities.json must have exactly one row per registry entry"
    );

    for row in &matrix.languages {
        assert!(
            !row.parser_crate.trim().is_empty(),
            "{} is missing parser_crate",
            row.language
        );
        assert!(
            !row.extensions.is_empty(),
            "{} is missing extension coverage",
            row.language
        );
        assert!(
            matches!(
                row.dependency_status.as_str(),
                "current" | "upgrade_available" | "git_pinned" | "held"
            ),
            "{} has unsupported dependency_status {}",
            row.language,
            row.dependency_status
        );

        let capabilities = capabilities_for_language(&row.language).unwrap();
        let spec = language_spec(&row.language).unwrap_or_else(|| {
            panic!(
                "{} is present in the registry but missing from language specs",
                row.language
            )
        });
        assert_eq!(
            row.extensions,
            spec.extensions
                .iter()
                .map(|extension| extension.to_string())
                .collect::<Vec<_>>(),
            "{} extensions must come from language specs",
            row.language
        );
        assert_eq!(
            row.parser_crate, spec.parser_crate,
            "{} parser crate must come from language specs",
            row.language
        );
        assert_eq!(
            capabilities.symbols, row.capabilities.symbols,
            "{}",
            row.language
        );
        assert_eq!(
            capabilities.relationships, row.capabilities.relationships,
            "{}",
            row.language
        );
        assert_eq!(
            capabilities.pending_relationships, row.capabilities.pending_relationships,
            "{}",
            row.language
        );
        assert_eq!(
            capabilities.identifiers, row.capabilities.identifiers,
            "{}",
            row.language
        );
        assert_eq!(
            capabilities.types, row.capabilities.types,
            "{}",
            row.language
        );
    }
}

#[test]
fn capability_matrix_has_golden_case_for_every_registry_entry() {
    let root = workspace_root();
    let matrix = load_matrix(&root);

    for row in matrix.languages {
        assert!(
            !row.fixtures.is_empty(),
            "{} must have at least one golden fixture",
            row.language
        );
        for fixture in row.fixtures {
            assert!(
                !fixture.name.trim().is_empty(),
                "{} has an unnamed fixture",
                row.language
            );
            let source = root.join(&fixture.source);
            let expected = root.join(&fixture.expected);
            assert!(
                source.is_file(),
                "{} fixture source does not exist: {}",
                row.language,
                source.display()
            );
            assert!(
                expected.is_file(),
                "{} fixture expected output does not exist: {}",
                row.language,
                expected.display()
            );
        }
    }
}

#[test]
fn capability_matrix_requires_relationship_fixture_evidence() {
    let root = workspace_root();
    let matrix = load_matrix(&root);

    for row in matrix.languages {
        let has_relationship_evidence = row
            .fixtures
            .iter()
            .any(|fixture| fixture_exercises_relationships(&root, fixture));
        let exception = row
            .capability_gaps
            .iter()
            .find(|gap| gap.capability == "relationships" && gap.status == "exception");

        assert!(
            row.capabilities.relationships || exception.is_none(),
            "{} has a relationship fixture exception but does not advertise relationship support",
            row.language
        );

        if has_relationship_evidence {
            assert!(
                exception.is_none(),
                "{} has relationship fixture evidence and no longer needs relationship_fixture_exception",
                row.language
            );
        }

        assert!(
            !row.capabilities.relationships || has_relationship_evidence || exception.is_some(),
            "{} advertises relationship support but no golden fixture exercises relationships, pending_relationships, or structured_pending_relationships",
            row.language
        );
    }
}

#[test]
fn capability_matrix_requires_target_capabilities() {
    let root = workspace_root();
    let matrix = load_matrix(&root);

    for row in matrix.languages {
        validate_target_capability(&row, "symbols", row.target_capabilities.symbols);
        validate_target_capability(&row, "relationships", row.target_capabilities.relationships);
        validate_target_capability(
            &row,
            "pending_relationships",
            row.target_capabilities.pending_relationships,
        );
        validate_target_capability(&row, "identifiers", row.target_capabilities.identifiers);
        validate_target_capability(&row, "types", row.target_capabilities.types);

        for gap in &row.capability_gaps {
            assert!(
                matches!(
                    gap.capability.as_str(),
                    "symbols" | "relationships" | "pending_relationships" | "identifiers" | "types"
                ),
                "{} has an unknown capability gap: {}",
                row.language,
                gap.capability
            );
            assert!(
                matches!(gap.status.as_str(), "open" | "exception"),
                "{} has unsupported gap status {} for {}",
                row.language,
                gap.status,
                gap.capability
            );
            assert!(
                !gap.reason.trim().is_empty(),
                "{} {} gap is missing a reason",
                row.language,
                gap.capability
            );
            assert!(
                !gap.required_closure.trim().is_empty(),
                "{} {} gap is missing required closure text",
                row.language,
                gap.capability
            );
            assert!(
                root.join(&gap.evidence).exists(),
                "{} {} gap evidence path does not exist: {}",
                row.language,
                gap.capability,
                gap.evidence
            );
        }
    }
}

#[test]
fn regex_capabilities_advertise_golden_relationships() {
    let capabilities = capabilities_for_language("regex").unwrap();

    assert!(
        capabilities.relationships,
        "regex has golden-tested named and numeric backreference relationship extraction"
    );
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("julie-extractors crate should live under crates/")
        .to_path_buf()
}

fn load_matrix(root: &Path) -> CapabilityMatrix {
    let matrix_path = root.join("fixtures/extraction/capabilities.json");
    let json = fs::read_to_string(&matrix_path).unwrap_or_else(|err| {
        panic!(
            "failed to read capability matrix at {}: {}",
            matrix_path.display(),
            err
        )
    });
    serde_json::from_str(&json).unwrap_or_else(|err| {
        panic!(
            "failed to parse capability matrix at {}: {}",
            matrix_path.display(),
            err
        )
    })
}

fn validate_target_capability(row: &CapabilityRow, capability: &str, target_enabled: bool) {
    let implemented = implemented_capability(row, capability);
    let gap = row
        .capability_gaps
        .iter()
        .find(|gap| gap.capability == capability);

    if !target_enabled {
        assert!(
            !implemented,
            "{} implements {} even though the target marks it non-applicable",
            row.language, capability
        );
        return;
    }

    if implemented {
        assert!(
            gap.is_none(),
            "{} implements target capability {} but still records a gap",
            row.language,
            capability
        );
    } else {
        assert!(
            gap.is_some(),
            "{} target capability {} is true but implementation is false and no gap is recorded",
            row.language,
            capability
        );
    }
}

fn implemented_capability(row: &CapabilityRow, capability: &str) -> bool {
    match capability {
        "symbols" => row.capabilities.symbols,
        "relationships" => row.capabilities.relationships,
        "pending_relationships" => row.capabilities.pending_relationships,
        "identifiers" => row.capabilities.identifiers,
        "types" => row.capabilities.types,
        other => panic!("unknown capability {other}"),
    }
}

fn fixture_exercises_relationships(root: &Path, fixture: &FixtureRow) -> bool {
    let expected_path = root.join(&fixture.expected);
    let json = fs::read_to_string(&expected_path).unwrap_or_else(|err| {
        panic!(
            "failed to read expected fixture at {}: {}",
            expected_path.display(),
            err
        )
    });
    let expected: Value = serde_json::from_str(&json).unwrap_or_else(|err| {
        panic!(
            "failed to parse expected fixture at {}: {}",
            expected_path.display(),
            err
        )
    });

    [
        "relationships",
        "pending_relationships",
        "structured_pending_relationships",
    ]
    .iter()
    .any(|field| {
        expected
            .get(field)
            .and_then(Value::as_array)
            .is_some_and(|items| !items.is_empty())
    })
}
