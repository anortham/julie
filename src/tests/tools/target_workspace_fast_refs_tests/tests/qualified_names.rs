use super::*;

// =========================================================================
// Tests: qualified name support (Task 2)
// =========================================================================

/// Mirror of the qualified-name resolution logic that will be in
/// `find_references_and_definitions`: parse "Parent::child", look up by child
/// name only, then retain definitions whose parent symbol name matches.
fn find_defs_qualified(db: &SymbolDatabase, symbol: &str) -> Vec<Symbol> {
    use crate::tools::navigation::resolution::parse_qualified_name;

    let (effective_symbol, parent_filter) = match parse_qualified_name(symbol) {
        Some((parent, child)) => (child.to_string(), Some(parent.to_string())),
        None => (symbol.to_string(), None),
    };

    let mut defs = db
        .get_symbols_by_name(&effective_symbol)
        .unwrap_or_default();

    if let Some(ref parent) = parent_filter {
        // Collect parent IDs from definitions that have one
        let parent_ids: Vec<String> = defs
            .iter()
            .filter_map(|s| s.parent_id.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        if !parent_ids.is_empty() {
            // Batch-fetch parent symbols
            let parents = db.get_symbols_by_ids(&parent_ids).unwrap_or_default();
            let matching_parent_ids: std::collections::HashSet<String> = parents
                .into_iter()
                .filter(|p| p.name == *parent)
                .map(|p| p.id)
                .collect();

            // Keep only definitions whose parent_id is in matching_parent_ids
            defs.retain(|s| {
                s.parent_id
                    .as_deref()
                    .map(|pid| matching_parent_ids.contains(pid))
                    .unwrap_or(false)
            });
        } else {
            // Definitions have no parent_id — qualified search finds nothing
            defs.clear();
        }
    }

    defs
}

/// Helper to create a class/struct symbol (parent container)
fn make_class_symbol(id: &str, name: &str, file_path: &str, line: u32) -> Symbol {
    Symbol {
        id: id.to_string(),
        name: name.to_string(),
        kind: SymbolKind::Class,
        language: "rust".to_string(),
        file_path: file_path.to_string(),
        start_line: line,
        end_line: line + 20,
        start_column: 0,
        end_column: 0,
        start_byte: 0,
        end_byte: 0,
        parent_id: None,
        signature: Some(format!("struct {}", name)),
        doc_comment: None,
        visibility: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: None,
        content_type: None,
        body_span: None,
        body_hash: None,
        annotations: Vec::new(),
    }
}

/// Helper to create a method symbol with a parent_id
fn make_method_symbol(id: &str, name: &str, file_path: &str, line: u32, parent_id: &str) -> Symbol {
    Symbol {
        id: id.to_string(),
        name: name.to_string(),
        kind: SymbolKind::Method,
        language: "rust".to_string(),
        file_path: file_path.to_string(),
        start_line: line,
        end_line: line + 5,
        start_column: 0,
        end_column: 0,
        start_byte: 0,
        end_byte: 0,
        parent_id: Some(parent_id.to_string()),
        signature: Some(format!("pub fn {}()", name)),
        doc_comment: None,
        visibility: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: None,
        content_type: None,
        body_span: None,
        body_hash: None,
        annotations: Vec::new(),
    }
}

#[test]
fn test_fast_refs_qualified_name_filters_by_parent() {
    let files = &["src/engine.rs", "src/pipeline.rs"];
    let (_tmp, mut db) = setup_db(files);

    // Store parent class symbols
    let engine = make_class_symbol("class-engine", "Engine", "src/engine.rs", 1);
    let pipeline = make_class_symbol("class-pipeline", "Pipeline", "src/pipeline.rs", 1);
    db.store_symbols(&[engine, pipeline]).unwrap();

    // Store "process" methods — one under Engine, one under Pipeline
    let engine_process = make_method_symbol(
        "method-engine-process",
        "process",
        "src/engine.rs",
        10,
        "class-engine",
    );
    let pipeline_process = make_method_symbol(
        "method-pipeline-process",
        "process",
        "src/pipeline.rs",
        10,
        "class-pipeline",
    );
    db.store_symbols(&[engine_process, pipeline_process])
        .unwrap();

    // Unqualified: should find both "process" methods
    let unqualified = find_defs_qualified(&db, "process");
    assert_eq!(
        unqualified.len(),
        2,
        "unqualified 'process' should find both methods, got {}",
        unqualified.len()
    );

    // Qualified "Engine::process": should find only the Engine method
    let engine_defs = find_defs_qualified(&db, "Engine::process");
    assert_eq!(
        engine_defs.len(),
        1,
        "Engine::process should find exactly 1 definition, got {}",
        engine_defs.len()
    );
    assert_eq!(engine_defs[0].id, "method-engine-process");
    assert_eq!(engine_defs[0].file_path, "src/engine.rs");

    // Qualified "Pipeline::process": should find only the Pipeline method
    let pipeline_defs = find_defs_qualified(&db, "Pipeline::process");
    assert_eq!(
        pipeline_defs.len(),
        1,
        "Pipeline::process should find exactly 1 definition, got {}",
        pipeline_defs.len()
    );
    assert_eq!(pipeline_defs[0].id, "method-pipeline-process");
    assert_eq!(pipeline_defs[0].file_path, "src/pipeline.rs");

    // Qualified with unknown parent: should find nothing
    let unknown_defs = find_defs_qualified(&db, "Unknown::process");
    assert_eq!(
        unknown_defs.len(),
        0,
        "Unknown::process should find nothing, got {}",
        unknown_defs.len()
    );
}

#[test]
fn test_fast_refs_qualified_dot_separator() {
    let files = &["src/service.rs"];
    let (_tmp, mut db) = setup_db(files);

    let service = make_class_symbol("class-service", "Service", "src/service.rs", 1);
    db.store_symbols(&[service]).unwrap();

    let method = make_method_symbol("method-run", "run", "src/service.rs", 5, "class-service");
    db.store_symbols(&[method]).unwrap();

    // Dot separator "Service.run" should also work
    let defs = find_defs_qualified(&db, "Service.run");
    assert_eq!(
        defs.len(),
        1,
        "Service.run should find 1 definition, got {}",
        defs.len()
    );
    assert_eq!(defs[0].id, "method-run");
}
