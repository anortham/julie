use serde_json::{Value, json};

use crate::tools::editing::edit_file::EditFileTool;
use crate::tools::editing::rewrite_symbol::RewriteSymbolTool;
use crate::tools::get_context::GetContextTool;
use crate::tools::navigation::{CallPathTool, FastRefsTool};
use crate::tools::patterns::PatternsTool;
use crate::tools::spillover::SpilloverGetTool;
use crate::tools::{BlastRadiusTool, DeepDiveTool, GetSymbolsTool, RenameSymbolTool};

fn target_metadata(symbol_name: Option<&str>, file_path: Option<&str>, line: Option<u32>) -> Value {
    json!({
        "target_symbol_name": symbol_name,
        "target_file_path": file_path,
        "target_line": line,
    })
}

pub(crate) fn merge_object(mut base: Value, additions: Value) -> Value {
    if let (Some(base), Some(additions)) = (base.as_object_mut(), additions.as_object()) {
        for (key, value) in additions {
            base.insert(key.clone(), value.clone());
        }
        Value::Object(base.clone())
    } else {
        additions
    }
}

pub(crate) fn with_failure_kind(base: Value, failure_kind: &str) -> Value {
    merge_object(base, json!({ "failure_kind": failure_kind }))
}

pub(crate) fn fast_refs_metadata(params: &FastRefsTool) -> Value {
    json!({
        "symbol": params.symbol,
        "include_definition": params.include_definition,
        "limit": params.limit,
        "reference_kind": params.reference_kind,
        "workspace": params.workspace,
        "target": target_metadata(Some(&params.symbol), None, None),
    })
}

pub(crate) fn call_path_metadata(params: &CallPathTool) -> Value {
    json!({
        "from": params.from,
        "to": params.to,
        "max_hops": params.max_hops,
        "workspace": params.workspace,
        "from_file_path": params.from_file_path,
        "to_file_path": params.to_file_path,
        "target": target_metadata(Some(&params.to), params.to_file_path.as_deref(), None),
    })
}

pub(crate) fn get_symbols_metadata(params: &GetSymbolsTool) -> Value {
    json!({
        "file": params.file_path,
        "mode": params.mode,
        "target_filter": params.target,
        "workspace": params.workspace,
        "target": target_metadata(params.target.as_deref(), Some(&params.file_path), None),
    })
}

pub(crate) fn deep_dive_metadata(params: &DeepDiveTool) -> Value {
    json!({
        "symbol": params.symbol,
        "depth": params.depth,
        "context_file": params.context_file,
        "workspace": params.workspace,
        "target": target_metadata(Some(&params.symbol), params.context_file.as_deref(), None),
    })
}

pub(crate) fn patterns_metadata(params: &PatternsTool) -> Value {
    json!({
        "operation": params.operation,
        "pattern_id": params.pattern_id,
        "query": params.query,
        "path": params.path,
        "language": params.language,
        "where": params.where_filter,
        "facet": params.facet,
        "group_by": params.group_by,
        "limit": params.limit,
        "workspace": params.workspace,
        "format": params.format,
        "target": target_metadata(None, params.path.as_deref(), None),
    })
}

pub(crate) fn get_context_metadata(params: &GetContextTool) -> Value {
    json!({
        "query": params.query,
        "language": params.language,
        "file_pattern": params.file_pattern,
        "max_tokens": params.max_tokens,
        "format": params.format,
        "edited_files": params.edited_files,
        "entry_symbols": params.entry_symbols,
        "stack_trace": params.stack_trace,
        "failing_test": params.failing_test,
        "max_hops": params.max_hops,
        "prefer_tests": params.prefer_tests,
        "workspace": params.workspace,
        "target": target_metadata(None, None, None),
    })
}

pub(crate) fn spillover_get_metadata(params: &SpilloverGetTool) -> Value {
    json!({
        "spillover_handle": params.spillover_handle,
        "limit": params.limit,
        "format": params.format,
        "target": target_metadata(None, None, None),
    })
}

pub(crate) fn blast_radius_metadata(params: &BlastRadiusTool) -> Value {
    json!({
        "symbol_ids": params.symbol_ids,
        "file_paths": params.file_paths,
        "from_revision": params.from_revision,
        "to_revision": params.to_revision,
        "max_depth": params.max_depth,
        "limit": params.limit,
        "include_tests": params.include_tests,
        "format": params.format,
        "workspace": params.workspace,
        "target": target_metadata(None, None, None),
    })
}

pub(crate) fn rename_symbol_metadata(params: &RenameSymbolTool) -> Value {
    json!({
        "kind": "rename_symbol",
        "dry_run": params.dry_run,
        "applied": false,
        "input_bytes": serde_json::to_vec(params).map(|bytes| bytes.len() as u64).unwrap_or(0),
        "old_name": params.old_name,
        "new_name": params.new_name,
        "scope": params.scope,
        "workspace": params.workspace,
        "target": target_metadata(Some(&params.old_name), params.scope.as_deref(), None),
    })
}

pub(crate) fn edit_file_metadata(params: &EditFileTool) -> Value {
    merge_object(
        params.base_metrics_metadata(),
        json!({
            "file": params.file_path,
            "workspace": params.workspace,
            "target": target_metadata(None, Some(&params.file_path), None),
        }),
    )
}

pub(crate) fn rewrite_symbol_metadata(params: &RewriteSymbolTool) -> Value {
    merge_object(
        params.base_metrics_metadata(),
        json!({
            "workspace": params.workspace,
            "file_path": params.file_path,
            "target": target_metadata(Some(&params.symbol), params.file_path.as_deref(), None),
        }),
    )
}
