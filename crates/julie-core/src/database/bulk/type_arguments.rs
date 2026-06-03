//! Bulk persistence for ordered/nested generic type arguments at use sites
//! (Miller bridge Phase 2). Mirrors `bulk/identifiers.rs`: early-return on
//! empty, `INSERT OR REPLACE`, run under the FK-disabled bulk window owned by
//! `atomic.rs`.
//!
//! The extractor layer emits a tree per use site
//! ([`TypeArgumentUsage`] holding nested [`TypeArgument`]s). This module
//! flattens that tree into row form once — [`flatten_type_argument_usages`] —
//! and stores the result on [`crate::indexing_core::batch::ExtractedBatch`] so
//! every atomic write path inserts the same rows via the
//! [`CanonicalWriteSet`](crate::database::bulk::atomic::CanonicalWriteSet).

use anyhow::Result;
use rusqlite::{Transaction, params};

use julie_extractors::base::{TypeArgument, TypeArgumentUsage};

/// One flattened `type_arguments` row, ready to insert.
///
/// `parent_arg_id` is `None` for a top-level argument and the parent row's
/// `id` for a nested one, so the resolver can reconstruct the applied-type
/// tree. `target_symbol_id` is intentionally write-once-NULL today: nothing
/// resolves it yet (see `cleanup.rs` for the forward-safe NULL-on-symbol-delete
/// guard kept in lockstep).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeArgumentRow {
    pub id: String,
    pub identifier_id: String,
    pub parent_arg_id: Option<String>,
    pub ordinal: u32,
    pub type_name: String,
    pub file_path: String,
    pub language: String,
}

/// Flatten use-site type-argument trees into insertable rows.
///
/// Each node's `id` is a stable hash of `identifier_id` + its ordinal path
/// (e.g. `1.0` for the `int` of `Dictionary<string, List<int>>`), which is
/// unique within a use site and deterministic across re-indexes, so
/// `INSERT OR REPLACE` is idempotent. Parent/child rows are linked by `id`.
pub fn flatten_type_argument_usages(usages: &[TypeArgumentUsage]) -> Vec<TypeArgumentRow> {
    let mut rows = Vec::new();
    for usage in usages {
        for arg in &usage.arguments {
            flatten_arg(usage, arg, None, &arg.ordinal.to_string(), &mut rows);
        }
    }
    rows
}

fn flatten_arg(
    usage: &TypeArgumentUsage,
    arg: &TypeArgument,
    parent_arg_id: Option<&str>,
    path: &str,
    rows: &mut Vec<TypeArgumentRow>,
) {
    let id = type_argument_row_id(&usage.identifier_id, path);
    rows.push(TypeArgumentRow {
        id: id.clone(),
        identifier_id: usage.identifier_id.clone(),
        parent_arg_id: parent_arg_id.map(str::to_string),
        ordinal: arg.ordinal,
        type_name: arg.type_name.clone(),
        file_path: usage.file_path.clone(),
        language: usage.language.clone(),
    });
    for child in &arg.children {
        let child_path = format!("{path}.{}", child.ordinal);
        flatten_arg(usage, child, Some(&id), &child_path, rows);
    }
}

fn type_argument_row_id(identifier_id: &str, path: &str) -> String {
    let digest = md5::compute(format!("{identifier_id}:{path}").as_bytes());
    format!("{digest:x}")
}

/// Insert flattened type-argument rows. Parents are written before children
/// (the flatten order is pre-order), so the self-referential `parent_arg_id`
/// is satisfiable even with FK checks on; under the bulk FK-disabled window the
/// ordering is moot but kept for clarity.
pub(crate) fn insert_type_arguments_tx(
    tx: &Transaction<'_>,
    rows: &[TypeArgumentRow],
    now: i64,
) -> Result<i64> {
    if rows.is_empty() {
        return Ok(0);
    }

    let mut stmt = tx.prepare(
        "INSERT OR REPLACE INTO type_arguments
         (id, identifier_id, parent_arg_id, ordinal, type_name,
          target_symbol_id, file_path, language, last_indexed)
         VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?7, ?8)",
    )?;

    let mut inserted = 0;
    for row in rows {
        stmt.execute(params![
            row.id,
            row.identifier_id,
            row.parent_arg_id,
            row.ordinal,
            row.type_name,
            row.file_path,
            row.language,
            now,
        ])?;
        inserted += 1;
    }

    Ok(inserted)
}
