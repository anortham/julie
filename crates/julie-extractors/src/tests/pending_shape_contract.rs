//! Shape contract for `StructuredPendingRelationship` outputs across all
//! languages.
//!
//! Every golden fixture that emits at least one
//! `structured_pending_relationships` entry must have entries with
//! non-placeholder field values. This is the contract that makes the
//! pending-relationship signal useful at resolve time — without it,
//! cross-file calls collapse onto wrong targets at semantic-merge time.
//!
//! Invariants asserted (per rubric §2.1, mapped to the snake_case JSON shape
//! the canonical extractor emits):
//! - `target.terminal_name` is non-empty (the actual symbol name being
//!   referenced, not a placeholder).
//! - `target.display_name` is non-empty.
//! - `caller_scope_key`, when present, is a non-empty string (it should
//!   resolve to a real symbol-key in the same fixture).
//! - `pending.line_number` is `> 0` (real source position, not the root-node
//!   span fallback).
//! - `pending.file_path` is non-empty.
//!
//! Note: the rubric prose uses camelCase (`terminalName`, `callerScopeSymbolId`)
//! as a forward-looking convention; the actual emitted JSON is snake_case. The
//! semantic invariants are unchanged.
//!
//! The test scans every fixture listed in `capabilities.json`. Languages
//! whose fixtures emit no structured pending entries pass trivially —
//! Phase 4 work adds entries per-language and this test then becomes their
//! Recipe-A regression gate.

use crate::tests::capability_matrix::{load_expected_fixture, load_matrix, workspace_root};
use serde_json::Value;

#[test]
fn structured_pending_entries_have_non_placeholder_fields() {
    let root = workspace_root();
    let matrix = load_matrix(&root);
    let mut errors = Vec::new();
    for row in &matrix.languages {
        for fixture in &row.fixtures {
            let expected = load_expected_fixture(&root, fixture);
            let pending = match expected.get("structured_pending_relationships") {
                Some(Value::Array(arr)) if !arr.is_empty() => arr.clone(),
                _ => continue,
            };
            for (idx, entry) in pending.iter().enumerate() {
                let where_ = format!(
                    "{}/{} structured_pending_relationships[{}]",
                    row.language, fixture.name, idx
                );
                let target = match entry.get("target").and_then(Value::as_object) {
                    Some(t) => t,
                    None => {
                        errors.push(format!("{where_} has no `target` object"));
                        continue;
                    }
                };
                let terminal = target
                    .get("terminal_name")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if terminal.is_empty() {
                    errors.push(format!("{where_} has empty target.terminal_name"));
                }
                let display = target
                    .get("display_name")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if display.is_empty() {
                    errors.push(format!("{where_} has empty target.display_name"));
                }
                if let Some(scope) = entry.get("caller_scope_key") {
                    let s = scope.as_str().unwrap_or("");
                    if s.is_empty() {
                        errors.push(format!(
                            "{where_} has caller_scope_key set to a non-string or empty value"
                        ));
                    }
                }
                let pending_obj = match entry.get("pending").and_then(Value::as_object) {
                    Some(p) => p,
                    None => {
                        errors.push(format!("{where_} has no `pending` payload object"));
                        continue;
                    }
                };
                let line = pending_obj
                    .get("line_number")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                if line == 0 {
                    errors.push(format!(
                        "{where_} pending.line_number must be > 0 (got 0 — likely a root-node span fallback)"
                    ));
                }
                let file_path = pending_obj
                    .get("file_path")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if file_path.is_empty() {
                    errors.push(format!("{where_} pending.file_path is empty"));
                }
            }
        }
    }
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}
