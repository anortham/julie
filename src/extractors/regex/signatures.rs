use super::groups;
use crate::extractors::base::BaseExtractor;

/// Build signature for a basic pattern
pub fn build_pattern_signature(pattern: &str) -> String {
    // Safely truncate UTF-8 string at character boundary
    BaseExtractor::truncate_string(pattern, 97)
}

/// Build signature for a character class
pub fn build_character_class_signature(class_text: &str) -> String {
    format!("Character class: {}", class_text)
}

/// Build signature for a group
pub(super) fn build_group_signature(group_text: &str) -> String {
    if let Some(group_name) = groups::extract_group_name(group_text) {
        format!("Named group '{}': {}", group_name, group_text)
    } else {
        format!("Group: {}", group_text)
    }
}

/// Build signature for a quantifier
pub(super) fn build_quantifier_signature(quantifier_text: &str) -> String {
    format!("Quantifier: {}", quantifier_text)
}

/// Build signature for an anchor
pub(super) fn build_anchor_signature(anchor_text: &str, anchor_type: &str) -> String {
    format!("Anchor ({}): {}", anchor_type, anchor_text)
}

/// Build signature for a lookaround
pub(super) fn build_lookaround_signature(
    lookaround_text: &str,
    direction: &str,
    polarity: &str,
) -> String {
    format!("{} {}: {}", polarity, direction, lookaround_text)
}

/// Build signature for an alternation
pub(super) fn build_alternation_signature(alternation_text: &str) -> String {
    format!("Alternation: {}", alternation_text)
}

/// Build signature for a predefined class
pub(super) fn build_predefined_class_signature(class_text: &str, category: &str) -> String {
    format!("Predefined class ({}): {}", category, class_text)
}

/// Build signature for a unicode property
pub(super) fn build_unicode_property_signature(property_text: &str, property: &str) -> String {
    format!("Unicode property ({}): {}", property, property_text)
}

/// Build signature for a backreference
pub(super) fn build_backreference_signature(
    backref_text: &str,
    group_name: Option<&str>,
    group_number: Option<&str>,
) -> String {
    if let Some(name) = group_name {
        format!("Named backreference to '{}': {}", name, backref_text)
    } else if let Some(number) = group_number {
        format!("Backreference to group {}: {}", number, backref_text)
    } else {
        format!("Backreference: {}", backref_text)
    }
}

/// Build signature for a conditional
pub(super) fn build_conditional_signature(conditional_text: &str, condition: &str) -> String {
    format!("Conditional ({}): {}", condition, conditional_text)
}

// REMOVED (2025-10-31): build_atomic_group_signature() - Dead code
// extract_atomic_group() was unreachable, so this helper is also unreachable

/// Build signature for a literal
pub(super) fn build_literal_signature(literal_text: &str) -> String {
    format!("Literal: {}", literal_text)
}

/// Build signature for a generic pattern
pub(super) fn build_generic_signature(pattern_text: &str) -> String {
    pattern_text.to_string()
}
