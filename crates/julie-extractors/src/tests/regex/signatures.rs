// Inline tests extracted from extractors/regex/signatures.rs

#[cfg(test)]
mod tests {
    use crate::regex::signatures::{
        build_character_class_signature, build_pattern_signature,
    };

    #[test]
    fn test_build_pattern_signature() {
        assert_eq!(build_pattern_signature("abc"), "abc");
        let long = "a".repeat(150);
        assert!(build_pattern_signature(&long).ends_with("..."));
    }

    #[test]
    fn test_build_character_class_signature() {
        assert_eq!(
            build_character_class_signature("[a-z]"),
            "Character class: [a-z]"
        );
    }
}
