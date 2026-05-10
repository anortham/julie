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
    /// Typed evidence — see `EvidenceRef`. The resolver test in
    /// `capability_matrix_evidence_resolves` (Task 1.2) verifies the referenced
    /// artifact actually exists; this struct only carries the shape.
    evidence: EvidenceRef,
    /// Names the Phase task that will close this row. Required for
    /// `status: "open"`; forbidden for `closed`/`exception`. Validated by
    /// `capability_matrix_open_rows_have_planned_closure_task` (Task 1.2).
    #[serde(default)]
    planned_closure_task: Option<String>,
}

/// Typed evidence reference. Every `capability_gap.evidence` cell must
/// deserialize as `Test`, `Fixture`, or `Commit`. The legacy bare-string form
/// parses to `DeadString`, which the
/// `capability_matrix_evidence_is_typed_object` test rejects.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum EvidenceRef {
    Test {
        #[allow(dead_code)]
        kind: TestKind,
        value: String,
        command: String,
    },
    Fixture {
        #[allow(dead_code)]
        kind: FixtureKind,
        value: String,
        #[allow(dead_code)]
        command: String,
    },
    Commit {
        #[allow(dead_code)]
        kind: CommitKind,
        value: String,
        #[allow(dead_code)]
        command: String,
    },
    /// Legacy bare-string evidence. Rejected by
    /// `capability_matrix_evidence_is_typed_object`.
    DeadString(String),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum TestKind {
    Test,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum FixtureKind {
    Fixture,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum CommitKind {
    Commit,
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
        let has_relationship_evidence = row.fixtures.iter().any(|fixture| {
            assert_fixture_pending_parity(&root, fixture, &row.language);
            fixture_exercises_relationships(&root, fixture)
        });
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
            // Typed-evidence shape is enforced by
            // `capability_matrix_evidence_is_typed_object`; resolution is
            // enforced by `capability_matrix_evidence_resolves` (Task 1.2).
            // No path-existence check here — bare strings are gone.
        }
    }
}

/// Task 1.1: every `capability_gaps[].evidence` cell deserializes as a typed
/// object (`{kind, value, command}`), not the legacy bare-string form. This is
/// the shape contract; resolution is enforced by
/// `capability_matrix_evidence_resolves` in Task 1.2.
#[test]
fn capability_matrix_evidence_is_typed_object() {
    let root = workspace_root();
    let matrix = load_matrix(&root);
    let mut errors = Vec::new();
    for row in &matrix.languages {
        for gap in &row.capability_gaps {
            match &gap.evidence {
                EvidenceRef::DeadString(s) => errors.push(format!(
                    "language {} gap {} still has bare-string evidence `{}` — \
                     migrate to typed object {{kind, value, command}}",
                    row.language, gap.capability, s
                )),
                EvidenceRef::Test { value, command, .. } => {
                    if value.is_empty() {
                        errors.push(format!(
                            "language {} gap {} test evidence has empty value",
                            row.language, gap.capability
                        ));
                    }
                    if !command.starts_with("cargo nextest") {
                        errors.push(format!(
                            "language {} gap {} test evidence command must start with \
                             `cargo nextest`, got `{}`",
                            row.language, gap.capability, command
                        ));
                    }
                }
                EvidenceRef::Fixture { value, .. } => {
                    if value.is_empty() {
                        errors.push(format!(
                            "language {} gap {} fixture evidence has empty value",
                            row.language, gap.capability
                        ));
                    }
                }
                EvidenceRef::Commit { value, .. } => {
                    if value.len() != 40 || !value.chars().all(|c| c.is_ascii_hexdigit()) {
                        errors.push(format!(
                            "language {} gap {} commit evidence must be a 40-char hex SHA, \
                             got `{}`",
                            row.language, gap.capability, value
                        ));
                    }
                }
            }
        }
    }
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}

#[test]
fn capability_matrix_records_known_gaps_for_languages_with_unfixed_findings() {
    let root = workspace_root();
    let matrix = load_matrix(&root);
    let gap_count = matrix
        .languages
        .iter()
        .map(|row| row.capability_gaps.len())
        .sum::<usize>();

    assert!(
        gap_count > 0,
        "capabilities.json must record explicit capability_gaps while audit findings remain open"
    );
}

#[test]
fn capability_matrix_pending_claim_requires_pending_output_in_fixtures() {
    let root = workspace_root();
    let matrix = load_matrix(&root);

    for row in matrix.languages {
        if !row.capabilities.pending_relationships {
            continue;
        }

        let has_pending_evidence = row
            .fixtures
            .iter()
            .any(|fixture| fixture_exercises_pending_relationships(&root, fixture));
        let has_pending_gap = row
            .capability_gaps
            .iter()
            .any(|gap| gap.capability == "pending_relationships");

        assert!(
            has_pending_evidence || has_pending_gap,
            "{} advertises pending relationship support but no golden fixture emits pending_relationships or structured_pending_relationships and no gap is recorded",
            row.language
        );
    }
}

// Plan-doc stronger versions: per-language coverage instead of global totals.

#[test]
fn test_capability_matrix_records_known_gaps_for_languages_with_unfixed_findings() {
    // For every language that sets a capability to false in target_capabilities,
    // capability_gaps must have a matching entry (gap.capability == the false capability).
    // Silently lowering target_capabilities without a documented gap is dishonest.
    let root = workspace_root();
    let matrix = load_matrix(&root);

    let cap_names = [
        "symbols",
        "relationships",
        "pending_relationships",
        "identifiers",
        "types",
    ];

    for row in &matrix.languages {
        for &cap in &cap_names {
            let target_enabled = match cap {
                "symbols" => row.target_capabilities.symbols,
                "relationships" => row.target_capabilities.relationships,
                "pending_relationships" => row.target_capabilities.pending_relationships,
                "identifiers" => row.target_capabilities.identifiers,
                "types" => row.target_capabilities.types,
                _ => unreachable!(),
            };

            if !target_enabled {
                let has_gap = row.capability_gaps.iter().any(|gap| gap.capability == cap);
                assert!(
                    has_gap,
                    "{} sets target_capabilities.{} = false but has no matching capability_gaps \
                     entry. Add a gap with capability=\"{}\" documenting why this capability is \
                     not targeted.",
                    row.language, cap, cap
                );
            }
        }
    }
}

#[test]
fn test_capability_matrix_pending_claim_requires_pending_output_in_fixtures() {
    // For every language that targets pending_relationships (target_capabilities.pending_relationships
    // == true), either a golden fixture must emit at least one pending or structured_pending entry,
    // or a capability_gaps entry for "pending_relationships" must explain the shortfall.
    // This is stricter than the existing check, which tests capabilities (current state) instead
    // of target_capabilities (intended state).
    let root = workspace_root();
    let matrix = load_matrix(&root);

    for row in &matrix.languages {
        if !row.target_capabilities.pending_relationships {
            continue;
        }

        let has_pending_evidence = row
            .fixtures
            .iter()
            .any(|fixture| fixture_exercises_pending_relationships(&root, fixture));
        let has_pending_gap = row
            .capability_gaps
            .iter()
            .any(|gap| gap.capability == "pending_relationships");

        assert!(
            has_pending_evidence || has_pending_gap,
            "{} sets target_capabilities.pending_relationships = true but no golden fixture emits \
             pending_relationships or structured_pending_relationships and no capability_gaps entry \
             for pending_relationships is recorded.",
            row.language
        );
    }
}

#[test]
fn capability_matrix_sql_relationship_gap_closes_with_view_and_trigger_evidence() {
    let root = workspace_root();
    let matrix = load_matrix(&root);
    let sql = matrix
        .languages
        .iter()
        .find(|row| row.language == "sql")
        .expect("SQL capability row should exist");

    let relationship_types = sql
        .fixtures
        .iter()
        .flat_map(|fixture| {
            let expected = load_expected_fixture(&root, fixture);
            expected
                .get("relationships")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        })
        .filter_map(|relationship| {
            relationship
                .get("metadata")
                .and_then(|metadata| metadata.get("relationshipType"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect::<BTreeSet<_>>();

    assert!(
        relationship_types.contains("view_source") && relationship_types.contains("trigger_target"),
        "SQL golden fixtures must prove view_source and trigger_target relationships before closing TS-RF-006"
    );

    assert!(
        !sql.capability_gaps
            .iter()
            .any(|gap| gap.capability == "relationships"),
        "SQL view/trigger relationship evidence is present, so the relationships capability gap is stale"
    );
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

    if !implemented {
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
    let expected = load_expected_fixture(root, fixture);

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

fn fixture_exercises_pending_relationships(root: &Path, fixture: &FixtureRow) -> bool {
    let expected = load_expected_fixture(root, fixture);

    ["pending_relationships", "structured_pending_relationships"]
        .iter()
        .any(|field| {
            expected
                .get(field)
                .and_then(Value::as_array)
                .is_some_and(|items| !items.is_empty())
        })
}

fn assert_fixture_pending_parity(root: &Path, fixture: &FixtureRow, language: &str) {
    let expected = load_expected_fixture(root, fixture);
    let pending = expected
        .get("pending_relationships")
        .and_then(Value::as_array)
        .unwrap_or_else(|| {
            panic!(
                "{} fixture {} is missing pending_relationships",
                language, fixture.name
            )
        });
    let structured_pending = expected
        .get("structured_pending_relationships")
        .and_then(Value::as_array)
        .unwrap_or_else(|| {
            panic!(
                "{} fixture {} is missing structured_pending_relationships",
                language, fixture.name
            )
        });
    let degraded = structured_pending
        .iter()
        .map(|item| {
            item.get("pending").cloned().unwrap_or_else(|| {
                panic!(
                    "{} fixture {} has structured pending without pending payload",
                    language, fixture.name
                )
            })
        })
        .collect::<Vec<_>>();

    assert_eq!(
        pending,
        degraded.as_slice(),
        "{} fixture {} must keep pending_relationships aligned with degraded structured_pending_relationships",
        language,
        fixture.name
    );
}

fn load_expected_fixture(root: &Path, fixture: &FixtureRow) -> Value {
    let expected_path = root.join(&fixture.expected);
    let json = fs::read_to_string(&expected_path).unwrap_or_else(|err| {
        panic!(
            "failed to read expected fixture at {}: {}",
            expected_path.display(),
            err
        )
    });
    serde_json::from_str(&json).unwrap_or_else(|err| {
        panic!(
            "failed to parse expected fixture at {}: {}",
            expected_path.display(),
            err
        )
    })
}
