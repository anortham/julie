use super::helpers::extract_method_name_from_call;
/// Special method call extraction for Ruby
/// Handles require, attr_accessor, define_method, def_delegator, module_function, Struct.new
use crate::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use tree_sitter::Node;

/// Extract special method calls that create symbols
pub(super) fn extract_call(base: &mut BaseExtractor, node: Node) -> Option<Symbol> {
    let method_name = extract_method_name_from_call(node, |n| base.get_node_text(n))?;

    match method_name.as_str() {
        "require" | "require_relative" => extract_require(base, node),
        "attr_reader" | "attr_writer" | "attr_accessor" => {
            extract_attr_accessor(base, node, &method_name)
        }
        "define_method" | "define_singleton_method" => {
            extract_define_method(base, node, &method_name)
        }
        "def_delegator" => extract_def_delegator(base, node),
        "module_function" => None, // Recognized but no separate symbol needed
        _ => None,
    }
}

/// Try to extract a Struct.new assignment as a Class symbol.
///
/// Called from the traversal when we encounter an assignment node.
/// Detects `Name = Struct.new(:field1, :field2, ...)` and returns a Class symbol.
/// Returns None if the assignment RHS is not a Struct.new call.
pub(super) fn try_extract_struct_new(
    base: &mut BaseExtractor,
    assignment_node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
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
    let call_text = base.get_node_text(&right_side);
    // Strip the do_block from the signature if present (keep just the Struct.new(...) part)
    let signature = if let Some(arg_list) = right_side.child_by_field_name("arguments") {
        format!("{} = Struct.new{}", name, base.get_node_text(&arg_list))
    } else {
        format!("{} = {}", name, call_text)
    };

    // Extract doc comment from the assignment node
    let doc_comment = base.find_doc_comment(&assignment_node);

    Some(base.create_symbol(
        &assignment_node,
        name,
        SymbolKind::Class,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id,
            metadata: None,
            doc_comment,
        },
    ))
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
        },
    ))
}

/// Extract attr_reader/attr_writer/attr_accessor calls
fn extract_attr_accessor(
    base: &mut BaseExtractor,
    node: Node,
    method_name: &str,
) -> Option<Symbol> {
    let arg_node = node.child_by_field_name("arguments")?;
    let symbol_nodes: Vec<_> = arg_node
        .children(&mut arg_node.walk())
        .filter(|c| matches!(c.kind(), "simple_symbol" | "symbol"))
        .collect();

    if let Some(first_symbol) = symbol_nodes.first() {
        let attr_name = base.get_node_text(first_symbol).replace(':', "");
        let signature = format!("{} :{}", method_name, attr_name);
        Some(base.create_symbol(
            &node,
            attr_name,
            SymbolKind::Property,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: None,
                metadata: None,
                doc_comment: None,
            },
        ))
    } else {
        None
    }
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
            },
        ))
    } else {
        None
    }
}
