// Inline tests extracted from src/extractors/rust/mod.rs
//
// This module contains tests for the main Rust extractor:
// - RustExtractor creation and initialization
// - Two-phase extraction (phase 1: all symbols, phase 2: impl blocks)
// - Tree walking and symbol extraction orchestration

#[cfg(test)]
mod tests {
    use crate::extractors::rust::RustExtractor;
    use std::path::PathBuf;

    #[test]
    fn test_rust_extractor_creation() {
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = RustExtractor::new(
            "rust".to_string(),
            "test.rs".to_string(),
            "fn main() {}".to_string(),
            &workspace_root,
        );
        assert_eq!(extractor.get_base_mut().file_path, "test.rs");
    }
}
