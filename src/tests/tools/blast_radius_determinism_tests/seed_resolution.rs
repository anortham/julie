use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn test_file_path_seeds_filter_noisy_structural_symbols_but_symbol_ids_are_exact()
-> Result<()> {
    let (_temp_dir, handler, workspace_id) = setup_handler().await?;

    let files = vec![make_file("src/noisy.ts", "hash_noisy")];
    let symbols = vec![
        make_symbol_with_kind("class_seed", "Pipeline", "src/noisy.ts", SymbolKind::Class),
        make_symbol_with_kind(
            "constructor_seed",
            "constructor",
            "src/noisy.ts",
            SymbolKind::Constructor,
        ),
        make_symbol_with_kind(
            "delegate_seed",
            "WorkDelegate",
            "src/noisy.ts",
            SymbolKind::Delegate,
        ),
        make_symbol_with_kind("enum_seed", "State", "src/noisy.ts", SymbolKind::Enum),
        make_symbol_with_kind("event_seed", "onReady", "src/noisy.ts", SymbolKind::Event),
        make_symbol_with_kind(
            "fn_seed",
            "runPipeline",
            "src/noisy.ts",
            SymbolKind::Function,
        ),
        make_symbol_with_kind(
            "interface_seed",
            "Runner",
            "src/noisy.ts",
            SymbolKind::Interface,
        ),
        make_symbol_with_kind("method_seed", "execute", "src/noisy.ts", SymbolKind::Method),
        make_symbol_with_kind(
            "module_seed",
            "pipeline",
            "src/noisy.ts",
            SymbolKind::Module,
        ),
        make_symbol_with_kind(
            "namespace_seed",
            "PipelineNS",
            "src/noisy.ts",
            SymbolKind::Namespace,
        ),
        make_symbol_with_kind(
            "operator_seed",
            "operator+",
            "src/noisy.ts",
            SymbolKind::Operator,
        ),
        make_symbol_with_kind("struct_seed", "Job", "src/noisy.ts", SymbolKind::Struct),
        make_symbol_with_kind("trait_seed", "Runnable", "src/noisy.ts", SymbolKind::Trait),
        make_symbol_with_kind("type_seed", "JobId", "src/noisy.ts", SymbolKind::Type),
        make_symbol_with_kind("union_seed", "Result", "src/noisy.ts", SymbolKind::Union),
        make_symbol_with_kind("field_seed", "status", "src/noisy.ts", SymbolKind::Field),
        make_symbol_with_kind(
            "enum_member_seed",
            "Ready",
            "src/noisy.ts",
            SymbolKind::EnumMember,
        ),
        make_symbol_with_kind("import_seed", "React", "src/noisy.ts", SymbolKind::Import),
        make_symbol_with_kind(
            "property_seed",
            "value",
            "src/noisy.ts",
            SymbolKind::Property,
        ),
        make_symbol_with_kind("variable_seed", "tmp", "src/noisy.ts", SymbolKind::Variable),
        make_symbol_with_kind(
            "constant_seed",
            "DEFAULT_LIMIT",
            "src/noisy.ts",
            SymbolKind::Constant,
        ),
    ];

    let db = handler.primary_database().await?;
    {
        let mut guard = db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.bulk_store_fresh_atomic(
            &files,
            &symbols,
            &Vec::<Relationship>::new(),
            &Vec::<Identifier>::new(),
            &[],
            workspace_id.as_str(),
        )?;

        let file_seed_context = resolve_seed_context(
            &BlastRadiusTool {
                symbol_ids: vec![],
                file_paths: vec!["src/noisy.ts".to_string()],
                from_revision: None,
                to_revision: None,
                max_depth: 1,
                limit: 10,
                include_tests: true,
                format: Some("compact".to_string()),
                workspace: Some("primary".to_string()),
            },
            &guard,
            workspace_id.as_str(),
        )?;
        let mut file_seed_ids: Vec<&str> = file_seed_context
            .seed_symbols
            .iter()
            .map(|symbol| symbol.id.as_str())
            .collect();
        file_seed_ids.sort();

        assert_eq!(
            file_seed_ids,
            vec![
                "class_seed",
                "constructor_seed",
                "delegate_seed",
                "enum_seed",
                "event_seed",
                "fn_seed",
                "interface_seed",
                "method_seed",
                "module_seed",
                "namespace_seed",
                "operator_seed",
                "struct_seed",
                "trait_seed",
                "type_seed",
                "union_seed",
            ],
            "file path seeds should keep meaningful definitions and drop noisy structural symbols"
        );

        let explicit_seed_context = resolve_seed_context(
            &BlastRadiusTool {
                symbol_ids: vec![
                    "field_seed".to_string(),
                    "enum_member_seed".to_string(),
                    "import_seed".to_string(),
                    "variable_seed".to_string(),
                    "constant_seed".to_string(),
                ],
                file_paths: vec![],
                from_revision: None,
                to_revision: None,
                max_depth: 1,
                limit: 10,
                include_tests: true,
                format: Some("compact".to_string()),
                workspace: Some("primary".to_string()),
            },
            &guard,
            workspace_id.as_str(),
        )?;
        let mut explicit_seed_ids: Vec<&str> = explicit_seed_context
            .seed_symbols
            .iter()
            .map(|symbol| symbol.id.as_str())
            .collect();
        explicit_seed_ids.sort();

        assert_eq!(
            explicit_seed_ids,
            vec![
                "constant_seed",
                "enum_member_seed",
                "field_seed",
                "import_seed",
                "variable_seed",
            ],
            "explicit symbol ids must be preserved even when they point to noisy structural symbols"
        );
    }

    Ok(())
}
