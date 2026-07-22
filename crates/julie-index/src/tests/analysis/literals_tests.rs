//! Tests for the config-driven literal carrier classification + gate
//! (`classify_literals_by_carrier`). The gate sets `kind` on a carrier match
//! and DROPS any literal whose carrier is not recognized — that drop is the
//! bloat control.

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use crate::analysis::literals::{LiteralCarrierConfig, classify_literals_by_carrier};
    use crate::search::LanguageConfigs;
    use julie_extractors::{Literal, LiteralKind};

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
    fn bare_sql_carrier_matches_local_receiver_member_call() {
        // The TS-SQL local-receiver case: `pool.query("SELECT ...")` emits the
        // carrier "pool.query" (object.property), but the DB receiver is a local
        // variable that can't be enumerated in config. A BARE config entry
        // `query` must match it by last dot-segment so local-variable DB
        // receivers are not silently missed.
        let configs = HashMap::from([(
            "typescript".to_string(),
            LiteralCarrierConfig {
                url: HashSet::new(),
                sql: HashSet::from(["query".to_string()]),
                route: HashSet::new(),
            },
        )]);
        let mut literals = vec![make_literal(
            "typescript",
            Some("pool.query"),
            "SELECT 1 FROM t",
        )];
        classify_literals_by_carrier(&mut literals, &configs);

        assert_eq!(
            literals.len(),
            1,
            "a bare config carrier must match a member-call carrier by last segment"
        );
        assert_eq!(literals[0].kind, LiteralKind::Sql);
    }

    #[test]
    fn dotted_carrier_config_does_not_overmatch_other_receivers() {
        // A DOTTED config entry `axios.get` must match ONLY `axios.get`, never a
        // different receiver's `.get` (e.g. `cache.get`) — otherwise the generic
        // `.get()` method on any object would flood URL literals.
        let configs = HashMap::from([(
            "typescript".to_string(),
            LiteralCarrierConfig {
                url: HashSet::from(["axios.get".to_string()]),
                sql: HashSet::new(),
                route: HashSet::new(),
            },
        )]);
        let mut literals = vec![
            make_literal("typescript", Some("axios.get"), "/api/a"),
            make_literal("typescript", Some("cache.get"), "session-key"),
        ];
        classify_literals_by_carrier(&mut literals, &configs);

        assert_eq!(
            literals.len(),
            1,
            "a dotted config carrier must match exactly, not over-match a different receiver"
        );
        assert_eq!(literals[0].carrier.as_deref(), Some("axios.get"));
        assert_eq!(literals[0].kind, LiteralKind::Url);
    }

    #[test]
    fn bare_carrier_config_matches_any_receiver_method() {
        // A BARE config entry is the opt-in "match this method on any receiver"
        // form: `get` matches `HTTParty.get`, `client.get`, bare `get`, etc.
        let configs = HashMap::from([(
            "ruby".to_string(),
            LiteralCarrierConfig {
                url: HashSet::from(["get".to_string()]),
                sql: HashSet::new(),
                route: HashSet::new(),
            },
        )]);
        let mut literals = vec![make_literal("ruby", Some("HTTParty.get"), "http://x")];
        classify_literals_by_carrier(&mut literals, &configs);

        assert_eq!(
            literals.len(),
            1,
            "bare config matches any receiver's method"
        );
        assert_eq!(literals[0].kind, LiteralKind::Url);
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

    #[test]
    fn tsx_jsx_literals_use_parent_language_carrier_configs() {
        // TSX/JSX extractors report their concrete language names, but their
        // carrier vocabulary lives in the TypeScript/JavaScript configs.
        let carrier_configs = LanguageConfigs::load_embedded().build_literal_carrier_configs();
        let mut literals = vec![
            make_literal("tsx", Some("fetch"), "/api/tsx"),
            make_literal("jsx", Some("fetch"), "/api/jsx"),
        ];

        classify_literals_by_carrier(&mut literals, &carrier_configs);

        assert_eq!(literals.len(), 2, "TSX/JSX carrier matches must survive");
        assert_eq!(literals[0].kind, LiteralKind::Url);
        assert_eq!(literals[1].kind, LiteralKind::Url);
    }
}
