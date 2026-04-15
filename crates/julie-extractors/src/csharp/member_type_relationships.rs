// C# Member Type Relationship Extraction
//
// Extracts `Uses` relationships from field and property type declarations,
// and provides shared type-name resolution used by constructor params too.

use crate::base::{Relationship, RelationshipKind, Symbol, SymbolKind, UnresolvedTarget};
use crate::csharp::CSharpExtractor;

/// Extract field type relationships.
///
/// C# fields: `private ILogger _logger;`
/// AST: field_declaration → [modifier*] → variable_declaration → {type_node, variable_declarator}
///
/// Creates `Uses` relationship from the containing class to the field type.
/// Deduplicates against existing relationships (e.g., from constructor params).
pub(crate) fn extract_field_type_relationships(
    extractor: &mut CSharpExtractor,
    node: tree_sitter::Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let (class_symbol_id, type_name, line_number, row) = {
        let base = extractor.get_base();

        // Find the variable_declaration child which contains the type
        let mut cursor = node.walk();
        let var_decl = node
            .children(&mut cursor)
            .find(|c| c.kind() == "variable_declaration");
        let Some(var_decl) = var_decl else { return };

        // The type is the first meaningful child of variable_declaration
        let mut vd_cursor = var_decl.walk();
        let type_node = var_decl.children(&mut vd_cursor).next();
        let Some(type_node) = type_node else { return };

        let type_name = match extract_type_name_from_node(base, type_node) {
            Some(name) if !name.is_empty() => name,
            _ => return,
        };

        // Find containing class
        let class_symbol = find_containing_class(base, node, symbols);
        let Some(class_symbol) = class_symbol else {
            return;
        };

        (
            class_symbol.id.clone(),
            type_name,
            node.start_position().row as u32 + 1,
            node.start_position().row,
        )
    };

    emit_uses_relationship(
        extractor,
        node,
        &class_symbol_id,
        &type_name,
        line_number,
        row,
        symbols,
        relationships,
    );
}

/// Extract property type relationships.
///
/// C# properties: `public ILogger Logger { get; set; }`
/// AST: property_declaration → [modifier*] → type_node → identifier → accessor_list
///
/// Creates `Uses` relationship from the containing class to the property type.
/// Deduplicates against existing relationships (e.g., from constructor params or fields).
pub(crate) fn extract_property_type_relationships(
    extractor: &mut CSharpExtractor,
    node: tree_sitter::Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let (class_symbol_id, type_name, line_number, row) = {
        let base = extractor.get_base();

        // The type is a direct child of property_declaration, after modifiers
        let mut cursor = node.walk();
        let mut type_name = None;
        for child in node.children(&mut cursor) {
            match child.kind() {
                "modifier" => continue,
                // The first non-modifier child is the type node
                _ => {
                    type_name = extract_type_name_from_node(base, child);
                    break;
                }
            }
        }

        let type_name = match type_name {
            Some(name) if !name.is_empty() => name,
            _ => return,
        };

        // Find containing class
        let class_symbol = find_containing_class(base, node, symbols);
        let Some(class_symbol) = class_symbol else {
            return;
        };

        (
            class_symbol.id.clone(),
            type_name,
            node.start_position().row as u32 + 1,
            node.start_position().row,
        )
    };

    emit_uses_relationship(
        extractor,
        node,
        &class_symbol_id,
        &type_name,
        line_number,
        row,
        symbols,
        relationships,
    );
}

/// Find the containing class/struct/record symbol by walking up the tree.
pub(crate) fn find_containing_class<'a>(
    base: &crate::base::BaseExtractor,
    node: tree_sitter::Node,
    symbols: &'a [Symbol],
) -> Option<&'a Symbol> {
    let mut current = Some(node);
    while let Some(candidate) = current {
        if candidate.kind() == "class_declaration"
            || candidate.kind() == "struct_declaration"
            || candidate.kind() == "record_declaration"
        {
            let mut candidate_cursor = candidate.walk();
            if let Some(name_node) = candidate
                .children(&mut candidate_cursor)
                .find(|c| c.kind() == "identifier")
            {
                let class_name = base.get_node_text(&name_node);
                let declaration_start_line = candidate.start_position().row as u32 + 1;

                let exact_match = symbols.iter().find(|s| {
                    s.name == class_name
                        && (s.kind == SymbolKind::Class
                            || s.kind == SymbolKind::Struct
                            || s.kind == SymbolKind::Type)
                        && s.file_path == base.file_path
                        && s.start_line == declaration_start_line
                });

                if exact_match.is_some() {
                    return exact_match;
                }

                // Fallback for malformed/incomplete syntax trees where declaration positions
                // can drift from extracted symbol spans.
                return symbols.iter().find(|s| {
                    s.name == class_name
                        && (s.kind == SymbolKind::Class
                            || s.kind == SymbolKind::Struct
                            || s.kind == SymbolKind::Type)
                        && s.file_path == base.file_path
                });
            }
            break;
        }
        current = candidate.parent();
    }
    None
}

/// Emit a Uses relationship, deduplicating against existing relationships.
///
/// If the type is found in local symbols, creates a resolved Relationship.
/// If not found, creates a PendingRelationship for cross-file resolution.
/// Skips if a Uses relationship from this class to this type already exists.
fn emit_uses_relationship(
    extractor: &mut CSharpExtractor,
    node: tree_sitter::Node,
    class_symbol_id: &str,
    type_name: &str,
    line_number: u32,
    row: usize,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    // Check for existing Uses to this type (from constructor params, other fields, etc.)
    let already_exists = relationships.iter().any(|r| {
        r.from_symbol_id == class_symbol_id
            && r.kind == RelationshipKind::Uses
            && (r.to_symbol_id == type_name
                || symbols
                    .iter()
                    .find(|s| s.id == r.to_symbol_id)
                    .map_or(false, |s| s.name == type_name))
    });
    if already_exists {
        return;
    }

    // Also check pending relationships
    let pending_exists = extractor.get_pending_relationships().iter().any(|p| {
        p.from_symbol_id == class_symbol_id
            && p.kind == RelationshipKind::Uses
            && p.callee_name == type_name
    });
    if pending_exists {
        return;
    }

    let file_path = extractor.get_base().file_path.clone();

    // Build symbol map preferring type-defining symbols (Class, Interface, Struct, Enum)
    // over member symbols (Property, Field, Method) for type resolution.
    // This handles cases where a member symbol shares a name with a type
    // (e.g., property extracted with type name due to AST ambiguity).
    let mut symbol_map: std::collections::HashMap<String, &Symbol> =
        symbols.iter().map(|s| (s.name.clone(), s)).collect();
    for s in symbols.iter().filter(|s| {
        matches!(
            s.kind,
            SymbolKind::Class
                | SymbolKind::Interface
                | SymbolKind::Struct
                | SymbolKind::Enum
                | SymbolKind::Trait
                | SymbolKind::Type
        )
    }) {
        symbol_map.insert(s.name.clone(), s);
    }

    match symbol_map.get(type_name) {
        Some(type_symbol) => {
            relationships.push(Relationship {
                id: format!(
                    "{}_{}_{:?}_{}",
                    class_symbol_id,
                    type_symbol.id,
                    RelationshipKind::Uses,
                    row
                ),
                from_symbol_id: class_symbol_id.to_string(),
                to_symbol_id: type_symbol.id.clone(),
                kind: RelationshipKind::Uses,
                file_path,
                line_number,
                confidence: 0.9,
                metadata: None,
            });
        }
        None => {
            let mut pending = extractor.get_base().create_pending_relationship(
                class_symbol_id.to_string(),
                UnresolvedTarget::simple(type_name),
                RelationshipKind::Uses,
                &node,
                Some(class_symbol_id.to_string()),
                Some(0.8),
            );
            pending.pending.line_number = line_number;
            extractor.add_structured_pending_relationship(pending);
        }
    }
}

/// Extract the type name from a constructor parameter node.
/// Returns `None` for predefined/tuple types (not interesting relationships).
pub(crate) fn extract_parameter_type_name(
    base: &crate::base::BaseExtractor,
    param: tree_sitter::Node,
) -> Option<String> {
    // Parameter structure: [modifier*] type identifier [= default]
    let mut cursor = param.walk();
    for child in param.children(&mut cursor) {
        match child.kind() {
            "modifier" | "parameter_modifier" | "params" | "this" => continue,
            "tuple_type" => return None,
            _ => return extract_type_name_from_node(base, child),
        }
    }
    None
}

/// Extract the type name from a type AST node.
///
/// Handles all C# type forms:
/// - Simple: `ILogger` (identifier)
/// - Generic: `IRepository<User>` (generic_name → extracts base name)
/// - Nullable: `ILogger?` (nullable_type → unwraps inner)
/// - Qualified: `Namespace.IType` (qualified_name → last identifier)
/// - Array: `ILogger[]` (array_type → extracts base)
/// - Tuple: `(int x, int y)` (tuple_type) → returns None (member tuple shapes are not type deps)
/// - Predefined: `string`, `int`, `bool` → returns None (not interesting)
///
/// Shared between constructor parameters, field declarations, and property declarations.
pub(crate) fn extract_type_name_from_node(
    base: &crate::base::BaseExtractor,
    node: tree_sitter::Node,
) -> Option<String> {
    match node.kind() {
        // Skip predefined types (string, int, bool, etc.)
        "predefined_type" => None,

        // Skip tuple types: (int a, int b)
        "tuple_type" => None,

        // Simple type: ILogger, MyClass
        "identifier" => Some(base.get_node_text(&node)),

        // Generic: ILogger<MyService> -> "ILogger"
        "generic_name" => {
            let mut gc = node.walk();
            node.children(&mut gc)
                .find(|c| c.kind() == "identifier")
                .map(|name_node| base.get_node_text(&name_node))
        }

        // Nullable: ILogger? -> unwrap inner type
        "nullable_type" => {
            let mut nc = node.walk();
            for inner in node.children(&mut nc) {
                match inner.kind() {
                    "predefined_type" => return None,
                    "identifier" => return Some(base.get_node_text(&inner)),
                    "generic_name" => {
                        let mut gc = inner.walk();
                        return inner
                            .children(&mut gc)
                            .find(|c| c.kind() == "identifier")
                            .map(|name_node| base.get_node_text(&name_node));
                    }
                    _ => {}
                }
            }
            None
        }

        // Qualified: Namespace.IType -> last identifier
        "qualified_name" => {
            let mut qc = node.walk();
            node.children(&mut qc)
                .filter(|c| c.kind() == "identifier")
                .last()
                .map(|ident| base.get_node_text(&ident))
        }

        // Array: ILogger[] -> extract base type
        "array_type" => {
            let mut ac = node.walk();
            for inner in node.children(&mut ac) {
                match inner.kind() {
                    "predefined_type" => return None,
                    "identifier" => return Some(base.get_node_text(&inner)),
                    "generic_name" => {
                        let mut gc = inner.walk();
                        return inner
                            .children(&mut gc)
                            .find(|c| c.kind() == "identifier")
                            .map(|name_node| base.get_node_text(&name_node));
                    }
                    _ => {}
                }
            }
            None
        }

        // Fallback: if node kind contains "type" or "name", treat as type
        _ => {
            if node.kind().contains("type") || node.kind().contains("name") {
                Some(base.get_node_text(&node))
            } else {
                None
            }
        }
    }
}
