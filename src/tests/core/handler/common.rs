// Shared helpers and re-exports for handler tests. Submodules access these
// transparently via `use super::*;` (the parent `handler` module re-exports
// this whole file with `pub use common::*;`).

pub(crate) use crate::dashboard::state::DashboardEvent;
pub(crate) use crate::database::types::FileInfo;
pub(crate) use crate::handler::{JulieServerHandler, metrics_db_path_for_workspace};
pub(crate) use crate::tools::metrics::session::ToolCallReport;
pub(crate) use anyhow::Result;
pub(crate) use rmcp::ServerHandler;
pub(crate) use rmcp::model::{CallToolRequestParams, NumberOrString, PaginatedRequestParams};
pub(crate) use rmcp::service::{RequestContext, serve_directly};
pub(crate) use serde_json::Value;
pub(crate) use std::path::PathBuf;
pub(crate) use std::sync::Arc;
pub(crate) use std::sync::atomic::{AtomicUsize, Ordering};
pub(crate) use tempfile::TempDir;
pub(crate) use tokio::sync::broadcast;

pub fn json_object(value: Value) -> rmcp::model::JsonObject {
    value
        .as_object()
        .expect("test arguments should be a JSON object")
        .clone()
}

pub async fn latest_tool_metric(
    handler: &JulieServerHandler,
    tool_name: &str,
) -> Result<(i64, serde_json::Value)> {
    let db_arc = {
        let workspace = handler.workspace.read().await;
        workspace
            .as_ref()
            .and_then(|workspace| workspace.db.as_ref())
            .expect("indexed workspace should have a database")
            .clone()
    };

    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let row = {
                let db = db_arc.lock().expect("workspace db should lock");
                let mut stmt = db.conn.prepare(
                    "SELECT success, metadata
                     FROM tool_calls
                     WHERE tool_name = ?1
                     ORDER BY id DESC LIMIT 1",
                )?;
                let mut rows = stmt.query(rusqlite::params![tool_name])?;
                rows.next()?
                    .map(|row| {
                        Ok::<(i64, Option<String>), rusqlite::Error>((row.get(0)?, row.get(1)?))
                    })
                    .transpose()?
            };

            if let Some((success, metadata)) = row {
                let metadata = metadata
                    .as_deref()
                    .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
                    .unwrap_or(serde_json::Value::Null);
                break Ok::<(i64, serde_json::Value), anyhow::Error>((success, metadata));
            }
            tokio::task::yield_now().await;
        }
    })
    .await?
}

pub async fn call_public_tool(
    handler: &JulieServerHandler,
    tool_name: &str,
    arguments: serde_json::Value,
    request_id: i64,
) -> Result<bool> {
    let (server_transport, client_transport) = tokio::io::duplex(64);
    drop(client_transport);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);
    let request =
        CallToolRequestParams::new(tool_name.to_string()).with_arguments(json_object(arguments));
    let result = <JulieServerHandler as ServerHandler>::call_tool(
        handler,
        request,
        RequestContext::new(NumberOrString::Number(request_id), service.peer().clone()),
    )
    .await;
    let _ = service.cancel().await;
    Ok(result.is_ok())
}

pub fn set_readonly(path: &std::path::Path, readonly: bool) -> Result<()> {
    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_readonly(readonly);
    std::fs::set_permissions(path, permissions)?;
    Ok(())
}

pub fn collect_schema_enum_strings(root: &Value, schema: &Value, values: &mut Vec<String>) {
    if let Some(enum_values) = schema.get("enum").and_then(Value::as_array) {
        values.extend(
            enum_values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string),
        );
    }

    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        if let Some(pointer) = reference.strip_prefix('#') {
            if let Some(target) = root.pointer(pointer) {
                collect_schema_enum_strings(root, target, values);
            }
        }
    }

    for key in ["anyOf", "oneOf", "allOf"] {
        if let Some(items) = schema.get(key).and_then(Value::as_array) {
            for item in items {
                collect_schema_enum_strings(root, item, values);
            }
        }
    }
}
