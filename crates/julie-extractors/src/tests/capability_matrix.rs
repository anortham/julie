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
    capabilities: CapabilityFlags,
    fixtures: Vec<FixtureRow>,
    #[serde(default)]
    relationship_fixture_exception: Option<String>,
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
            .relationship_fixture_exception
            .as_deref()
            .map(str::trim)
            .filter(|reason| !reason.is_empty());

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
fn regex_capabilities_do_not_advertise_stubbed_relationships() {
    let capabilities = capabilities_for_language("regex").unwrap();

    assert!(
        !capabilities.relationships,
        "regex extract_relationships currently returns no relationships; keep the capability false until a golden fixture proves relationship extraction"
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
