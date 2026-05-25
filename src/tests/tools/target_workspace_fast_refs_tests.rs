//! Tests for target-workspace `fast_refs` parity (Task 1).
//!
//! Verifies that `find_references_in_target_workspace` correctly:
//! - Accepts and applies `limit` parameter (truncation after sorting)
//! - Accepts and applies `reference_kind` filter (on relationships + identifiers)
//! - Includes identifier-based reference discovery (Strategy 4)
//! - Deduplicates identifier refs against existing relationships and definitions

#[cfg(test)]
mod tests {
    use crate::database::{FileInfo, SymbolDatabase};
    use crate::extractors::base::{Relationship, RelationshipKind, Symbol, SymbolKind};
    use std::collections::HashSet;
    use tempfile::TempDir;

    // =========================================================================
    // Helpers
    // =========================================================================

    /// Create a test database with file info entries pre-seeded.
    fn setup_db(files: &[&str]) -> (TempDir, SymbolDatabase) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        for file in files {
            db.store_file_info(&FileInfo {
                path: file.to_string(),
                language: "rust".to_string(),
                hash: format!("hash_{}", file),
                size: 500,
                last_modified: 1000000,
                last_indexed: 0,
                symbol_count: 2,
                line_count: 0,
                content: None,
            })
            .unwrap();
        }
        (temp_dir, db)
    }

    fn make_symbol(id: &str, name: &str, file_path: &str, line: u32) -> Symbol {
        Symbol {
            id: id.to_string(),
            name: name.to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: file_path.to_string(),
            start_line: line,
            end_line: line + 5,
            start_column: 0,
            end_column: 0,
            start_byte: 0,
            end_byte: 0,
            parent_id: None,
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

    fn make_relationship(
        id: &str,
        from: &str,
        to: &str,
        file_path: &str,
        line: u32,
        kind: RelationshipKind,
        confidence: f32,
    ) -> Relationship {
        Relationship {
            id: id.to_string(),
            from_symbol_id: from.to_string(),
            to_symbol_id: to.to_string(),
            kind,
            file_path: file_path.to_string(),
            line_number: line,
            confidence,
            metadata: None,
        }
    }

    /// Store a dummy caller symbol in the DB so FK constraints are satisfied
    /// when we create relationships from it.
    fn store_caller_symbol(db: &mut SymbolDatabase, id: &str, file_path: &str, line: u32) {
        let sym = make_symbol(id, &format!("caller_{}", id), file_path, line);
        db.store_symbols(&[sym]).unwrap();
    }

    /// Insert a raw identifier into the test database.
    fn insert_identifier(
        db: &SymbolDatabase,
        name: &str,
        kind: &str,
        file: &str,
        line: u32,
        containing_symbol_id: Option<&str>,
        confidence: f32,
    ) {
        db.conn
            .execute(
                "INSERT INTO identifiers (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, containing_symbol_id, confidence)
                 VALUES (?1, ?2, ?3, 'rust', ?4, ?5, 0, ?5, 10, 0, 100, ?6, ?7)",
                rusqlite::params![
                    format!("ident_{}_{}_{}", name, file, line),
                    name,
                    kind,
                    file,
                    line,
                    containing_symbol_id,
                    confidence,
                ],
            )
            .unwrap();
    }

    /// Insert a raw identifier into the test database with an explicit target link.
    fn insert_identifier_with_target(
        db: &SymbolDatabase,
        name: &str,
        kind: &str,
        file: &str,
        line: u32,
        containing_symbol_id: Option<&str>,
        target_symbol_id: Option<&str>,
        confidence: f32,
    ) {
        db.conn
            .execute(
                "INSERT INTO identifiers (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, containing_symbol_id, target_symbol_id, confidence)
                 VALUES (?1, ?2, ?3, 'rust', ?4, ?5, 0, ?5, 10, 0, 100, ?6, ?7, ?8)",
                rusqlite::params![
                    format!("ident_{}_{}_{}_{}_{}", name, kind, file, line, confidence.to_bits()),
                    name,
                    kind,
                    file,
                    line,
                    containing_symbol_id,
                    target_symbol_id,
                    confidence,
                ],
            )
            .unwrap();
    }

    /// The function under test calls into the target-workspace logic
    /// with a raw SymbolDatabase, bypassing the handler/workspace machinery.
    ///
    /// This mirrors what `find_references_in_target_workspace` does internally
    /// inside the `spawn_blocking` block, but extracted so we can unit-test it.
    fn find_refs_in_db(
        db: &SymbolDatabase,
        symbol: &str,
        limit: u32,
        reference_kind: Option<&str>,
    ) -> (Vec<Symbol>, Vec<Relationship>) {
        use crate::extractors::base::RelationshipKind;
        use crate::tools::navigation::resolution::parse_qualified_name;
        use crate::utils::cross_language_intelligence::generate_naming_variants;

        let symbol_owned = symbol.to_string();
        let (effective_symbol, parent_filter) = match parse_qualified_name(&symbol_owned) {
            Some((parent, child)) => (child.to_string(), Some(parent.to_string())),
            None => (symbol_owned.clone(), None),
        };

        // Strategy 1: Exact name lookup
        let mut defs = db
            .get_symbols_by_name(&effective_symbol)
            .unwrap_or_default();

        // Strategy 2: Cross-language naming variants
        let variants = generate_naming_variants(&effective_symbol);
        if defs.is_empty() {
            for variant in &variants {
                if *variant != effective_symbol {
                    if let Ok(variant_symbols) = db.get_symbols_by_name(variant) {
                        for sym in variant_symbols {
                            if sym.name == *variant {
                                defs.push(sym);
                            }
                        }
                    }
                }
            }
        }

        if let Some(ref parent_name) = parent_filter {
            let parent_ids: Vec<String> = defs
                .iter()
                .filter_map(|s| s.parent_id.clone())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();

            if !parent_ids.is_empty() {
                let parents = db.get_symbols_by_ids(&parent_ids).unwrap_or_default();
                let matching_parent_ids: std::collections::HashSet<String> = parents
                    .into_iter()
                    .filter(|p| p.name == *parent_name)
                    .map(|p| p.id)
                    .collect();

                defs.retain(|s| {
                    s.parent_id
                        .as_deref()
                        .map(|pid| matching_parent_ids.contains(pid))
                        .unwrap_or(false)
                });
            } else {
                defs.clear();
            }
        }

        // Deduplicate definitions
        defs.sort_by(|a, b| a.id.cmp(&b.id));
        defs.dedup_by(|a, b| a.id == b.id);

        let mut import_refs: Vec<Relationship> = Vec::new();
        defs.retain(|sym| {
            if sym.kind == SymbolKind::Import {
                import_refs.push(Relationship {
                    id: format!("import_{}_{}", sym.file_path, sym.start_line),
                    from_symbol_id: sym.id.clone(),
                    to_symbol_id: String::new(),
                    kind: RelationshipKind::Imports,
                    file_path: sym.file_path.clone(),
                    line_number: sym.start_line,
                    confidence: 1.0,
                    metadata: None,
                });
                false
            } else {
                true
            }
        });

        // Strategy 3: Relationships to symbols
        let definition_ids: Vec<String> = defs.iter().map(|d| d.id.clone()).collect();

        let mut refs: Vec<Relationship> = match reference_kind {
            Some(kind) if kind != "import" => Vec::new(),
            _ => import_refs,
        };

        let relationship_refs: Vec<Relationship> = if let Some(kind) = reference_kind {
            db.get_relationships_to_symbols_filtered_by_kind(&definition_ids, kind)
                .unwrap_or_default()
        } else {
            db.get_relationships_to_symbols(&definition_ids)
                .unwrap_or_default()
        };
        refs.extend(relationship_refs);

        // Strategy 4: Identifier-based reference discovery
        let mut all_names = vec![effective_symbol.clone()];
        for v in &variants {
            if *v != effective_symbol {
                all_names.push(v.clone());
            }
        }

        let first_def_id = defs.first().map(|d| d.id.clone()).unwrap_or_default();

        let identifier_refs = if let Some(kind) = reference_kind {
            db.get_identifiers_by_names_and_kind(&all_names, kind)
                .unwrap_or_default()
        } else {
            db.get_identifiers_by_names(&all_names).unwrap_or_default()
        };

        // Build dedup set from existing relationships AND definitions
        let mut existing_refs: HashSet<(String, u32)> = refs
            .iter()
            .map(|r| (r.file_path.clone(), r.line_number))
            .collect();
        for def in &defs {
            existing_refs.insert((def.file_path.clone(), def.start_line));
        }

        for ident in identifier_refs {
            let key = (ident.file_path.clone(), ident.start_line);
            if existing_refs.contains(&key) {
                continue;
            }

            let rel_kind = match ident.kind.as_str() {
                "call" => RelationshipKind::Calls,
                "import" => RelationshipKind::Imports,
                "type_usage" => RelationshipKind::Uses,
                "member_access" => RelationshipKind::References,
                _ => RelationshipKind::References,
            };

            refs.push(Relationship {
                id: format!("ident_{}_{}", ident.file_path, ident.start_line),
                from_symbol_id: ident.containing_symbol_id.unwrap_or_default(),
                to_symbol_id: first_def_id.clone(),
                kind: rel_kind,
                file_path: ident.file_path,
                line_number: ident.start_line,
                confidence: ident.confidence,
                metadata: None,
            });
        }

        // Sort references by confidence (descending), then file_path, then line_number
        refs.sort_by(|a, b| {
            let conf_cmp = b
                .confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal);
            if conf_cmp != std::cmp::Ordering::Equal {
                return conf_cmp;
            }
            let file_cmp = a.file_path.cmp(&b.file_path);
            if file_cmp != std::cmp::Ordering::Equal {
                return file_cmp;
            }
            a.line_number.cmp(&b.line_number)
        });

        // Apply limit
        refs.truncate(limit as usize);

        (defs, refs)
    }

    mod async_target_workspace;
    mod identifier_refs;
    mod kind_filtering;
    mod limits;
    mod qualified_names;
}
