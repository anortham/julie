use crate::pipeline::detect_language_for_path;
use crate::registry::supported_languages;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
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
    dependency_status: String,
    fixtures: Vec<FixtureRow>,
}

#[derive(Debug, Deserialize)]
struct FixtureRow {
    source: String,
    expected: String,
}

#[test]
fn parser_upgrade_gate_covers_full_language_inventory() {
    let root = workspace_root();
    let matrix = load_matrix(&root);
    let registry_languages: BTreeSet<_> = supported_languages().into_iter().collect();
    let mut parser_crates = BTreeMap::<String, usize>::new();
    let mut fixture_count = 0usize;

    for row in &matrix.languages {
        assert!(
            registry_languages.contains(row.language.as_str()),
            "matrix contains non-registry language {}",
            row.language
        );
        assert!(
            matches!(
                row.dependency_status.as_str(),
                "current" | "upgrade_available" | "git_pinned" | "held"
            ),
            "{} has invalid dependency status {}",
            row.language,
            row.dependency_status
        );
        *parser_crates.entry(row.parser_crate.clone()).or_default() += 1;

        for fixture in &row.fixtures {
            fixture_count += 1;
            assert!(
                root.join(&fixture.expected).is_file(),
                "{} expected output is missing: {}",
                row.language,
                fixture.expected
            );
            let detected = detect_language_for_path(&fixture.source).unwrap_or_else(|err| {
                panic!(
                    "{} fixture should route through language detection: {}",
                    row.language, err
                )
            });
            assert_eq!(
                detected, row.language,
                "{} fixture must exercise its own parser entry",
                row.language
            );
        }
    }

    assert_eq!(
        matrix.languages.len(),
        registry_languages.len(),
        "parser upgrade gate must cover every registry entry"
    );
    assert!(
        fixture_count >= registry_languages.len(),
        "parser upgrade gate needs at least one fixture per registry entry"
    );
    assert!(
        parser_crates.len() >= 30,
        "parser upgrade gate should cover the full parser inventory, not a small subset"
    );
}

#[test]
fn parser_upgrade_gate_keeps_variants_visible() {
    let root = workspace_root();
    let matrix = load_matrix(&root);
    let languages: BTreeSet<_> = matrix
        .languages
        .iter()
        .map(|row| row.language.as_str())
        .collect();

    for variant in ["tsx", "jsx", "vue"] {
        assert!(
            languages.contains(variant),
            "variant {} must have its own parser-upgrade row",
            variant
        );
    }
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
