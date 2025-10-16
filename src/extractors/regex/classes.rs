/// Check if a pattern represents a negated character class
pub(super) fn is_negated_class(class_text: &str) -> bool {
    class_text.starts_with("[^")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_negated_class() {
        assert!(is_negated_class("[^a-z]"));
        assert!(!is_negated_class("[a-z]"));
    }
}
