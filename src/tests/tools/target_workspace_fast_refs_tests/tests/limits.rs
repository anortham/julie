use super::*;

// =========================================================================
// Tests: limit parameter
// =========================================================================

#[test]
fn test_target_workspace_limit_truncates_references() {
    let files = &[
        "src/lib.rs",
        "src/a.rs",
        "src/b.rs",
        "src/c.rs",
        "src/d.rs",
        "src/e.rs",
    ];
    let (_tmp, mut db) = setup_db(files);

    // Store the target symbol
    let target = make_symbol("sym-process", "process", "src/lib.rs", 10);
    db.store_symbols(&[target]).unwrap();

    // Store caller symbols (FK constraint requires them to exist)
    for i in 0..5u8 {
        let file = format!("src/{}.rs", (b'a' + i) as char);
        store_caller_symbol(&mut db, &format!("caller_{}", i), &file, 20 + i as u32);
    }

    // Store 5 relationships pointing to the target
    let rels: Vec<Relationship> = (0..5u8)
        .map(|i| {
            let file = format!("src/{}.rs", (b'a' + i) as char);
            make_relationship(
                &format!("rel_{}", i),
                &format!("caller_{}", i),
                "sym-process",
                &file,
                20 + i as u32,
                RelationshipKind::Calls,
                0.9,
            )
        })
        .collect();
    db.store_relationships(&rels).unwrap();

    // With limit=3, should only get 3 references
    let (defs, refs) = find_refs_in_db(&db, "process", 3, None);
    assert_eq!(defs.len(), 1, "should find 1 definition");
    assert_eq!(
        refs.len(),
        3,
        "should truncate to limit=3, got {} refs",
        refs.len()
    );

    // With limit=10, should get all 5
    let (_, refs_all) = find_refs_in_db(&db, "process", 10, None);
    assert_eq!(
        refs_all.len(),
        5,
        "with limit=10, should get all 5 refs, got {}",
        refs_all.len()
    );
}

#[test]
fn test_target_workspace_limit_applied_after_sorting() {
    let files = &["src/lib.rs", "src/a.rs", "src/b.rs", "src/c.rs"];
    let (_tmp, mut db) = setup_db(files);

    let target = make_symbol("sym-foo", "foo", "src/lib.rs", 1);
    db.store_symbols(&[target]).unwrap();

    // Store caller symbols for FK constraints
    store_caller_symbol(&mut db, "caller_low", "src/c.rs", 30);
    store_caller_symbol(&mut db, "caller_high", "src/a.rs", 10);
    store_caller_symbol(&mut db, "caller_mid", "src/b.rs", 20);

    // Store 3 relationships with different confidence levels
    let rels = vec![
        make_relationship(
            "rel_low",
            "caller_low",
            "sym-foo",
            "src/c.rs",
            30,
            RelationshipKind::Calls,
            0.5, // lowest confidence
        ),
        make_relationship(
            "rel_high",
            "caller_high",
            "sym-foo",
            "src/a.rs",
            10,
            RelationshipKind::Calls,
            0.95, // highest confidence
        ),
        make_relationship(
            "rel_mid",
            "caller_mid",
            "sym-foo",
            "src/b.rs",
            20,
            RelationshipKind::Calls,
            0.8, // mid confidence
        ),
    ];
    db.store_relationships(&rels).unwrap();

    // Limit to 2: should keep the two highest confidence refs
    let (_, refs) = find_refs_in_db(&db, "foo", 2, None);
    assert_eq!(refs.len(), 2);
    // Should be sorted by confidence descending, so highest first
    assert!(
        refs[0].confidence >= refs[1].confidence,
        "refs should be sorted by confidence descending: {} >= {}",
        refs[0].confidence,
        refs[1].confidence
    );
    assert_eq!(
        refs[0].file_path, "src/a.rs",
        "highest confidence ref should be first"
    );
}

#[test]
fn test_target_workspace_exact_definition_suppresses_variant_definition_noise() {
    let files = &["src/language_spec.rs"];
    let (_tmp, mut db) = setup_db(files);

    let type_symbol = make_symbol(
        "sym-language-spec",
        "LanguageSpec",
        "src/language_spec.rs",
        10,
    );
    let function_symbol = make_symbol(
        "sym-language-spec-fn",
        "language_spec",
        "src/language_spec.rs",
        20,
    );
    db.store_symbols(&[type_symbol, function_symbol]).unwrap();

    let (defs, refs) = find_refs_in_db(&db, "LanguageSpec", 10, None);
    let definition_names = defs
        .iter()
        .map(|symbol| symbol.name.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        definition_names,
        vec!["LanguageSpec"],
        "exact symbol lookup should not mix in naming-variant definitions when an exact definition exists"
    );
    assert!(
        refs.is_empty(),
        "the regression fixture stores definitions only, not references"
    );
}
