use crate::base::{RelationshipKind, Symbol, SymbolKind, SymbolOptions, UnresolvedTarget};
use crate::r::RExtractor;
use std::collections::HashMap;
use tree_sitter::Node;

use super::text_args::{
    argument_list_text, clean_r_name, function_signature, split_top_level_arguments,
};

pub(super) fn assignment_name(extractor: &RExtractor, left: Node) -> Option<String> {
    match left.kind() {
        "identifier" | "string" | "string_content" => {
            clean_r_name(&extractor.base.get_node_text(&left))
        }
        _ => clean_r_name(&extractor.base.get_node_text(&left)),
    }
}

pub(super) fn is_container_assignment(extractor: &RExtractor, left: Node, right: Node) -> bool {
    if right.kind() != "call" {
        return false;
    }

    let Some(name) = assignment_name(extractor, left) else {
        return false;
    };
    let Some(call_name) = call_name(extractor, right) else {
        return false;
    };

    matches!(name.as_str(), "public" | "private" | "fields" | "methods") && call_name == "list"
}

pub(super) fn extract_assignment_class_factory(
    extractor: &mut RExtractor,
    node: Node,
    assigned_name: &str,
    call: Node,
    parent_id: &Option<String>,
) -> Option<Symbol> {
    let call_name = call_name(extractor, call)?;
    let class_system = match call_name.as_str() {
        "R6Class" => "R6",
        "setRefClass" => "ReferenceClass",
        _ => return None,
    };

    let args = call.child(1);
    let class_name = args
        .and_then(|args| positional_string_argument(extractor, args, 0))
        .unwrap_or_else(|| assigned_name.to_string());

    let mut metadata = HashMap::new();
    metadata.insert(
        "r_class_system".to_string(),
        serde_json::Value::String(class_system.to_string()),
    );
    if let Some(inherits) = args.and_then(|args| named_argument_value(extractor, args, "inherit")) {
        metadata.insert("inherit".to_string(), serde_json::Value::String(inherits));
    }

    let symbol = extractor.base.create_symbol(
        &node,
        assigned_name.to_string(),
        SymbolKind::Class,
        SymbolOptions {
            parent_id: parent_id.clone(),
            signature: Some(format!("{assigned_name} <- {call_name}(\"{class_name}\")")),
            metadata: Some(metadata),
            doc_comment: extractor.base.find_doc_comment(&node),
            ..Default::default()
        },
    );
    extractor.symbols.push(symbol.clone());
    extract_class_list_members(extractor, call, &symbol, class_system);
    Some(symbol)
}

pub(super) fn extract_s4_call(
    extractor: &mut RExtractor,
    node: Node,
    parent_id: &Option<String>,
) -> Option<Symbol> {
    let call_name = call_name(extractor, node)?;
    match call_name.as_str() {
        "setClass" => extract_s4_class(extractor, node, parent_id),
        "setGeneric" => extract_s4_generic(extractor, node, parent_id),
        "setMethod" => extract_s4_method(extractor, node, parent_id),
        _ => None,
    }
}

pub(super) fn extract_import_call(
    extractor: &mut RExtractor,
    node: Node,
    parent_id: &Option<String>,
) -> Option<Symbol> {
    let func_node = node.child(0)?;
    if func_node.kind() != "identifier" {
        return None;
    }
    let func_name = extractor.base.get_node_text(&func_node);
    if func_name != "library" && func_name != "require" && func_name != "source" {
        return None;
    }

    let args_node = node.child(1)?;
    let import_name = first_import_argument(extractor, args_node)?;
    if import_name.is_empty() {
        return None;
    }

    let signature = if func_name == "source" {
        extractor.base.get_node_text(&node)
    } else {
        format!("{}({})", func_name, import_name)
    };
    let symbol = extractor.base.create_symbol(
        &node,
        import_name,
        SymbolKind::Import,
        SymbolOptions {
            parent_id: parent_id.clone(),
            signature: Some(signature),
            doc_comment: extractor.base.find_doc_comment(&node),
            ..Default::default()
        },
    );
    extractor.symbols.push(symbol.clone());
    if func_name == "source" {
        emit_source_import_pending(extractor, &symbol, node);
    }
    Some(symbol)
}

pub(super) fn member_metadata(
    extractor: &RExtractor,
    node: Node,
    parent_id: &Option<String>,
) -> HashMap<String, serde_json::Value> {
    let mut metadata = HashMap::new();
    if let Some(parent_id) = parent_id {
        if let Some(parent) = extractor
            .symbols
            .iter()
            .find(|symbol| symbol.id == *parent_id)
        {
            if let Some(class_system) = parent
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("r_class_system"))
                .and_then(|value| value.as_str())
            {
                metadata.insert(
                    "r_class_system".to_string(),
                    serde_json::Value::String(class_system.to_string()),
                );
            }
        }
    }
    if let Some(visibility) = enclosing_member_visibility(extractor, node) {
        metadata.insert(
            "member_visibility".to_string(),
            serde_json::Value::String(visibility),
        );
    }
    metadata
}

fn first_import_argument(extractor: &RExtractor, args_node: Node) -> Option<String> {
    let text = extractor.base.get_node_text(&args_node);
    split_top_level_arguments(&argument_list_text(&text))
        .into_iter()
        .find_map(|argument| {
            if argument.contains('=') {
                None
            } else {
                clean_r_name(&argument)
            }
        })
}

pub(super) fn emit_source_import_pending(extractor: &mut RExtractor, symbol: &Symbol, node: Node) {
    let target = UnresolvedTarget {
        display_name: symbol.name.clone(),
        terminal_name: symbol.name.clone(),
        receiver: None,
        namespace_path: Vec::new(),
        import_context: None,
    };
    let pending = extractor.base.create_pending_relationship(
        symbol.id.clone(),
        target,
        RelationshipKind::Imports,
        &node,
        Some(symbol.id.clone()),
        Some(1.0),
    );
    extractor.add_structured_pending_relationship(pending);
}

fn extract_s4_class(
    extractor: &mut RExtractor,
    node: Node,
    parent_id: &Option<String>,
) -> Option<Symbol> {
    let args = node.child(1)?;
    let name = positional_string_argument(extractor, args, 0)?;
    let mut metadata = s4_metadata("class");
    if let Some(slots) = named_c_argument_names(extractor, args, "slots") {
        metadata.insert(
            "slots".to_string(),
            serde_json::Value::String(slots.join(",")),
        );
    }
    if let Some(contains) = named_string_argument(extractor, args, "contains") {
        metadata.insert("contains".to_string(), serde_json::Value::String(contains));
    }

    let symbol = extractor.base.create_symbol(
        &node,
        name.clone(),
        SymbolKind::Class,
        SymbolOptions {
            parent_id: parent_id.clone(),
            signature: Some(format!("setClass(\"{name}\")")),
            metadata: Some(metadata),
            doc_comment: extractor.base.find_doc_comment(&node),
            ..Default::default()
        },
    );
    extractor.symbols.push(symbol.clone());
    Some(symbol)
}

fn extract_s4_generic(
    extractor: &mut RExtractor,
    node: Node,
    parent_id: &Option<String>,
) -> Option<Symbol> {
    let args = node.child(1)?;
    let name = positional_string_argument(extractor, args, 0)?;
    let symbol = extractor.base.create_symbol(
        &node,
        name.clone(),
        SymbolKind::Function,
        SymbolOptions {
            parent_id: parent_id.clone(),
            signature: Some(format!("setGeneric(\"{name}\")")),
            metadata: Some(s4_metadata("generic")),
            doc_comment: extractor.base.find_doc_comment(&node),
            ..Default::default()
        },
    );
    extractor.symbols.push(symbol.clone());
    Some(symbol)
}

fn extract_s4_method(
    extractor: &mut RExtractor,
    node: Node,
    parent_id: &Option<String>,
) -> Option<Symbol> {
    let args = node.child(1)?;
    let generic = positional_string_argument(extractor, args, 0)?;
    let class_name = positional_string_argument(extractor, args, 1).unwrap_or_default();
    let name = if class_name.is_empty() {
        generic.clone()
    } else {
        format!("{generic},{class_name}")
    };
    let mut metadata = s4_metadata("method");
    metadata.insert(
        "s4_generic".to_string(),
        serde_json::Value::String(generic.clone()),
    );
    if !class_name.is_empty() {
        metadata.insert(
            "s4_class".to_string(),
            serde_json::Value::String(class_name.clone()),
        );
    }

    let symbol = extractor.base.create_symbol(
        &node,
        name,
        SymbolKind::Method,
        SymbolOptions {
            parent_id: parent_id.clone(),
            signature: Some(format!("setMethod(\"{generic}\", \"{class_name}\")")),
            metadata: Some(metadata),
            doc_comment: extractor.base.find_doc_comment(&node),
            ..Default::default()
        },
    );
    extractor.symbols.push(symbol.clone());
    Some(symbol)
}

fn s4_metadata(role: &str) -> HashMap<String, serde_json::Value> {
    let mut metadata = HashMap::new();
    metadata.insert(
        "r_class_system".to_string(),
        serde_json::Value::String("S4".to_string()),
    );
    metadata.insert(
        "s4_role".to_string(),
        serde_json::Value::String(role.to_string()),
    );
    metadata
}

fn enclosing_member_visibility(extractor: &RExtractor, node: Node) -> Option<String> {
    let mut current = node.parent();
    for _ in 0..8 {
        let parent = current?;
        if parent.kind() == "binary_operator" {
            if let Some(left) = parent.child(0) {
                let text = assignment_name(extractor, left)?;
                if text == "public" || text == "private" {
                    return Some(text);
                }
            }
        }
        current = parent.parent();
    }
    None
}

fn call_name(extractor: &RExtractor, call: Node) -> Option<String> {
    let callee = call.child(0)?;
    if callee.kind() == "identifier" {
        clean_r_name(&extractor.base.get_node_text(&callee))
    } else {
        None
    }
}

fn positional_string_argument(extractor: &RExtractor, args: Node, index: usize) -> Option<String> {
    split_top_level_arguments(argument_list_text(&extractor.base.get_node_text(&args)).as_str())
        .into_iter()
        .filter(|argument| !argument.contains('='))
        .nth(index)
        .and_then(|argument| clean_r_name(&argument))
}

fn named_string_argument(extractor: &RExtractor, args: Node, name: &str) -> Option<String> {
    named_argument_value(extractor, args, name).and_then(|text| clean_r_name(&text))
}

fn named_argument_value(extractor: &RExtractor, args: Node, name: &str) -> Option<String> {
    find_named_argument_value(extractor, args, name)
}

fn find_named_argument_value(extractor: &RExtractor, node: Node, name: &str) -> Option<String> {
    if node.kind() == "argument" {
        let argument_name = node.child_by_field_name("name")?;
        let value = node.child_by_field_name("value")?;
        if assignment_name(extractor, argument_name).as_deref() == Some(name) {
            return Some(extractor.base.get_node_text(&value));
        }
    }

    if node.kind() == "binary_operator" {
        let left = node.child(0)?;
        let op = node.child(1)?;
        let right = node.child(2)?;
        if extractor.base.get_node_text(&op) == "="
            && assignment_name(extractor, left).as_deref() == Some(name)
        {
            return Some(extractor.base.get_node_text(&right));
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(value) = find_named_argument_value(extractor, child, name) {
            return Some(value);
        }
    }
    None
}

fn named_c_argument_names(extractor: &RExtractor, args: Node, name: &str) -> Option<Vec<String>> {
    let text = named_argument_value(extractor, args, name)?;
    let start = text.find('(')? + 1;
    let end = text.rfind(')')?;
    let inner = &text[start..end];
    let names = inner
        .split(',')
        .filter_map(|entry| {
            entry
                .split_once('=')
                .map(|(slot, _)| slot.trim().to_string())
        })
        .filter(|slot| !slot.is_empty())
        .collect::<Vec<_>>();
    if names.is_empty() { None } else { Some(names) }
}

fn extract_class_list_members(
    extractor: &mut RExtractor,
    call: Node,
    class_symbol: &Symbol,
    class_system: &str,
) {
    for visibility in ["public", "private", "fields", "methods"] {
        let Some(body) = named_list_body(&extractor.base.get_node_text(&call), visibility) else {
            continue;
        };
        let member_visibility = match visibility {
            "private" => "private",
            _ => "public",
        };
        for entry in split_top_level_arguments(&body) {
            let Some((raw_name, value)) = entry.split_once('=') else {
                continue;
            };
            let Some(name) = clean_r_name(raw_name) else {
                continue;
            };
            let value = value.trim();
            let is_method = value.starts_with("function");
            let kind = if is_method {
                SymbolKind::Method
            } else {
                SymbolKind::Field
            };
            let mut metadata = HashMap::new();
            metadata.insert(
                "r_class_system".to_string(),
                serde_json::Value::String(class_system.to_string()),
            );
            metadata.insert(
                "member_visibility".to_string(),
                serde_json::Value::String(member_visibility.to_string()),
            );
            let signature = if is_method {
                Some(format!("{} = {}", name, function_signature(value)))
            } else {
                Some(format!("{name} = {value}"))
            };
            let symbol = extractor.base.create_symbol(
                &call,
                name,
                kind,
                SymbolOptions {
                    parent_id: Some(class_symbol.id.clone()),
                    signature,
                    metadata: Some(metadata),
                    ..Default::default()
                },
            );
            extractor.symbols.push(symbol);
        }
    }
}

fn named_list_body(call_text: &str, name: &str) -> Option<String> {
    let needle = format!("{name} = list");
    let start = call_text.find(&needle)?;
    let after_needle = &call_text[start + needle.len()..];
    let open_offset = after_needle.find('(')?;
    let body_start = start + needle.len() + open_offset + 1;
    let mut depth = 1usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for (offset, ch) in call_text[body_start..].char_indices() {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == active_quote {
                quote = None;
            }
            continue;
        }

        match ch {
            '"' | '\'' | '`' => quote = Some(ch),
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(call_text[body_start..body_start + offset].to_string());
                }
            }
            _ => {}
        }
    }
    None
}
