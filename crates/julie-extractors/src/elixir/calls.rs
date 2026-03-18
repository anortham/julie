use super::ElixirExtractor;
/// Core call dispatch for Elixir extraction.
///
/// In tree-sitter-elixir, nearly every definition is a `call` node.
/// This module inspects the call target and dispatches to the appropriate handler.
use super::helpers;
use crate::base::{Symbol, SymbolKind, SymbolOptions, Visibility};
use crate::test_detection::is_test_symbol;
use serde_json::Value;
use std::collections::HashMap;
use tree_sitter::Node;

/// Dispatch a call node to the appropriate extraction handler.
///
/// Returns `Some((symbol, children_visited))` if this call defines a symbol.
/// `children_visited` is true when the handler already traversed child nodes
/// (e.g., defmodule visits its do_block to extract nested definitions).
pub(super) fn dispatch_call(
    extractor: &mut ElixirExtractor,
    node: &Node,
    symbols: &mut Vec<Symbol>,
    parent_id: Option<&str>,
) -> Option<(Symbol, bool)> {
    let target_name = helpers::extract_call_target_name(&extractor.base, node)?;
    match target_name.as_str() {
        "defmodule" => extract_defmodule(extractor, node, symbols, parent_id),
        "def" => extract_def(extractor, node, parent_id, Visibility::Public),
        "defp" => extract_def(extractor, node, parent_id, Visibility::Private),
        "defmacro" => extract_defmacro(extractor, node, parent_id, Visibility::Public),
        "defmacrop" => extract_defmacro(extractor, node, parent_id, Visibility::Private),
        "defprotocol" => extract_defprotocol(extractor, node, symbols, parent_id),
        "defimpl" => extract_defimpl(extractor, node, symbols, parent_id),
        "defstruct" => extract_defstruct(extractor, node, symbols, parent_id),
        "import" => extract_import_call(extractor, node, parent_id),
        "use" => extract_use_call(extractor, node, parent_id),
        "alias" => extract_alias_call(extractor, node, parent_id),
        "require" => extract_require_call(extractor, node, parent_id),
        "test" => extract_test(extractor, node, parent_id),
        "describe" => extract_describe(extractor, node, symbols, parent_id),
        _ => None,
    }
}

// ========================================================================
// defmodule
// ========================================================================

fn extract_defmodule(
    extractor: &mut ElixirExtractor,
    node: &Node,
    symbols: &mut Vec<Symbol>,
    parent_id: Option<&str>,
) -> Option<(Symbol, bool)> {
    let module_name = helpers::extract_module_name(&extractor.base, node)?;

    let signature = format!("defmodule {}", module_name);
    let doc_comment = extractor.base.find_doc_comment(node);

    let symbol = extractor.base.create_symbol(
        node,
        module_name.clone(),
        SymbolKind::Module,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(String::from),
            metadata: None,
            doc_comment,
        },
    );

    let sym_id = symbol.id.clone();

    // Push module name for qualified name building
    extractor.module_stack.push(module_name);

    // Visit do_block children to extract nested definitions
    if let Some(do_block) = helpers::extract_do_block(node) {
        extractor.traverse_children(&do_block, symbols, Some(&sym_id));
    }

    extractor.module_stack.pop();

    Some((symbol, true)) // children already visited
}

// ========================================================================
// def / defp
// ========================================================================

fn extract_def(
    extractor: &mut ElixirExtractor,
    node: &Node,
    parent_id: Option<&str>,
    visibility: Visibility,
) -> Option<(Symbol, bool)> {
    let (fn_name, params) = helpers::extract_function_head(&extractor.base, node)?;

    let signature = match &params {
        Some(p) => format!("def {}{}", fn_name, p),
        None => format!("def {}", fn_name),
    };
    let doc_comment = extractor.base.find_doc_comment(node);

    // Test detection
    let metadata = if is_test_symbol(
        "elixir",
        &fn_name,
        &extractor.base.file_path,
        &SymbolKind::Function,
        &[],
        &[],
        doc_comment.as_deref(),
    ) {
        let mut m = HashMap::new();
        m.insert("is_test".to_string(), Value::Bool(true));
        Some(m)
    } else {
        None
    };

    let symbol = extractor.base.create_symbol(
        node,
        fn_name,
        SymbolKind::Function,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.map(String::from),
            metadata,
            doc_comment,
        },
    );

    Some((symbol, false))
}

// ========================================================================
// defmacro / defmacrop
// ========================================================================

fn extract_defmacro(
    extractor: &mut ElixirExtractor,
    node: &Node,
    parent_id: Option<&str>,
    visibility: Visibility,
) -> Option<(Symbol, bool)> {
    let (macro_name, params) = helpers::extract_function_head(&extractor.base, node)?;

    let keyword = if visibility == Visibility::Private {
        "defmacrop"
    } else {
        "defmacro"
    };
    let signature = match &params {
        Some(p) => format!("{} {}{}", keyword, macro_name, p),
        None => format!("{} {}", keyword, macro_name),
    };
    let doc_comment = extractor.base.find_doc_comment(node);

    let mut metadata = HashMap::new();
    metadata.insert("macro".to_string(), Value::Bool(true));

    let symbol = extractor.base.create_symbol(
        node,
        macro_name,
        SymbolKind::Function,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.map(String::from),
            metadata: Some(metadata),
            doc_comment,
        },
    );

    Some((symbol, false))
}

// ========================================================================
// defprotocol
// ========================================================================

fn extract_defprotocol(
    extractor: &mut ElixirExtractor,
    node: &Node,
    symbols: &mut Vec<Symbol>,
    parent_id: Option<&str>,
) -> Option<(Symbol, bool)> {
    let protocol_name = helpers::extract_module_name(&extractor.base, node)?;

    let signature = format!("defprotocol {}", protocol_name);
    let doc_comment = extractor.base.find_doc_comment(node);

    let symbol = extractor.base.create_symbol(
        node,
        protocol_name.clone(),
        SymbolKind::Interface,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(String::from),
            metadata: None,
            doc_comment,
        },
    );

    let sym_id = symbol.id.clone();

    extractor.module_stack.push(protocol_name);

    if let Some(do_block) = helpers::extract_do_block(node) {
        extractor.traverse_children(&do_block, symbols, Some(&sym_id));
    }

    extractor.module_stack.pop();

    Some((symbol, true))
}

// ========================================================================
// defimpl
// ========================================================================

fn extract_defimpl(
    extractor: &mut ElixirExtractor,
    node: &Node,
    symbols: &mut Vec<Symbol>,
    parent_id: Option<&str>,
) -> Option<(Symbol, bool)> {
    let protocol_name = helpers::extract_impl_protocol_name(&extractor.base, node)?;
    let for_type = helpers::extract_keyword_value(&extractor.base, node, "for").unwrap_or_default();

    let impl_name = if for_type.is_empty() {
        protocol_name.clone()
    } else {
        format!("{}.{}", protocol_name, for_type)
    };

    let signature = if for_type.is_empty() {
        format!("defimpl {}", protocol_name)
    } else {
        format!("defimpl {}, for: {}", protocol_name, for_type)
    };
    let doc_comment = extractor.base.find_doc_comment(node);

    let mut metadata = HashMap::new();
    metadata.insert("protocol_impl".to_string(), Value::Bool(true));
    if !for_type.is_empty() {
        metadata.insert("for_type".to_string(), Value::String(for_type));
    }
    metadata.insert("protocol".to_string(), Value::String(protocol_name));

    let symbol = extractor.base.create_symbol(
        node,
        impl_name.clone(),
        SymbolKind::Class,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(String::from),
            metadata: Some(metadata),
            doc_comment,
        },
    );

    let sym_id = symbol.id.clone();

    extractor.module_stack.push(impl_name);

    if let Some(do_block) = helpers::extract_do_block(node) {
        extractor.traverse_children(&do_block, symbols, Some(&sym_id));
    }

    extractor.module_stack.pop();

    Some((symbol, true))
}

// ========================================================================
// defstruct
// ========================================================================

fn extract_defstruct(
    extractor: &mut ElixirExtractor,
    node: &Node,
    symbols: &mut Vec<Symbol>,
    parent_id: Option<&str>,
) -> Option<(Symbol, bool)> {
    let fields = helpers::extract_struct_fields(&extractor.base, node);

    let struct_name = extractor
        .module_stack
        .last()
        .cloned()
        .unwrap_or_else(|| "Struct".to_string());

    let field_names: Vec<&str> = fields.iter().map(|(n, _, _)| n.as_str()).collect();
    let signature = format!("defstruct [{}]", field_names.join(", "));

    let symbol = extractor.base.create_symbol(
        node,
        struct_name,
        SymbolKind::Struct,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(String::from),
            metadata: None,
            doc_comment: None,
        },
    );

    let sym_id = symbol.id.clone();

    // Create field symbols as children — use the parent (defstruct) node for location
    // since we don't carry Node references for individual atoms
    for (field_name, _start_byte, _end_byte) in &fields {
        let field_sym = extractor.base.create_symbol(
            node,
            field_name.clone(),
            SymbolKind::Field,
            SymbolOptions {
                signature: Some(format!(":{}", field_name)),
                visibility: Some(Visibility::Public),
                parent_id: Some(sym_id.clone()),
                metadata: None,
                doc_comment: None,
            },
        );
        symbols.push(field_sym);
    }

    Some((symbol, true)) // fields already emitted
}

// ========================================================================
// Import calls: use, import, alias, require
// ========================================================================

fn extract_import_call(
    extractor: &mut ElixirExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<(Symbol, bool)> {
    extract_directive(extractor, node, parent_id, "import")
}

fn extract_use_call(
    extractor: &mut ElixirExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<(Symbol, bool)> {
    extract_directive(extractor, node, parent_id, "use")
}

fn extract_alias_call(
    extractor: &mut ElixirExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<(Symbol, bool)> {
    extract_directive(extractor, node, parent_id, "alias")
}

fn extract_require_call(
    extractor: &mut ElixirExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<(Symbol, bool)> {
    extract_directive(extractor, node, parent_id, "require")
}

fn extract_directive(
    extractor: &mut ElixirExtractor,
    node: &Node,
    parent_id: Option<&str>,
    keyword: &str,
) -> Option<(Symbol, bool)> {
    let target = helpers::extract_import_target(&extractor.base, node)?;
    let signature = format!("{} {}", keyword, extractor.base.get_node_text(node).trim());

    let symbol = extractor.base.create_symbol(
        node,
        target,
        SymbolKind::Import,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(String::from),
            metadata: None,
            doc_comment: None,
        },
    );

    Some((symbol, false))
}

// ========================================================================
// ExUnit: test / describe
// ========================================================================

/// Extract an ExUnit `test "description" do ... end` block as a Function symbol.
fn extract_test(
    extractor: &mut ElixirExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<(Symbol, bool)> {
    let description = helpers::extract_first_string_arg(&extractor.base, node)?;
    let signature = format!("test \"{}\"", description);

    let mut metadata = HashMap::new();
    metadata.insert("is_test".to_string(), Value::Bool(true));

    let symbol = extractor.base.create_symbol(
        node,
        description,
        SymbolKind::Function,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Private),
            parent_id: parent_id.map(String::from),
            metadata: Some(metadata),
            doc_comment: None,
        },
    );

    Some((symbol, false))
}

/// Extract an ExUnit `describe "context" do ... end` block as a Namespace symbol.
/// Traverses child nodes to extract nested test definitions.
fn extract_describe(
    extractor: &mut ElixirExtractor,
    node: &Node,
    symbols: &mut Vec<Symbol>,
    parent_id: Option<&str>,
) -> Option<(Symbol, bool)> {
    let description = helpers::extract_first_string_arg(&extractor.base, node)?;
    let signature = format!("describe \"{}\"", description);

    let symbol = extractor.base.create_symbol(
        node,
        description,
        SymbolKind::Namespace,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Private),
            parent_id: parent_id.map(String::from),
            metadata: None,
            doc_comment: None,
        },
    );

    let sym_id = symbol.id.clone();

    // Visit do_block children to extract nested test definitions
    if let Some(do_block) = helpers::extract_do_block(node) {
        extractor.traverse_children(&do_block, symbols, Some(&sym_id));
    }

    Some((symbol, true)) // children already visited
}
