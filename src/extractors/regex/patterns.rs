use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use serde_json::Value;
use std::collections::HashMap;
use tree_sitter::Node;

use super::{classes, flags, groups, helpers, signatures};

/// Create metadata with JSON values
pub(super) fn create_metadata(pairs: &[(&str, &str)]) -> HashMap<String, Value> {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), Value::String(value.to_string())))
        .collect()
}

/// Extract a basic pattern symbol
pub(super) fn extract_pattern(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let pattern_text = base.get_node_text(&node);
    let signature = signatures::build_pattern_signature(&pattern_text);
    let symbol_kind = helpers::determine_pattern_kind(&pattern_text);

    let metadata = create_metadata(&[
        ("type", "regex-pattern"),
        ("pattern", &pattern_text),
        (
            "complexity",
            &helpers::calculate_complexity(&pattern_text).to_string(),
        ),
    ]);

    let doc_comment = base.find_doc_comment(&node);

    Some(base.create_symbol(
        &node,
        pattern_text,
        symbol_kind,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id,
            metadata: Some(metadata),
            doc_comment,
        },
    ))
}

/// Extract a character class symbol
pub(super) fn extract_character_class(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let class_text = base.get_node_text(&node);
    let signature = signatures::build_character_class_signature(&class_text);

    let metadata = create_metadata(&[
        ("type", "character-class"),
        ("pattern", &class_text),
        (
            "negated",
            &classes::is_negated_class(&class_text).to_string(),
        ),
    ]);

    let doc_comment = base.find_doc_comment(&node);

    Some(base.create_symbol(
        &node,
        class_text,
        SymbolKind::Class,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id,
            metadata: Some(metadata),
            doc_comment,
        },
    ))
}

/// Extract a group symbol
pub(super) fn extract_group(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let group_text = base.get_node_text(&node);
    let signature = signatures::build_group_signature(&group_text);

    let mut metadata = create_metadata(&[
        ("type", "group"),
        ("pattern", &group_text),
        (
            "capturing",
            &groups::is_capturing_group(&group_text).to_string(),
        ),
    ]);

    if let Some(name) = groups::extract_group_name(&group_text) {
        metadata.insert("named".to_string(), Value::String(name));
    }

    let doc_comment = base.find_doc_comment(&node);

    Some(base.create_symbol(
        &node,
        group_text,
        SymbolKind::Class,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id,
            metadata: Some(metadata),
            doc_comment,
        },
    ))
}

/// Extract a quantifier symbol
pub(super) fn extract_quantifier(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let quantifier_text = base.get_node_text(&node);
    let signature = signatures::build_quantifier_signature(&quantifier_text);

    let metadata = create_metadata(&[
        ("type", "quantifier"),
        ("pattern", &quantifier_text),
        ("lazy", &quantifier_text.contains('?').to_string()),
        ("possessive", &quantifier_text.contains('+').to_string()),
    ]);

    let doc_comment = base.find_doc_comment(&node);

    Some(base.create_symbol(
        &node,
        quantifier_text,
        SymbolKind::Function,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id,
            metadata: Some(metadata),
            doc_comment,
        },
    ))
}

/// Extract an anchor symbol
pub(super) fn extract_anchor(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let anchor_text = base.get_node_text(&node);
    let anchor_type = flags::get_anchor_type(&anchor_text);
    let signature = signatures::build_anchor_signature(&anchor_text, &anchor_type);

    let metadata = create_metadata(&[
        ("type", "anchor"),
        ("pattern", &anchor_text),
        ("position", &anchor_type),
    ]);

    let doc_comment = base.find_doc_comment(&node);

    Some(base.create_symbol(
        &node,
        anchor_text,
        SymbolKind::Constant,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id,
            metadata: Some(metadata),
            doc_comment,
        },
    ))
}

/// Extract a lookaround symbol
pub(super) fn extract_lookaround(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let lookaround_text = base.get_node_text(&node);
    let direction = flags::get_lookaround_direction(&lookaround_text);
    let polarity = if flags::is_positive_lookaround(&lookaround_text) {
        "positive"
    } else {
        "negative"
    };
    let signature = signatures::build_lookaround_signature(&lookaround_text, &direction, polarity);

    let metadata = create_metadata(&[
        ("type", "lookaround"),
        ("pattern", &lookaround_text),
        ("direction", &direction),
        (
            "positive",
            &flags::is_positive_lookaround(&lookaround_text).to_string(),
        ),
    ]);

    let doc_comment = base.find_doc_comment(&node);

    Some(base.create_symbol(
        &node,
        lookaround_text,
        SymbolKind::Method,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id,
            metadata: Some(metadata),
            doc_comment,
        },
    ))
}

/// Extract an alternation symbol
pub(super) fn extract_alternation(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let alternation_text = base.get_node_text(&node);
    let signature = signatures::build_alternation_signature(&alternation_text);

    let metadata = create_metadata(&[
        ("type", "alternation"),
        ("pattern", &alternation_text),
        (
            "options",
            &flags::extract_alternation_options(&alternation_text).join(","),
        ),
    ]);

    let doc_comment = base.find_doc_comment(&node);

    Some(base.create_symbol(
        &node,
        alternation_text,
        SymbolKind::Variable,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id,
            metadata: Some(metadata),
            doc_comment,
        },
    ))
}

/// Extract a predefined character class symbol
pub(super) fn extract_predefined_class(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let class_text = base.get_node_text(&node);
    let category = flags::get_predefined_class_category(&class_text);
    let signature = signatures::build_predefined_class_signature(&class_text, &category);

    let metadata = create_metadata(&[
        ("type", "predefined-class"),
        ("pattern", &class_text),
        ("category", &category),
    ]);

    let doc_comment = base.find_doc_comment(&node);

    Some(base.create_symbol(
        &node,
        class_text,
        SymbolKind::Constant,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id,
            metadata: Some(metadata),
            doc_comment,
        },
    ))
}

/// Extract a unicode property symbol
pub(super) fn extract_unicode_property(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let property_text = base.get_node_text(&node);
    let property = flags::extract_unicode_property_name(&property_text);
    let signature = signatures::build_unicode_property_signature(&property_text, &property);

    let metadata = create_metadata(&[
        ("type", "unicode-property"),
        ("pattern", &property_text),
        ("property", &property),
    ]);

    let doc_comment = base.find_doc_comment(&node);

    Some(base.create_symbol(
        &node,
        property_text,
        SymbolKind::Constant,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id,
            metadata: Some(metadata),
            doc_comment,
        },
    ))
}

/// Extract a backreference symbol
pub(super) fn extract_backreference(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let backref_text = base.get_node_text(&node);
    let group_number = flags::extract_group_number(&backref_text);
    let group_name = flags::extract_backref_group_name(&backref_text);
    let signature = signatures::build_backreference_signature(
        &backref_text,
        group_name.as_deref(),
        group_number.as_deref(),
    );

    let mut metadata = create_metadata(&[("type", "backreference"), ("pattern", &backref_text)]);

    if let Some(num) = group_number {
        metadata.insert("groupNumber".to_string(), Value::String(num));
    }

    if let Some(name) = group_name {
        metadata.insert("groupName".to_string(), Value::String(name));
    }

    let doc_comment = base.find_doc_comment(&node);

    Some(base.create_symbol(
        &node,
        backref_text,
        SymbolKind::Variable,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id,
            metadata: Some(metadata),
            doc_comment,
        },
    ))
}

/// Extract a conditional symbol
pub(super) fn extract_conditional(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let conditional_text = base.get_node_text(&node);
    let condition = flags::extract_condition(&conditional_text);
    let signature = signatures::build_conditional_signature(&conditional_text, &condition);

    let metadata = create_metadata(&[
        ("type", "conditional"),
        ("pattern", &conditional_text),
        ("condition", &condition),
    ]);

    let doc_comment = base.find_doc_comment(&node);

    Some(base.create_symbol(
        &node,
        conditional_text,
        SymbolKind::Method,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id,
            metadata: Some(metadata),
            doc_comment,
        },
    ))
}

// REMOVED (2025-10-31): extract_atomic_group() - Unreachable dead code
// Tree-sitter regex parser does NOT generate "atomic_group" nodes for (?> ...) syntax
// These are parsed as ERROR nodes instead, making this function unreachable
// See: src/tests/extractors/regex/advanced_features.rs for ERROR node handling tests

// REMOVED (2025-10-31): extract_comment() - Unreachable dead code
// Tree-sitter regex parser does NOT generate "comment" nodes for (?# ...) syntax
// These are parsed as ERROR nodes + individual pattern_character nodes instead
// See: src/tests/extractors/regex/advanced_features.rs for ERROR node handling tests

/// Extract a literal symbol
pub(super) fn extract_literal(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let literal_text = base.get_node_text(&node);
    let signature = signatures::build_literal_signature(&literal_text);

    let metadata = create_metadata(&[
        ("type", "literal"),
        ("pattern", &literal_text),
        (
            "escaped",
            &helpers::is_escaped_literal(&literal_text).to_string(),
        ),
    ]);

    let doc_comment = base.find_doc_comment(&node);

    Some(base.create_symbol(
        &node,
        literal_text,
        SymbolKind::Variable,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id,
            metadata: Some(metadata),
            doc_comment,
        },
    ))
}

/// Extract a generic pattern symbol
pub(super) fn extract_generic_pattern(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let pattern_text = base.get_node_text(&node);
    let signature = signatures::build_generic_signature(&pattern_text);
    let symbol_kind = helpers::determine_pattern_kind(&pattern_text);

    let metadata = create_metadata(&[
        ("type", "generic-pattern"),
        ("pattern", &pattern_text),
        ("nodeType", node.kind()),
    ]);

    let doc_comment = base.find_doc_comment(&node);

    Some(base.create_symbol(
        &node,
        pattern_text,
        symbol_kind,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id,
            metadata: Some(metadata),
            doc_comment,
        },
    ))
}
