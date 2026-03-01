//! DI Registration Relationship Extraction for C#
//!
//! Extracts `Instantiates` relationships from DI container registration calls:
//! - `services.AddScoped<IFoo, Foo>()` → Instantiates to IFoo and Foo
//! - `services.AddSingleton<MyService>()` → Instantiates to MyService
//! - `services.AddHostedService<Worker>()` → Instantiates to Worker
//! - `builder.Services.AddScoped<T>()` → chained access works too
//!
//! Without these relationships, DI-registered classes have zero graph centrality
//! because no source code references them directly — the container resolves them
//! at runtime.

use crate::base::{PendingRelationship, Relationship, RelationshipKind, Symbol, SymbolKind};
use crate::csharp::CSharpExtractor;
use crate::csharp::member_type_relationships::{extract_type_name_from_node, find_containing_class};

/// DI registration method names that we recognize.
/// These are the standard Microsoft.Extensions.DependencyInjection methods.
const DI_REGISTRATION_METHODS: &[&str] = &[
    "AddSingleton",
    "AddScoped",
    "AddTransient",
    "AddHostedService",
    "AddKeyedSingleton",
    "AddKeyedScoped",
    "AddKeyedTransient",
    "TryAddSingleton",
    "TryAddScoped",
    "TryAddTransient",
];

/// Extract `Instantiates` relationships from DI registration calls.
///
/// Called for every `invocation_expression` node in the C# AST.
/// Only produces relationships when the call is a recognized DI registration method
/// with generic type arguments.
pub(crate) fn extract_di_registration_relationships(
    extractor: &mut CSharpExtractor,
    node: tree_sitter::Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    // We need: invocation_expression → member_access_expression → generic_name
    // The member_access_expression is the first child of invocation_expression
    let first_child = match node.child(0) {
        Some(c) if c.kind() == "member_access_expression" => c,
        _ => return,
    };

    // Find the generic_name child of the member_access_expression
    let mut cursor = first_child.walk();
    let generic_name = match first_child
        .children(&mut cursor)
        .find(|c| c.kind() == "generic_name")
    {
        Some(gn) => gn,
        None => return,
    };

    // Extract method name from generic_name's identifier child
    let base = extractor.get_base();
    let mut gc = generic_name.walk();
    let children: Vec<_> = generic_name.children(&mut gc).collect();

    let method_name = match children.iter().find(|c| c.kind() == "identifier") {
        Some(ident) => base.get_node_text(ident),
        None => return,
    };

    // Check if this is a recognized DI registration method
    if !DI_REGISTRATION_METHODS.contains(&method_name.as_str()) {
        return;
    }

    // Extract type arguments
    let type_arg_list = match children.iter().find(|c| c.kind() == "type_argument_list") {
        Some(tal) => *tal,
        None => return,
    };

    // Find containing class — this is the `from` side of the relationship
    let containing_class = match find_containing_class(base, node, symbols) {
        Some(cls) => cls,
        None => return, // Top-level statements — skip, matching extract_call_relationships behavior
    };

    let file_path = base.file_path.clone();
    let line_number = node.start_position().row as u32 + 1;

    // Collect type names first (needs immutable borrow on base via extract_type_name_from_node),
    // then emit relationships/pending after (needs mutable borrow for add_pending_relationship).
    let mut type_names: Vec<String> = Vec::new();
    let mut tc = type_arg_list.walk();
    for type_node in type_arg_list.children(&mut tc) {
        if let Some(name) = extract_type_name_from_node(base, type_node) {
            type_names.push(name);
        }
    }

    let from_id = containing_class.id.clone();

    // Now emit relationships — base borrow is no longer needed
    for type_name in type_names {
        // DI type arguments always refer to types, not constructors.
        // Prefer class/interface/struct matches to avoid hitting a same-named constructor.
        if let Some(target) = symbols
            .iter()
            .find(|s| s.name == type_name && matches!(s.kind, SymbolKind::Class | SymbolKind::Interface | SymbolKind::Struct | SymbolKind::Type))
            .or_else(|| symbols.iter().find(|s| s.name == type_name))
        {
            relationships.push(Relationship {
                id: format!("rel-di-{}-{}-{}", from_id, target.id, line_number),
                from_symbol_id: from_id.clone(),
                to_symbol_id: target.id.clone(),
                kind: RelationshipKind::Instantiates,
                file_path: file_path.clone(),
                line_number,
                confidence: 0.9,
                metadata: None,
            });
        } else {
            extractor.add_pending_relationship(PendingRelationship {
                from_symbol_id: from_id.clone(),
                callee_name: type_name,
                kind: RelationshipKind::Instantiates,
                file_path: file_path.clone(),
                line_number,
                confidence: 0.9,
            });
        }
    }
}
