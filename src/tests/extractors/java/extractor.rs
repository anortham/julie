/// Inline tests extracted from extractors/java/mod.rs
///
/// Tests for Java extractor initialization and basic functionality.
/// Ported from original inline tests in the JavaExtractor implementation.

use crate::extractors::java::JavaExtractor;

#[test]
fn test_java_extractor_initialization() {
    let extractor = JavaExtractor::new(
        "java".to_string(),
        "Test.java".to_string(),
        "class Test {}".to_string(),
    );
    assert_eq!(extractor.base().file_path, "Test.java");
}
