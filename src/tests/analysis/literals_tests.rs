//! Tests for the config-driven literal carrier classification + gate
//! (`classify_literals_by_carrier`). The gate sets `kind` on a carrier match
//! and DROPS any literal whose carrier is not recognized — that drop is the
//! bloat control.

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use crate::analysis::literals::{LiteralCarrierConfig, classify_literals_by_carrier};
    use crate::extractors::{Literal, LiteralKind};

    /// Build a captured-state literal (kind=Other, as a reader emits it).
    fn make_literal(language: &str, carrier: Option<&str>, text: &str) -> Literal {
        Literal {
            id: format!("lit-{language}-{text}"),
            literal_text: text.to_string(),
            kind: LiteralKind::Other,
            carrier: carrier.map(|c| c.to_string()),
            arg_position: 0,
            language: language.to_string(),
            file_path: "src/app.ts".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column: 10,
            start_byte: 0,
            end_byte: 10,
            containing_symbol_id: None,
            confidence: 1.0,
        }
    }

    fn configs() -> HashMap<String, LiteralCarrierConfig> {
        HashMap::from([
            (
                "typescript".to_string(),
                LiteralCarrierConfig {
                    url: HashSet::from(["fetch".to_string(), "axios.get".to_string()]),
                    sql: HashSet::new(),
                    route: HashSet::new(),
                },
            ),
            (
                "csharp".to_string(),
                LiteralCarrierConfig {
                    url: HashSet::new(),
                    sql: HashSet::from(["query".to_string(), "executeasync".to_string()]),
                    route: HashSet::new(),
                },
            ),
        ])
    }

    #[test]
    fn ts_fetch_carrier_classified_as_url_and_retained() {
        let mut literals = vec![make_literal("typescript", Some("fetch"), "/api/users")];
        classify_literals_by_carrier(&mut literals, &configs());

        assert_eq!(literals.len(), 1, "carrier-matched literal must survive");
        assert_eq!(literals[0].kind, LiteralKind::Url);
    }

    #[test]
    fn csharp_query_carrier_classified_as_sql_and_retained() {
        let mut literals = vec![make_literal(
            "csharp",
            Some("Query"),
            "SELECT Id FROM Users",
        )];
        classify_literals_by_carrier(&mut literals, &configs());

        assert_eq!(literals.len(), 1);
        assert_eq!(literals[0].kind, LiteralKind::Sql);
    }

    #[test]
    fn carrier_match_is_case_insensitive() {
        let mut literals = vec![
            make_literal("csharp", Some("QUERY"), "SELECT 1"),
            make_literal("typescript", Some("FETCH"), "/x"),
        ];
        classify_literals_by_carrier(&mut literals, &configs());

        assert_eq!(literals.len(), 2, "case-insensitive carriers must match");
        assert_eq!(literals[0].kind, LiteralKind::Sql);
        assert_eq!(literals[1].kind, LiteralKind::Url);
    }

    #[test]
    fn non_carrier_callee_is_dropped() {
        // console.log / logger.Info are not carriers -> the gate drops them.
        let mut literals = vec![
            make_literal("typescript", Some("console.log"), "hello"),
            make_literal("csharp", Some("Console.WriteLine"), "msg"),
        ];
        classify_literals_by_carrier(&mut literals, &configs());

        assert!(
            literals.is_empty(),
            "non-carrier literals must be dropped by the gate, got {literals:?}"
        );
    }

    #[test]
    fn literal_without_carrier_is_dropped() {
        let mut literals = vec![make_literal("typescript", None, "/api/users")];
        classify_literals_by_carrier(&mut literals, &configs());
        assert!(literals.is_empty(), "carrier=None cannot classify -> drop");
    }

    #[test]
    fn language_without_config_drops_all_its_literals() {
        // 'go' has no entry in configs -> every go literal is dropped, while a
        // configured language's carrier-matched literal in the same batch stays.
        let mut literals = vec![
            make_literal("go", Some("Query"), "SELECT 1"),
            make_literal("typescript", Some("fetch"), "/api/users"),
        ];
        classify_literals_by_carrier(&mut literals, &configs());

        assert_eq!(
            literals.len(),
            1,
            "only the configured-language match stays"
        );
        assert_eq!(literals[0].language, "typescript");
        assert_eq!(literals[0].kind, LiteralKind::Url);
    }
}
