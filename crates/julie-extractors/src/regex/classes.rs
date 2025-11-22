/// Check if a pattern represents a negated character class
pub(crate) fn is_negated_class(class_text: &str) -> bool {
    class_text.starts_with("[^")
}
