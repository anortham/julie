// C# type-level + member attribute → annotation persistence.
//
// Phase 1 of the Miller-bridge extraction enrichments: every C# declaration
// kind that can bear `[Attribute]` markers must surface them as structured
// `symbol.annotations`, not just inside the signature string. Only methods and
// constructors did so before this batch.

use super::*;
use crate::base::AnnotationMarker;
use std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(code: &str) -> Vec<Symbol> {
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = CSharpExtractor::new(
            "c_sharp".to_string(),
            "test.cs".to_string(),
            code.to_string(),
            &workspace_root,
        );
        extractor.extract_symbols(&tree)
    }

    fn find<'a>(symbols: &'a [Symbol], name: &str) -> &'a Symbol {
        symbols.iter().find(|s| s.name == name).unwrap_or_else(|| {
            let names: Vec<&String> = symbols.iter().map(|s| &s.name).collect();
            panic!("symbol `{name}` not found; have: {names:?}")
        })
    }

    fn annotation<'a>(symbol: &'a Symbol, key: &str) -> &'a AnnotationMarker {
        symbol
            .annotations
            .iter()
            .find(|a| a.annotation_key == key)
            .unwrap_or_else(|| {
                panic!(
                    "symbol `{}` missing annotation `{}`; has: {:?}",
                    symbol.name, key, symbol.annotations
                )
            })
    }

    #[test]
    fn type_level_attributes_are_persisted_as_annotations() {
        // (source, symbol name, expected kind, key, display, raw_text)
        let cases: [(&str, &str, SymbolKind, &str, &str, &str); 5] = [
            (
                r#"[Table("Accounts")] public class Account { }"#,
                "Account",
                SymbolKind::Class,
                "table",
                "Table",
                r#"Table("Accounts")"#,
            ),
            (
                "[ServiceContract] public interface IService { }",
                "IService",
                SymbolKind::Interface,
                "servicecontract",
                "ServiceContract",
                "ServiceContract",
            ),
            (
                "[StructLayout(LayoutKind.Sequential)] public struct Point { }",
                "Point",
                SymbolKind::Struct,
                "structlayout",
                "StructLayout",
                "StructLayout(LayoutKind.Sequential)",
            ),
            (
                "[Flags] public enum Access { None }",
                "Access",
                SymbolKind::Enum,
                "flags",
                "Flags",
                "Flags",
            ),
            (
                "[Serializable] public record Money(decimal Amount);",
                "Money",
                SymbolKind::Class,
                "serializable",
                "Serializable",
                "Serializable",
            ),
        ];

        for (code, name, kind, key, display, raw) in cases {
            let symbols = extract(code);
            let symbol = find(&symbols, name);
            assert_eq!(symbol.kind, kind, "unexpected kind for `{name}`");
            let marker = annotation(symbol, key);
            assert_eq!(
                marker.annotation, display,
                "annotation display for `{name}`"
            );
            assert_eq!(
                marker.raw_text.as_deref(),
                Some(raw),
                "raw_text for `{name}`"
            );
        }
    }

    #[test]
    fn enum_member_attributes_are_persisted() {
        let symbols = extract(
            r#"
            public enum Status
            {
                [EnumMember(Value="active")] Active,
                Inactive
            }
            "#,
        );

        let active = find(&symbols, "Active");
        assert_eq!(active.kind, SymbolKind::EnumMember);
        let marker = annotation(active, "enummember");
        assert_eq!(marker.annotation, "EnumMember");
        assert_eq!(
            marker.raw_text.as_deref(),
            Some(r#"EnumMember(Value="active")"#)
        );

        // An unannotated member must stay empty (we attach only what is present).
        let inactive = find(&symbols, "Inactive");
        assert!(
            inactive.annotations.is_empty(),
            "unannotated enum member should carry no annotations, got {:?}",
            inactive.annotations
        );
    }

    #[test]
    fn property_attributes_are_persisted() {
        let symbols = extract(
            r#"
            public class Account
            {
                [Key]
                [Column("acct_id")]
                public int Id { get; set; }
            }
            "#,
        );

        let id = find(&symbols, "Id");
        assert_eq!(id.kind, SymbolKind::Property);
        assert_eq!(annotation(id, "key").annotation, "Key");
        assert_eq!(
            annotation(id, "column").raw_text.as_deref(),
            Some(r#"Column("acct_id")"#)
        );
    }

    #[test]
    fn field_attributes_are_persisted() {
        let symbols = extract(
            r#"
            public class Account
            {
                [Column("acct_id")]
                public int Id;

                [JsonProperty("balance")]
                public decimal Balance;
            }
            "#,
        );

        let id = find(&symbols, "Id");
        assert_eq!(id.kind, SymbolKind::Field);
        assert_eq!(
            annotation(id, "column").raw_text.as_deref(),
            Some(r#"Column("acct_id")"#)
        );

        let balance = find(&symbols, "Balance");
        assert_eq!(
            annotation(balance, "jsonproperty").annotation,
            "JsonProperty"
        );
    }

    #[test]
    fn event_attributes_are_persisted() {
        let symbols = extract(
            r#"
            public class Publisher
            {
                [Obsolete("use OnChanged")]
                public event EventHandler Changed;
            }
            "#,
        );

        let changed = find(&symbols, "Changed");
        assert_eq!(changed.kind, SymbolKind::Event);
        assert_eq!(
            annotation(changed, "obsolete").raw_text.as_deref(),
            Some(r#"Obsolete("use OnChanged")"#)
        );
    }

    #[test]
    fn delegate_attributes_are_persisted() {
        let symbols = extract(
            r#"
            public class Host
            {
                [Obsolete]
                public delegate int Calculator(int a, int b);
            }
            "#,
        );

        let calc = find(&symbols, "Calculator");
        assert_eq!(calc.kind, SymbolKind::Delegate);
        assert_eq!(annotation(calc, "obsolete").annotation, "Obsolete");
    }

    #[test]
    fn destructor_attributes_are_persisted() {
        // C# finalizers can legally bear attributes ([Obsolete]); the grammar
        // exposes attribute_list on destructor_declaration.
        let symbols = extract(
            r#"
            public class Resource
            {
                [Obsolete]
                ~Resource() { }
            }
            "#,
        );

        let finalizer = find(&symbols, "~Resource");
        assert_eq!(annotation(finalizer, "obsolete").annotation, "Obsolete");
    }
}
