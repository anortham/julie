//! Tests for resilient SymbolKind conversion at the Tantivy result boundary.
//!
//! A corrupt or schema-evolved Tantivy row with an unknown `kind` string must
//! not panic the search request. `tantivy_symbol_to_symbol` must degrade
//! gracefully to `SymbolKind::Variable` instead of unwinding.

#[cfg(test)]
mod tests {
    use crate::extractors::base::SymbolKind;
    use crate::search::index::SymbolSearchResult;
    use crate::tools::search::text_search::tantivy_symbol_to_symbol;

    fn make_result(kind: &str) -> SymbolSearchResult {
        SymbolSearchResult {
            id: "test_id".to_string(),
            name: "test_symbol".to_string(),
            signature: String::new(),
            doc_comment: String::new(),
            file_path: "src/lib.rs".to_string(),
            kind: kind.to_string(),
            language: "rust".to_string(),
            start_line: 1,
            score: 1.0,
            role: String::new(),
            test_role: String::new(),
            capability_flags: String::new(),
        }
    }

    /// A Tantivy row with an unrecognised kind string must not panic.
    /// Before the fix, `SymbolKind::from_string` calls `unwrap_or_else(|| panic!(...))`,
    /// so this test panics and nextest reports it as FAIL.
    /// After the fix it must pass with kind degraded to SymbolKind::Variable.
    #[test]
    fn test_tantivy_symbol_to_symbol_unknown_kind_does_not_panic() {
        let result = make_result("nonexistent_kind");
        let symbol = tantivy_symbol_to_symbol(result);
        assert_eq!(
            symbol.kind,
            SymbolKind::Variable,
            "unknown Tantivy kind must degrade to Variable, not panic"
        );
    }

    /// Known kind strings must still round-trip correctly through the conversion.
    #[test]
    fn test_tantivy_symbol_to_symbol_known_kind_round_trips() {
        let result = make_result("function");
        let symbol = tantivy_symbol_to_symbol(result);
        assert_eq!(symbol.kind, SymbolKind::Function);

        let result = make_result("class");
        let symbol = tantivy_symbol_to_symbol(result);
        assert_eq!(symbol.kind, SymbolKind::Class);
    }

    /// Regression: the rescue path in `content_search_with_index`
    /// (text_search.rs ~:399) re-stringifies an existing Symbol's kind before
    /// calling `tantivy_symbol_to_symbol`. The pre-Codex code used
    /// `format!("{:?}", kind).to_lowercase()`, which produces `"enummember"`
    /// for `SymbolKind::EnumMember`. That string is NOT in `try_from_string`'s
    /// match arms (which expect snake_case `"enum_member"`), so the previous
    /// `from_string` panicked and the post-DoS-fix `try_from_string` would
    /// silently degrade EnumMember to Variable. The fix uses the Display impl
    /// (`s.kind.to_string()`), which emits `"enum_member"` and round-trips.
    #[test]
    fn test_enum_member_rescue_stringification_round_trips() {
        let stringified = SymbolKind::EnumMember.to_string();
        assert_eq!(
            stringified, "enum_member",
            "Display impl must emit snake_case 'enum_member' for the rescue path"
        );
        let result = make_result(&stringified);
        let symbol = tantivy_symbol_to_symbol(result);
        assert_eq!(
            symbol.kind,
            SymbolKind::EnumMember,
            "EnumMember kind must survive rescue-path Display stringification + tantivy_symbol_to_symbol round-trip"
        );
    }
}
