// Dart Extractor Inline Tests
//
// This module contains inline tests extracted from src/extractors/dart/mod.rs
// Original location: src/extractors/dart/mod.rs lines 436-449
//
// Extraction rationale:
// - Consolidates all inline tests into the centralized test infrastructure
// - Reduces clutter in production code
// - Makes the test module discoverable through the test registry
// - Improves test organization and maintainability

use crate::extractors::dart::DartExtractor;

#[test]
fn test_dart_extractor_creation() {
    let extractor = DartExtractor::new(
        "dart".to_string(),
        "test.dart".to_string(),
        "void main() {}".to_string(),
    );
    assert_eq!(extractor.base.language, "dart");
}
