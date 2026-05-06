use super::parsing::{VueSection, parse_vue_sfc};
use crate::base::relationship_resolution::{StructuredPendingRelationship, UnresolvedTarget};
use crate::base::{BaseExtractor, Relationship, RelationshipKind, Symbol};
use regex::Regex;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;
use tree_sitter::{Node, Parser};

static TEMPLATE_INTERPOLATION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{\{\s*([^}]+?)\s*\}\}").unwrap());
static TEMPLATE_EVENT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"@[A-Za-z0-9:_-]+\s*=\s*"([^"]+)""#).unwrap());
static COMPONENT_TAG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<([A-Za-z][A-Za-z0-9_-]*)\b").unwrap());
static IDENTIFIER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[A-Za-z_$][A-Za-z0-9_$]*").unwrap());

pub(super) fn extract_relationships(base: &BaseExtractor, symbols: &[Symbol]) -> Vec<Relationship> {
    let Ok(sections) = parse_vue_sfc(&base.content) else {
        return Vec::new();
    };

    let local_symbols = unique_symbols_by_name(symbols);
    let Some(component) = component_symbol(symbols) else {
        return Vec::new();
    };

    let mut relationships = Vec::new();
    let mut seen = HashSet::new();

    for section in &sections {
        match section.section_type.as_str() {
            "script" => collect_script_relationships(
                base,
                section,
                component,
                &local_symbols,
                &mut relationships,
                &mut seen,
            ),
            "template" => collect_template_relationships(
                base,
                section,
                component,
                &local_symbols,
                &mut relationships,
                &mut seen,
            ),
            _ => {}
        }
    }

    relationships
}

pub(super) fn extract_structured_pending_relationships(
    base: &BaseExtractor,
    symbols: &[Symbol],
) -> Vec<StructuredPendingRelationship> {
    let Ok(sections) = parse_vue_sfc(&base.content) else {
        return Vec::new();
    };
    let Some(component) = component_symbol(symbols) else {
        return Vec::new();
    };
    let local_symbols = unique_symbols_by_name(symbols);
    let mut pending = Vec::new();
    let mut seen = HashSet::new();

    for section in &sections {
        if section.section_type != "template" {
            continue;
        }
        for (line_index, line) in section.content.lines().enumerate() {
            let line_number = section.start_line as u32 + line_index as u32 + 1;
            for captures in COMPONENT_TAG_RE.captures_iter(line) {
                let Some(tag_name) = captures.get(1).map(|matched| matched.as_str()) else {
                    continue;
                };
                if !is_component_tag(tag_name) || local_symbols.contains_key(tag_name) {
                    continue;
                }
                if !seen.insert((tag_name.to_string(), line_number)) {
                    continue;
                }
                pending.push(StructuredPendingRelationship::new(
                    component.id.clone(),
                    UnresolvedTarget::simple(tag_name),
                    Some(component.id.clone()),
                    RelationshipKind::References,
                    base.file_path.clone(),
                    line_number,
                    1.0,
                ));
            }
        }
    }

    pending
}

fn collect_script_relationships(
    base: &BaseExtractor,
    section: &VueSection,
    component: &Symbol,
    local_symbols: &HashMap<String, &Symbol>,
    relationships: &mut Vec<Relationship>,
    seen: &mut HashSet<(String, String, RelationshipKind, u32, String)>,
) {
    let Some(tree) = parse_script_section(section) else {
        return;
    };
    visit_script_node(
        base,
        tree.root_node(),
        &section.content,
        section.start_line,
        component,
        local_symbols,
        relationships,
        seen,
    );
}

fn visit_script_node(
    base: &BaseExtractor,
    node: Node,
    script_content: &str,
    start_line_offset: usize,
    component: &Symbol,
    local_symbols: &HashMap<String, &Symbol>,
    relationships: &mut Vec<Relationship>,
    seen: &mut HashSet<(String, String, RelationshipKind, u32, String)>,
) {
    if node.kind() == "call_expression" {
        if let Some(function_node) = node.child_by_field_name("function") {
            if let Some(name) = call_name(function_node, script_content) {
                if let Some(target) = local_symbols.get(&name) {
                    push_relationship(
                        base,
                        component,
                        target,
                        RelationshipKind::Calls,
                        (function_node.start_position().row + start_line_offset + 1) as u32,
                        &name,
                        seen,
                        relationships,
                    );
                }
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        visit_script_node(
            base,
            child,
            script_content,
            start_line_offset,
            component,
            local_symbols,
            relationships,
            seen,
        );
    }
}

fn collect_template_relationships(
    base: &BaseExtractor,
    section: &VueSection,
    component: &Symbol,
    local_symbols: &HashMap<String, &Symbol>,
    relationships: &mut Vec<Relationship>,
    seen: &mut HashSet<(String, String, RelationshipKind, u32, String)>,
) {
    for (line_index, line) in section.content.lines().enumerate() {
        let line_number = section.start_line as u32 + line_index as u32 + 1;
        for captures in TEMPLATE_INTERPOLATION_RE.captures_iter(line) {
            if let Some(expression) = captures.get(1) {
                collect_template_expression_relationships(
                    base,
                    expression.as_str(),
                    line_number,
                    component,
                    local_symbols,
                    relationships,
                    seen,
                );
            }
        }
        for captures in TEMPLATE_EVENT_RE.captures_iter(line) {
            if let Some(expression) = captures.get(1) {
                collect_template_expression_relationships(
                    base,
                    expression.as_str(),
                    line_number,
                    component,
                    local_symbols,
                    relationships,
                    seen,
                );
            }
        }
    }
}

fn collect_template_expression_relationships(
    base: &BaseExtractor,
    expression: &str,
    line_number: u32,
    component: &Symbol,
    local_symbols: &HashMap<String, &Symbol>,
    relationships: &mut Vec<Relationship>,
    seen: &mut HashSet<(String, String, RelationshipKind, u32, String)>,
) {
    for name in IDENTIFIER_RE
        .find_iter(expression)
        .map(|matched| matched.as_str())
        .filter(|name| !is_template_keyword(name))
    {
        if let Some(target) = local_symbols.get(name) {
            push_relationship(
                base,
                component,
                target,
                RelationshipKind::References,
                line_number,
                name,
                seen,
                relationships,
            );
        }
    }
}

fn push_relationship(
    base: &BaseExtractor,
    source: &Symbol,
    target: &Symbol,
    kind: RelationshipKind,
    line_number: u32,
    reference_name: &str,
    seen: &mut HashSet<(String, String, RelationshipKind, u32, String)>,
    relationships: &mut Vec<Relationship>,
) {
    let key = (
        source.id.clone(),
        target.id.clone(),
        kind.clone(),
        line_number,
        reference_name.to_string(),
    );
    if !seen.insert(key) {
        return;
    }

    let mut metadata = HashMap::new();
    metadata.insert(
        "referenceName".to_string(),
        Value::String(reference_name.to_string()),
    );

    relationships.push(Relationship {
        id: format!(
            "{}_{}_{:?}_{}_{}",
            source.id, target.id, kind, line_number, reference_name
        ),
        from_symbol_id: source.id.clone(),
        to_symbol_id: target.id.clone(),
        kind,
        file_path: base.file_path.clone(),
        line_number,
        confidence: 1.0,
        metadata: Some(metadata),
    });
}

fn unique_symbols_by_name(symbols: &[Symbol]) -> HashMap<String, &Symbol> {
    let mut grouped: HashMap<&str, Vec<&Symbol>> = HashMap::new();
    for symbol in symbols {
        grouped
            .entry(symbol.name.as_str())
            .or_default()
            .push(symbol);
    }
    grouped
        .into_iter()
        .filter_map(|(name, symbols)| {
            if symbols.len() == 1 {
                Some((name.to_string(), symbols[0]))
            } else {
                None
            }
        })
        .collect()
}

fn component_symbol(symbols: &[Symbol]) -> Option<&Symbol> {
    symbols.iter().find(|symbol| {
        symbol
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("type"))
            .and_then(Value::as_str)
            == Some("vue-sfc")
    })
}

fn parse_script_section(section: &VueSection) -> Option<tree_sitter::Tree> {
    let mut parser = Parser::new();
    let lang = section.lang.as_deref().unwrap_or("js");
    let tree_sitter_lang = if lang == "ts" || lang == "typescript" {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
    } else {
        tree_sitter_javascript::LANGUAGE.into()
    };
    parser.set_language(&tree_sitter_lang).ok()?;
    parser.parse(&section.content, None)
}

fn call_name(function_node: Node, script_content: &str) -> Option<String> {
    match function_node.kind() {
        "identifier" => Some(node_text(function_node, script_content)),
        "member_expression" => function_node
            .child_by_field_name("property")
            .map(|property| node_text(property, script_content)),
        _ => None,
    }
}

fn node_text(node: Node, content: &str) -> String {
    let bytes = content.as_bytes();
    let start = node.start_byte();
    let end = node.end_byte();
    if start < bytes.len() && end <= bytes.len() {
        String::from_utf8_lossy(&bytes[start..end]).to_string()
    } else {
        String::new()
    }
}

fn is_template_keyword(name: &str) -> bool {
    matches!(
        name,
        "true" | "false" | "null" | "undefined" | "if" | "else" | "return" | "typeof" | "new"
    )
}

fn is_component_tag(tag_name: &str) -> bool {
    tag_name
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
        || tag_name.contains('-')
}
