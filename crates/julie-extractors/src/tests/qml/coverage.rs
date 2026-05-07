// QML Coverage Tests
// Consolidated coverage for docs, types, visibility, bindings, and signal handlers

use super::*;
use crate::base::{SymbolKind, Visibility};

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata_str(symbol: &Symbol, key: &str) -> Option<String> {
        symbol
            .metadata
            .as_ref()
            .and_then(|m| m.get(key))
            .and_then(|v| v.as_str())
            .map(str::to_string)
    }

    #[test]
    fn test_qml_docs_types_visibility_bindings_and_signal_handlers_are_extracted() {
        let qml_code = r#"
import QtQuick 2.15

Rectangle {
    /** Count exposed to sibling components */
    property int count: 1

    /** Internal marker for implementation details */
    property string _secret: "hidden"

    /** Emitted when activation happens */
    signal activated(int value)

    /** Formats a label for display */
    function formatLabel(value): string {
        return value.toUpperCase()
    }

    /** Private helper for local arithmetic */
    function _helper(): int {
        return count + 1
    }

    /** Width tracks the count value */
    width: count * 10

    /** Signal handler for activation */
    onActivated: {
        _helper()
    }
}
"#;

        let tree = init_parser(qml_code, "qml");
        let workspace_root = std::path::PathBuf::from("/tmp/test");
        let mut extractor = QmlExtractor::new(
            "qml".to_string(),
            "test.qml".to_string(),
            qml_code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);
        let inferred_types = extractor.infer_types(&symbols);

        let count = symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Property && s.name == "count")
            .expect("Should extract property count");
        assert_eq!(count.visibility, Some(Visibility::Public));
        assert!(
            count
                .doc_comment
                .as_deref()
                .is_some_and(|doc| doc.contains("Count exposed to sibling components")),
            "count doc comment should be extracted"
        );
        assert_eq!(
            inferred_types.get(&count.id).map(String::as_str),
            Some("int"),
            "count type should be inferred from property signature"
        );

        let secret = symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Property && s.name == "_secret")
            .expect("Should extract property _secret");
        assert_eq!(secret.visibility, Some(Visibility::Private));
        assert!(
            secret
                .doc_comment
                .as_deref()
                .is_some_and(|doc| doc.contains("Internal marker for implementation details")),
            "_secret doc comment should be extracted"
        );
        assert_eq!(
            inferred_types.get(&secret.id).map(String::as_str),
            Some("string"),
            "_secret type should be inferred from property signature"
        );

        let format_label = symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Function && s.name == "formatLabel")
            .expect("Should extract function formatLabel");
        assert_eq!(format_label.visibility, Some(Visibility::Public));
        assert!(
            format_label
                .signature
                .as_deref()
                .is_some_and(|sig| sig.contains(": string")),
            "formatLabel signature should preserve typed return"
        );
        assert!(
            format_label
                .doc_comment
                .as_deref()
                .is_some_and(|doc| doc.contains("Formats a label for display")),
            "formatLabel doc comment should be extracted"
        );
        assert_eq!(
            inferred_types.get(&format_label.id).map(String::as_str),
            Some("string"),
            "formatLabel return type should be inferred from function signature"
        );

        let helper = symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Function && s.name == "_helper")
            .expect("Should extract function _helper");
        assert_eq!(helper.visibility, Some(Visibility::Private));
        assert!(
            helper
                .doc_comment
                .as_deref()
                .is_some_and(|doc| doc.contains("Private helper for local arithmetic")),
            "_helper doc comment should be extracted"
        );
        assert_eq!(
            inferred_types.get(&helper.id).map(String::as_str),
            Some("int"),
            "_helper return type should be inferred from function signature"
        );

        let width_binding = symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Property && s.name == "width")
            .expect("Should extract width binding symbol");
        assert_eq!(width_binding.visibility, Some(Visibility::Private));
        assert_eq!(
            metadata_str(width_binding, "binding_kind"),
            Some("property_binding".to_string()),
            "width binding should be tagged as property_binding"
        );
        assert!(
            width_binding
                .doc_comment
                .as_deref()
                .is_some_and(|doc| doc.contains("Width tracks the count value")),
            "width binding doc comment should be extracted"
        );

        let on_activated = symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Function && s.name == "onActivated")
            .expect("Should extract onActivated signal handler symbol");
        assert_eq!(on_activated.visibility, Some(Visibility::Private));
        assert_eq!(
            metadata_str(on_activated, "binding_kind"),
            Some("signal_handler".to_string()),
            "onActivated should be tagged as a signal handler"
        );
        assert_eq!(
            metadata_str(on_activated, "handled_signal"),
            Some("activated".to_string()),
            "onActivated should record the handled signal name"
        );
        assert!(
            on_activated
                .doc_comment
                .as_deref()
                .is_some_and(|doc| doc.contains("Signal handler for activation")),
            "onActivated doc comment should be extracted"
        );
    }
}
