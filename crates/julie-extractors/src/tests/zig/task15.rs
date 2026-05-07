use crate::base::{Symbol, SymbolKind};
use crate::tests::helpers::init_parser;
use crate::zig::ZigExtractor;
use std::path::PathBuf;

fn extract_symbols(code: &str) -> Vec<Symbol> {
    let tree = init_parser(code, "zig");
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = ZigExtractor::new(
        "zig".to_string(),
        "test.zig".to_string(),
        code.to_string(),
        &workspace_root,
    );
    extractor.extract_symbols(&tree)
}

#[test]
fn test_zig_usingnamespace_and_non_declaration_imports_are_extracted() {
    let code = r#"
const std = @import("std");
usingnamespace @import("pkg/private.zig");
pub usingnamespace @import("pkg/public.zig");
const debug_mode = @import("builtin").mode == .Debug;
"#;
    let symbols = extract_symbols(code);

    let debug_mode = symbols
        .iter()
        .find(|s| s.name == "debug_mode")
        .expect("debug_mode import expression should be extracted as a symbol");
    assert_eq!(debug_mode.kind, SymbolKind::Import);
    assert!(
        debug_mode
            .signature
            .as_ref()
            .is_some_and(|sig| sig.contains("@import(\"builtin\").mode")),
        "non-declaration @import forms should keep expression context in signature"
    );
    assert_eq!(
        debug_mode
            .metadata
            .as_ref()
            .and_then(|m| m.get("source"))
            .and_then(|v| v.as_str()),
        Some("builtin")
    );

    let is_usingnamespace = |symbol: &&Symbol| {
        symbol.kind == SymbolKind::Import
            && symbol
                .metadata
                .as_ref()
                .and_then(|m| m.get("isUsingNamespace"))
                .and_then(|v| v.as_bool())
                == Some(true)
    };

    let usingnamespace_symbols: Vec<_> = symbols
        .iter()
        .filter(is_usingnamespace)
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(
        usingnamespace_symbols.len(),
        2,
        "both usingnamespace imports should be extracted exactly once"
    );

    let private_ns = symbols
        .iter()
        .filter(is_usingnamespace)
        .find(|s| s.name == "usingnamespace:pkg/private.zig")
        .expect("private usingnamespace symbol missing");
    assert!(
        private_ns
            .signature
            .as_ref()
            .is_some_and(|sig| sig.contains("usingnamespace @import(\"pkg/private.zig\")"))
    );
    assert!(
        private_ns.parent_id.is_none(),
        "top-level usingnamespace should have no parent"
    );

    let public_ns = symbols
        .iter()
        .filter(is_usingnamespace)
        .find(|s| s.name == "usingnamespace:pkg/public.zig")
        .expect("public usingnamespace symbol missing");
    assert!(
        public_ns
            .signature
            .as_ref()
            .is_some_and(|sig| sig.contains("pub usingnamespace @import(\"pkg/public.zig\")"))
    );
    assert_eq!(
        public_ns
            .metadata
            .as_ref()
            .and_then(|m| m.get("source"))
            .and_then(|v| v.as_str()),
        Some("pkg/public.zig")
    );
    assert_eq!(
        usingnamespace_symbols
            .iter()
            .filter(|name| **name == "usingnamespace:pkg/private.zig")
            .count(),
        1
    );
    assert_eq!(
        usingnamespace_symbols
            .iter()
            .filter(|name| **name == "usingnamespace:pkg/public.zig")
            .count(),
        1
    );
}
