use crate::registry::project_log::{ProjectLog, SERVICE_LOG_PREFIX};
use crate::request_engine::RequestEngine;
use crate::request_engine::types::{RequestContext, RequestFailure, RequestOrigin, ToolRequest};
use crate::service::status::{ErrorRecord, RequestRecord, StatusLog, now_rfc3339};
use crate::tools::workspace::commands::registry::CheckoutStatus;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct AppState {
    pub engine: Arc<RequestEngine>,
    pub status: Arc<StatusLog>,
    pub token: Arc<str>,
    pub request_timeout: Duration,
    pub shutdown: tokio_util::sync::CancellationToken,
}

pub fn router(state: AppState, dashboard: Router) -> Router {
    Router::new()
        .route("/status", get(status))
        .route("/shutdown", post(shutdown))
        .route("/api/{tool}", post(api_call))
        .nest_service(
            "/mcp",
            crate::service::mcp::mcp_service(Arc::clone(&state.engine)),
        )
        .with_state(state.clone())
        .merge(dashboard)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            track_mcp_activity,
        ))
        .layer(axum::middleware::from_fn(default_mcp_headers))
        .layer(axum::middleware::from_fn_with_state(state, require_token))
}

async fn shutdown(State(state): State<AppState>) -> Response {
    let token = state.shutdown.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        token.cancel();
    });
    (StatusCode::ACCEPTED, r#"{"stopping":true}"#).into_response()
}

async fn default_mcp_headers(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let path = request.uri().path();
    if !(path == "/mcp" || path.starts_with("/mcp/")) {
        return next.run(request).await;
    }
    let (mut parts, body) = request.into_parts();
    let bytes = match axum::body::to_bytes(body, 16 * 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let json_val = serde_json::from_slice::<Value>(&bytes).ok();
    if !parts.headers.contains_key("mcp-protocol-version") {
        let version = json_val
            .as_ref()
            .and_then(|v| v.get("params"))
            .and_then(|p| {
                p.get("protocolVersion").or_else(|| {
                    p.get("_meta")
                        .and_then(|m| m.get("io.modelcontextprotocol/protocolVersion"))
                })
            })
            .and_then(|pv| pv.as_str())
            .unwrap_or("2026-07-28");
        if let Ok(hv) = axum::http::HeaderValue::from_str(version) {
            parts.headers.insert(
                axum::http::HeaderName::from_static("mcp-protocol-version"),
                hv,
            );
        }
    }
    let body_field = |path: &[&str]| {
        json_val
            .as_ref()
            .and_then(|v| path.iter().try_fold(v, |v, k| v.get(k)))
            .and_then(|n| n.as_str())
            .map(str::to_owned)
    };
    for (header, value) in [
        ("mcp-name", body_field(&["params", "name"])),
        ("mcp-method", body_field(&["method"])),
    ] {
        if parts.headers.contains_key(header) {
            continue;
        }
        if let Some(hv) = value.and_then(|v| axum::http::HeaderValue::from_str(&v).ok()) {
            parts
                .headers
                .insert(axum::http::HeaderName::from_static(header), hv);
        }
    }
    next.run(axum::extract::Request::from_parts(
        parts,
        axum::body::Body::from(bytes),
    ))
    .await
}

async fn track_mcp_activity(
    State(state): State<AppState>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    if !request.uri().path().starts_with("/mcp") {
        return next.run(request).await;
    }
    let tool = ["mcp-name", "mcp-method"]
        .iter()
        .find_map(|name| request.headers().get(*name).and_then(|v| v.to_str().ok()))
        .unwrap_or("mcp")
        .to_owned();
    let started = Instant::now();
    state.status.begin();
    let response = next.run(request).await;
    let outcome = if response.status().is_success() {
        "ok"
    } else {
        "error"
    };
    state.status.end(
        RequestRecord {
            tool,
            workspace_id: None,
            latency_ms: started.elapsed().as_millis(),
            outcome,
            at: now_rfc3339(),
        },
        None,
    );
    response
}

async fn require_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let presented = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_owned)
        .or_else(|| query.get("token").cloned());
    if presented.as_deref() == Some(&*state.token) {
        next.run(request).await
    } else {
        (StatusCode::UNAUTHORIZED, r#"{"error":"unauthorized"}"#).into_response()
    }
}

async fn status(State(state): State<AppState>) -> Json<Value> {
    let embedding_child = state.engine.semantic_runtime().child_status();
    let document = state
        .status
        .document(checkouts(&state).await, embedding_child);
    let mut body = serde_json::to_value(document).unwrap_or_else(|_| Value::Object(Map::new()));
    let log = ProjectLog::current_path(
        &state.engine.runtimes.registry_paths().logs_dir(),
        SERVICE_LOG_PREFIX,
    );
    if let Some(object) = body.as_object_mut() {
        object.insert(
            "log".into(),
            Value::String(log.to_string_lossy().into_owned()),
        );
    }
    Json(body)
}

async fn checkouts(state: &AppState) -> Vec<CheckoutStatus> {
    let mut arguments = Map::new();
    arguments.insert("operation".into(), Value::String("status".into()));
    let context = RequestContext::new(
        RequestOrigin::Mcp,
        Some(state.request_timeout),
        tokio_util::sync::CancellationToken::new(),
    );
    let reply = state
        .engine
        .execute(ToolRequest::new("manage_workspace", arguments), context)
        .await;
    match reply {
        Ok(reply) => serde_json::from_value(reply.result["structuredContent"]["checkouts"].clone())
            .unwrap_or_default(),
        Err(failure) => {
            tracing::warn!(code = %failure.code, message = %failure.message, "status: checkout scan failed");
            Vec::new()
        }
    }
}

async fn api_call(
    State(state): State<AppState>,
    Path(tool): Path<String>,
    Json(arguments): Json<Map<String, Value>>,
) -> Response {
    let started = Instant::now();
    state.status.begin();
    let request = ToolRequest::new(tool.clone(), arguments);
    let context = RequestContext::new(
        RequestOrigin::Mcp,
        Some(state.request_timeout),
        tokio_util::sync::CancellationToken::new(),
    );
    let result = state.engine.execute(request, context).await;
    let latency_ms = started.elapsed().as_millis();
    match result {
        Ok(reply) => {
            state.status.end(
                RequestRecord {
                    tool,
                    workspace_id: reply.workspace_id.clone(),
                    latency_ms,
                    outcome: "ok",
                    at: now_rfc3339(),
                },
                None,
            );
            Json(reply).into_response()
        }
        Err(failure) => {
            state.status.end(
                RequestRecord {
                    tool: tool.clone(),
                    workspace_id: None,
                    latency_ms,
                    outcome: "error",
                    at: now_rfc3339(),
                },
                Some(ErrorRecord {
                    tool,
                    code: failure.code.clone(),
                    message: failure.message.clone(),
                    at: now_rfc3339(),
                }),
            );
            failure_response(failure)
        }
    }
}

pub fn failure_response(failure: RequestFailure) -> Response {
    let code = if failure.code == RequestFailure::internal("").code {
        StatusCode::INTERNAL_SERVER_ERROR
    } else {
        StatusCode::BAD_REQUEST
    };
    (code, Json(failure)).into_response()
}
