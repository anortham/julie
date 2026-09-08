#[test]
fn consumer_extension_registry_covers_every_upstream_extension() {
    let actual = julie_core::file_policy::supported_extensions_for_indexing();
    for language in julie_extractors::capability_snapshot().languages() {
        for extension in &language.extensions {
            assert!(
                actual.contains(&extension.to_lowercase()),
                "{} {}",
                language.language,
                extension
            );
        }
    }
    assert_eq!(
        julie_core::file_policy::detect_language_for_indexing_with_content(
            std::path::Path::new("sample.fs"),
            "module Sample\nlet answer = 42\n"
        ),
        "fsharp"
    );
}
