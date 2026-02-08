use super::groups;
use crate::base::BaseExtractor;

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

/// Build signature for a lookaround
pub(super) fn build_lookaround_signature(
    lookaround_text: &str,
    direction: &str,
    polarity: &str,
) -> String {
    format!("{} {}: {}", polarity, direction, lookaround_text)
}

/// Build signature for a unicode property
pub(super) fn build_unicode_property_signature(property_text: &str, property: &str) -> String {
    format!("Unicode property ({}): {}", property, property_text)
}

/// Build signature for a conditional
pub(super) fn build_conditional_signature(conditional_text: &str, condition: &str) -> String {
    format!("Conditional ({}): {}", condition, conditional_text)
}

// REMOVED (2025-10-31): build_atomic_group_signature() - Dead code
// extract_atomic_group() was unreachable, so this helper is also unreachable
