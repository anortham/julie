use crate::base::{AnnotationMarker, SymbolOptions, normalize_annotations};

fn marker(
    annotation: &str,
    annotation_key: &str,
    raw_text: &str,
    carrier: Option<&str>,
) -> AnnotationMarker {
    AnnotationMarker {
        annotation: annotation.to_string(),
        annotation_key: annotation_key.to_string(),
        raw_text: Some(raw_text.to_string()),
        carrier: carrier.map(str::to_string),
    }
}

#[test]
fn annotation_normalization_covers_language_contract_examples() {
    let cases = [
        (
            "python",
            vec!["@app.route(\"/api\")"],
            vec![marker(
                "app.route",
                "app.route",
                "app.route(\"/api\")",
                None,
            )],
        ),
        (
            "csharp",
            vec!["[Authorize, Route(\"api\")]"],
            vec![
                marker("Authorize", "authorize", "Authorize", None),
                marker("Route", "route", "Route(\"api\")", None),
            ],
        ),
        (
            "rust",
            vec!["#[derive(Debug, Clone)]", "#[tokio::test]"],
            vec![
                marker("Debug", "debug", "Debug", Some("derive")),
                marker("Clone", "clone", "Clone", Some("derive")),
                marker("tokio::test", "tokio::test", "tokio::test", None),
            ],
        ),
        (
            "java",
            vec!["@org.junit.jupiter.api.Test"],
            vec![marker("Test", "test", "org.junit.jupiter.api.Test", None)],
        ),
        (
            "vbnet",
            vec!["<TestMethodAttribute>"],
            vec![marker(
                "TestMethodAttribute",
                "testmethod",
                "TestMethodAttribute",
                None,
            )],
        ),
        (
            "cpp",
            vec!["[[nodiscard, likely]]"],
            vec![
                marker("nodiscard", "nodiscard", "nodiscard", None),
                marker("likely", "likely", "likely", None),
            ],
        ),
        (
            "dart",
            vec!["@pragma('vm:prefer-inline')"],
            vec![marker(
                "pragma",
                "pragma",
                "pragma('vm:prefer-inline')",
                None,
            )],
        ),
        (
            "elixir",
            vec!["@moduledoc"],
            vec![marker("moduledoc", "moduledoc", "moduledoc", None)],
        ),
    ];

    for (language, raw_texts, expected) in cases {
        assert_eq!(
            normalize_annotations(&raw_texts, language),
            expected,
            "{language} annotations should normalize"
        );
    }
}

#[test]
fn annotation_normalization_handles_all_annotation_languages_and_deduplicates_by_key() {
    let cases = [
        (
            "typescript",
            vec!["@Injectable()", "@Injectable"],
            vec![marker("Injectable", "injectable", "Injectable()", None)],
        ),
        (
            "javascript",
            vec!["@sealed"],
            vec![marker("sealed", "sealed", "sealed", None)],
        ),
        (
            "kotlin",
            vec!["@JvmStatic"],
            vec![marker("JvmStatic", "jvmstatic", "JvmStatic", None)],
        ),
        (
            "scala",
            vec!["@scala.annotation.tailrec"],
            vec![marker(
                "tailrec",
                "tailrec",
                "scala.annotation.tailrec",
                None,
            )],
        ),
        (
            "php",
            vec!["#[Route('/api'), Security]"],
            vec![
                marker("Route", "route", "Route('/api')", None),
                marker("Security", "security", "Security", None),
            ],
        ),
        (
            "swift",
            vec!["@available(iOS 16.0, *)"],
            vec![marker(
                "available",
                "available",
                "available(iOS 16.0, *)",
                None,
            )],
        ),
        (
            "powershell",
            vec!["[CmdletBindingAttribute()]"],
            vec![marker(
                "CmdletBindingAttribute",
                "cmdletbinding",
                "CmdletBindingAttribute()",
                None,
            )],
        ),
        (
            "gdscript",
            vec!["@export_range(0, 10)"],
            vec![marker(
                "export_range",
                "export_range",
                "export_range(0, 10)",
                None,
            )],
        ),
    ];

    for (language, raw_texts, expected) in cases {
        assert_eq!(
            normalize_annotations(&raw_texts, language),
            expected,
            "{language} annotations should normalize"
        );
    }
}

#[test]
fn annotation_normalization_strips_attribute_suffix_only_from_matching_keys() {
    let cases = [
        (
            "csharp",
            "[TestMethodAttribute]",
            marker(
                "TestMethodAttribute",
                "testmethod",
                "TestMethodAttribute",
                None,
            ),
        ),
        (
            "vbnet",
            "<TestMethodAttribute>",
            marker(
                "TestMethodAttribute",
                "testmethod",
                "TestMethodAttribute",
                None,
            ),
        ),
        (
            "powershell",
            "[OutputTypeAttribute([string])]",
            marker(
                "OutputTypeAttribute",
                "outputtype",
                "OutputTypeAttribute([string])",
                None,
            ),
        ),
    ];

    for (language, raw_text, expected) in cases {
        assert_eq!(
            normalize_annotations(&[raw_text], language),
            vec![expected],
            "{language} should preserve display text while stripping Attribute from the key"
        );
    }
}

#[test]
fn annotation_normalization_symbol_options_default_has_empty_annotations() {
    assert!(SymbolOptions::default().annotations.is_empty());
}
