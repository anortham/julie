use super::helpers::extract_method_name_from_call;
/// Special method call extraction for Ruby
/// Handles require, attr_accessor, define_method, def_delegator, module_function, Struct.new
use crate::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract special method calls that create symbols.
pub(super) fn extract_call(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Vec<Symbol> {
    let Some(method_name) = extract_method_name_from_call(node, |n| base.get_node_text(n)) else {
        return Vec::new();
    };

    match method_name.as_str() {
        "require" | "require_relative" => extract_require(base, node).into_iter().collect(),
        "describe" | "context" | "feature" => extract_rspec_block(
            base,
            node,
            &method_name,
            parent_id.as_deref(),
            RSpecBlockKind::Container,
        )
        .into_iter()
        .collect(),
        "it" | "specify" | "example" | "scenario" => extract_rspec_block(
            base,
            node,
            &method_name,
            parent_id.as_deref(),
            RSpecBlockKind::Example,
        )
        .into_iter()
        .collect(),
        "before" | "after" | "around" => extract_rspec_block(
            base,
            node,
            &method_name,
            parent_id.as_deref(),
            RSpecBlockKind::Lifecycle,
        )
        .into_iter()
        .collect(),
        "attr_reader" | "attr_writer" | "attr_accessor" => {
            extract_attr_accessor(base, node, &method_name, parent_id)
        }
        "define_method" | "define_singleton_method" => {
            extract_define_method(base, node, &method_name)
                .into_iter()
                .collect()
        }
        "def_delegator" => extract_def_delegator(base, node).into_iter().collect(),
        "module_function" => Vec::new(), // Recognized but no separate symbol needed
        _ => Vec::new(),
    }
}

/// Try to extract a Struct.new assignment as a Class symbol.
///
/// Called from the traversal when we encounter an assignment node.
/// Detects `Name = Struct.new(:field1, :field2, ...)` and returns a Class symbol.
/// Returns None if the assignment RHS is not a Struct.new call.
/// Returns `(class_symbol, field_properties)` so the caller can set `symbol_opt`
/// to the class (for child parenting) and push the properties into the symbols vec.
pub(super) fn try_extract_struct_new(
    base: &mut BaseExtractor,
    assignment_node: Node,
    parent_id: Option<String>,
) -> Option<(Symbol, Vec<Symbol>)> {
    // Get the left (name) and right (call) sides of the assignment
    let left_side = assignment_node.child_by_field_name("left")?;
    let right_side = assignment_node.child_by_field_name("right")?;

    // Check if the RHS is a call node with Struct.new pattern
    if right_side.kind() != "call" {
        return None;
    }

    // Verify the call is Struct.new: constant("Struct") . identifier("new")
    let mut cursor = right_side.walk();
    let children: Vec<_> = right_side.children(&mut cursor).collect();
    if children.len() < 3
        || children[0].kind() != "constant"
        || base.get_node_text(&children[0]) != "Struct"
        || children[2].kind() != "identifier"
        || base.get_node_text(&children[2]) != "new"
    {
        return None;
    }

    let name = base.get_node_text(&left_side);

    // Build signature: "Person = Struct.new(:name, :age, :email)"
    // Strip the do_block from the signature if present (keep just the Struct.new(...) part)
    let signature = if let Some(arg_list) = right_side.child_by_field_name("arguments") {
        format!("{} = Struct.new{}", name, base.get_node_text(&arg_list))
    } else {
        format!("{} = {}", name, base.get_node_text(&right_side))
    };

    // Extract doc comment from the assignment node
    let doc_comment = base.find_doc_comment(&assignment_node);

    let class_symbol = base.create_symbol(
        &assignment_node,
        name,
        SymbolKind::Class,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id,
            metadata: None,
            doc_comment,
            annotations: Vec::new(),
        },
    );

    // Extract field arguments as Property children of the class
    let mut field_properties = Vec::new();
    if let Some(arg_list) = right_side.child_by_field_name("arguments") {
        let mut arg_cursor = arg_list.walk();
        for arg_child in arg_list.children(&mut arg_cursor) {
            if arg_child.kind() == "simple_symbol" {
                let field_text = base.get_node_text(&arg_child);
                let field_name = field_text.trim_start_matches(':').to_string();
                let prop = base.create_symbol(
                    &arg_child,
                    field_name,
                    SymbolKind::Property,
                    SymbolOptions {
                        signature: Some(field_text),
                        visibility: Some(Visibility::Public),
                        parent_id: Some(class_symbol.id.clone()),
                        metadata: None,
                        doc_comment: None,
                        annotations: Vec::new(),
                    },
                );
                field_properties.push(prop);
            }
        }
    }

    Some((class_symbol, field_properties))
}

/// Extract require/require_relative calls
fn extract_require(base: &mut BaseExtractor, node: Node) -> Option<Symbol> {
    let arg_node = node.child_by_field_name("arguments")?;
    let string_node = arg_node
        .children(&mut arg_node.walk())
        .find(|c| c.kind() == "string")?;

    let require_path = base.get_node_text(&string_node).replace(['\'', '"'], "");
    let module_name = require_path
        .split('/')
        .next_back()
        .unwrap_or(&require_path)
        .to_string();
    let method_name = extract_method_name_from_call(node, |n| base.get_node_text(n))?;

    Some(base.create_symbol(
        &node,
        module_name,
        SymbolKind::Import,
        SymbolOptions {
            signature: Some(format!(
                "{} {}",
                method_name,
                base.get_node_text(&string_node)
            )),
            visibility: Some(Visibility::Public),
            parent_id: None,
            metadata: None,
            doc_comment: None,
            annotations: Vec::new(),
        },
    ))
}

enum RSpecBlockKind {
    Container,
    Example,
    Lifecycle,
}

fn extract_rspec_block(
    base: &mut BaseExtractor,
    node: Node,
    method_name: &str,
    parent_id: Option<&str>,
    block_kind: RSpecBlockKind,
) -> Option<Symbol> {
    let is_lifecycle = matches!(block_kind, RSpecBlockKind::Lifecycle);
    let block_name = if is_lifecycle {
        method_name.to_string()
    } else {
        extract_first_rspec_argument(base, node)?
    };

    let signature = if is_lifecycle {
        format!("{method_name}()")
    } else {
        format!("{method_name} \"{block_name}\"")
    };

    let mut metadata = HashMap::new();
    match block_kind {
        RSpecBlockKind::Container => {
            metadata.insert("test_container".to_string(), serde_json::json!(true));
        }
        RSpecBlockKind::Example => {
            metadata.insert("is_test".to_string(), serde_json::json!(true));
        }
        RSpecBlockKind::Lifecycle => {
            metadata.insert("is_test".to_string(), serde_json::json!(true));
            metadata.insert("test_lifecycle".to_string(), serde_json::json!(true));
        }
    }

    let kind = if matches!(block_kind, RSpecBlockKind::Container) {
        SymbolKind::Namespace
    } else {
        SymbolKind::Function
    };

    Some(base.create_symbol(
        &node,
        block_name,
        kind,
        SymbolOptions {
            signature: Some(signature),
            visibility: None,
            parent_id: parent_id.map(str::to_string),
            metadata: Some(metadata),
            doc_comment: None,
            annotations: Vec::new(),
        },
    ))
}

fn extract_first_rspec_argument(base: &BaseExtractor, node: Node) -> Option<String> {
    let arg_node = node.child_by_field_name("arguments")?;
    let first_arg = arg_node.children(&mut arg_node.walk()).find(|child| {
        matches!(
            child.kind(),
            "string" | "simple_symbol" | "symbol" | "constant" | "identifier" | "scope_resolution"
        )
    })?;
    let raw = base.get_node_text(&first_arg);

    Some(match first_arg.kind() {
        "string" => raw
            .trim_matches(|ch| matches!(ch, '"' | '\'' | '`'))
            .to_string(),
        "simple_symbol" | "symbol" => raw.trim_start_matches(':').to_string(),
        _ => raw,
    })
}

/// Extract attr_reader/attr_writer/attr_accessor calls.
fn extract_attr_accessor(
    base: &mut BaseExtractor,
    node: Node,
    method_name: &str,
    parent_id: Option<String>,
) -> Vec<Symbol> {
    let Some(arg_node) = node.child_by_field_name("arguments") else {
        return Vec::new();
    };
    let symbol_nodes: Vec<_> = arg_node
        .children(&mut arg_node.walk())
        .filter(|c| matches!(c.kind(), "simple_symbol" | "symbol"))
        .collect();

    symbol_nodes
        .into_iter()
        .map(|symbol_node| {
            let attr_name = base.get_node_text(&symbol_node).replace(':', "");
            let signature = format!("{} :{}", method_name, attr_name);
            base.create_symbol(
                &symbol_node,
                attr_name,
                SymbolKind::Property,
                SymbolOptions {
                    signature: Some(signature),
                    visibility: Some(Visibility::Public),
                    parent_id: parent_id.clone(),
                    metadata: None,
                    doc_comment: None,
                    annotations: Vec::new(),
                },
            )
        })
        .collect()
}

/// Extract define_method/define_singleton_method calls
fn extract_define_method(
    base: &mut BaseExtractor,
    node: Node,
    method_name: &str,
) -> Option<Symbol> {
    let arg_node = node.child_by_field_name("arguments")?;
    let name_node = arg_node
        .children(&mut arg_node.walk())
        .find(|c| matches!(c.kind(), "simple_symbol" | "symbol" | "string"))?;

    let dynamic_method_name = base
        .get_node_text(&name_node)
        .trim_start_matches(':')
        .trim_matches('"')
        .to_string();

    Some(base.create_symbol(
        &node,
        dynamic_method_name,
        SymbolKind::Method,
        SymbolOptions {
            signature: Some(format!(
                "{} {}",
                method_name,
                base.get_node_text(&name_node)
            )),
            visibility: Some(Visibility::Public),
            parent_id: None,
            metadata: None,
            doc_comment: None,
            annotations: Vec::new(),
        },
    ))
}

/// Extract def_delegator calls
fn extract_def_delegator(base: &mut BaseExtractor, node: Node) -> Option<Symbol> {
    let arg_node = node.child_by_field_name("arguments")?;
    let args: Vec<_> = arg_node
        .children(&mut arg_node.walk())
        .filter(|n| n.kind() != "," && n.kind() != "(" && n.kind() != ")")
        .collect();

    if args.len() >= 2 {
        let method_arg = &args[1];
        let delegated_method_name = if matches!(method_arg.kind(), "simple_symbol" | "symbol") {
            base.get_node_text(method_arg).replace(':', "")
        } else {
            return None;
        };

        Some(base.create_symbol(
            &node,
            delegated_method_name,
            SymbolKind::Method,
            SymbolOptions {
                signature: Some(format!("def_delegator {}", base.get_node_text(&arg_node))),
                visibility: Some(Visibility::Public),
                parent_id: None,
                metadata: None,
                doc_comment: None,
                annotations: Vec::new(),
            },
        ))
    } else {
        None
    }
}
