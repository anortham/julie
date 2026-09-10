use super::*;

fn five_callers() -> Vec<(&'static str, String)> {
    vec![
        ("src/lib.rs", "pub fn process() {}\n".to_string()),
        ("src/a.rs", caller("caller_a", "process")),
        ("src/b.rs", caller("caller_b", "process")),
        ("src/c.rs", caller("caller_c", "process")),
        ("src/d.rs", caller("caller_d", "process")),
        ("src/e.rs", caller("caller_e", "process")),
    ]
}

fn borrowed<'a>(files: &'a [(&'static str, String)]) -> Vec<(&'static str, &'a str)> {
    files.iter().map(|(p, c)| (*p, c.as_str())).collect()
}

#[test]
fn test_target_workspace_limit_truncates_references() {
    let files = five_callers();

    let found = refs(&borrowed(&files), "process", 3, None);
    assert_eq!(found.definitions.len(), 1, "should find 1 definition");
    assert_eq!(
        found.references.len(),
        3,
        "should truncate to limit=3, got {} refs",
        found.references.len()
    );

    let all = refs(&borrowed(&files), "process", 10, None);
    assert_eq!(
        all.references.len(),
        5,
        "with limit=10, should get all 5 refs, got {}",
        all.references.len()
    );
}

#[test]
fn test_target_workspace_limit_applied_after_sorting() {
    let files = five_callers();

    let found = refs(&borrowed(&files), "process", 2, None);
    assert_eq!(found.references.len(), 2);
    assert!(
        found.references[0].confidence >= found.references[1].confidence,
        "refs should be sorted by confidence descending: {} >= {}",
        found.references[0].confidence,
        found.references[1].confidence
    );
    assert_eq!(
        found.references[0].file_path, "src/a.rs",
        "sorting must run before the limit cuts the list"
    );
    assert_eq!(found.references[1].file_path, "src/b.rs");
}

#[test]
fn test_target_workspace_exact_definition_suppresses_variant_definition_noise() {
    let found = refs(
        &[(
            "src/language_spec.rs",
            "pub struct LanguageSpec;\npub fn language_spec() {}\n",
        )],
        "LanguageSpec",
        10,
        None,
    );
    let definition_names = found
        .definitions
        .iter()
        .map(|symbol| symbol.name.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        definition_names,
        vec!["LanguageSpec"],
        "exact symbol lookup should not mix in naming-variant definitions when an exact definition exists"
    );
    assert!(
        found.references.is_empty(),
        "the regression fixture stores definitions only, not references"
    );
}
