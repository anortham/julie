use crate::tools::workspace::commands::{
    ManageWorkspaceOperation, ManageWorkspaceRequest, ManageWorkspaceTool,
};
use serde_json::{Value, json};

fn tool_from_json(value: Value) -> ManageWorkspaceTool {
    serde_json::from_value(value).expect("flat manage_workspace JSON should deserialize")
}

fn request_from_json(value: Value) -> anyhow::Result<ManageWorkspaceRequest> {
    let tool = tool_from_json(value);
    ManageWorkspaceRequest::try_from(&tool)
}

fn request_targets_primary(value: Value) -> bool {
    let args = value
        .as_object()
        .expect("test input should be a JSON object");
    ManageWorkspaceOperation::request_targets_primary(Some(args))
}

#[test]
fn manage_workspace_tool_keeps_flat_json_shape_for_representative_operations() {
    let rebuild = tool_from_json(json!({
        "operation": "rebuild",
        "path": "/repo",
        "force": true
    }));
    assert_eq!(rebuild.operation, "rebuild");
    assert_eq!(rebuild.path.as_deref(), Some("/repo"));
    assert_eq!(rebuild.force, Some(true));

    let refresh = tool_from_json(json!({
        "operation": "refresh",
        "workspace_id": "workspace-1",
        "force": false
    }));
    assert_eq!(refresh.operation, "refresh");
    assert_eq!(refresh.workspace_id.as_deref(), Some("workspace-1"));
    assert_eq!(refresh.force, Some(false));

    let health = tool_from_json(json!({
        "operation": "health",
        "detailed": true
    }));
    assert_eq!(health.operation, "health");
    assert_eq!(health.detailed, Some(true));
}

#[test]
fn manage_workspace_request_parses_valid_operations_with_live_fields() {
    let request = request_from_json(json!({
        "operation": "index",
        "path": "/repo",
        "force": true
    }))
    .unwrap();
    assert!(matches!(
        request,
        ManageWorkspaceRequest::Index { path, force }
            if path.as_deref() == Some("/repo") && force
    ));

    let request = request_from_json(json!({
        "operation": "rebuild",
        "path": "/repo"
    }))
    .unwrap();
    assert!(matches!(
        request,
        ManageWorkspaceRequest::Rebuild { path, workspace_id }
            if path.as_deref() == Some("/repo") && workspace_id.is_none()
    ));

    let request = request_from_json(json!({
        "operation": "remove",
        "workspace_id": "workspace-1"
    }))
    .unwrap();
    assert!(matches!(
        request,
        ManageWorkspaceRequest::Remove { workspace_id }
            if workspace_id == "workspace-1"
    ));

    let request = request_from_json(json!({ "operation": "list" })).unwrap();
    assert!(matches!(request, ManageWorkspaceRequest::List));

    let request = request_from_json(json!({ "operation": "status" })).unwrap();
    assert!(matches!(
        request,
        ManageWorkspaceRequest::Status {
            workspace_id: None,
            path: None
        }
    ));

    let request = request_from_json(json!({
        "operation": "refresh",
        "workspace_id": "workspace-1",
        "force": true
    }))
    .unwrap();
    assert!(matches!(
        request,
        ManageWorkspaceRequest::Refresh {
            workspace_id,
            force
        } if workspace_id == "workspace-1" && force
    ));

    let request = request_from_json(json!({
        "operation": "open",
        "path": "/repo",
        "force": true
    }))
    .unwrap();
    assert!(matches!(
        request,
        ManageWorkspaceRequest::Open {
            path,
            workspace_id,
            force
        } if path.as_deref() == Some("/repo") && workspace_id.is_none() && force
    ));

    let request = request_from_json(json!({
        "operation": "status",
        "workspace_id": "workspace-1"
    }))
    .unwrap();
    assert!(matches!(
        request,
        ManageWorkspaceRequest::Status { workspace_id, path }
            if workspace_id.as_deref() == Some("workspace-1") && path.is_none()
    ));

    let request = request_from_json(json!({
        "operation": "health",
        "detailed": true
    }))
    .unwrap();
    assert!(matches!(
        request,
        ManageWorkspaceRequest::Health { detailed } if detailed
    ));

    let request = request_from_json(json!({ "operation": "dashboard" })).unwrap();
    assert!(matches!(request, ManageWorkspaceRequest::Dashboard));
}

#[test]
fn manage_workspace_request_rejects_missing_required_fields_and_unknown_operations() {
    let cases = [
        (
            json!({ "operation": "rebuild" }),
            "'workspace_id' or 'path' parameter required for 'rebuild' operation",
        ),
        (
            json!({ "operation": "remove" }),
            "'workspace_id' parameter required for 'remove' operation",
        ),
        (
            json!({ "operation": "refresh" }),
            "'workspace_id' parameter required for 'refresh' operation",
        ),
        (
            json!({ "operation": "add" }),
            "Unknown operation: 'add'. Valid operations: index, list, open, remove, refresh, health, rebuild, status, recover_edit, recover-edit, dashboard",
        ),
    ];

    for (value, expected) in cases {
        let tool = tool_from_json(value);
        let err = ManageWorkspaceRequest::try_from(&tool).unwrap_err();
        assert_eq!(err.to_string(), expected);
    }
}

#[test]
fn manage_workspace_preflight_classification_uses_shared_operation_parser() {
    assert!(!request_targets_primary(json!({
        "operation": "rebuild",
        "path": "/repo"
    })));
    assert!(request_targets_primary(json!({ "operation": "list" })));
    assert!(request_targets_primary(json!({
        "operation": "remove",
        "workspace_id": "workspace-1"
    })));
    assert!(request_targets_primary(json!({ "operation": "health" })));
    assert!(!request_targets_primary(
        json!({ "operation": "dashboard" })
    ));

    assert!(request_targets_primary(json!({ "operation": "status" })));
    assert!(!request_targets_primary(json!({
        "operation": "status",
        "workspace_id": "workspace-1"
    })));
    assert!(!request_targets_primary(json!({
        "operation": "status",
        "path": "/repo"
    })));

    assert!(request_targets_primary(json!({ "operation": "index" })));
    assert!(request_targets_primary(json!({
        "operation": "index",
        "path": null
    })));
    assert!(!request_targets_primary(json!({
        "operation": "index",
        "path": "/repo"
    })));
    assert!(!request_targets_primary(json!({ "operation": "add" })));
}
