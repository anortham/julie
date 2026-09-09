use crate::request_engine::RequestEngine;
use crate::request_engine::types::{RequestContext, RequestFailure, RequestOrigin, ToolRequest};
use crate::service::status::{ErrorRecord, RequestRecord, StatusLog, now_rfc3339};
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
    if !parts.headers.contains_key("mcp-protocol-version") {
        parts.headers.insert(
            axum::http::HeaderName::from_static("mcp-protocol-version"),
            axum::http::HeaderValue::from_static("2026-07-28"),
        );
    }
    let bytes = match axum::body::to_bytes(body, 16 * 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    if !parts.headers.contains_key("mcp-name") {
        if let Ok(val) = serde_json::from_slice::<Value>(&bytes) {
            if let Some(name) = val
                .get("params")
                .and_then(|p| p.get("name"))
                .and_then(|n| n.as_str())
            {
                if let Ok(hv) = axum::http::HeaderValue::from_str(name) {
                    parts
                        .headers
                        .insert(axum::http::HeaderName::from_static("mcp-name"), hv);
                }
            }
        }
    }
    next.run(axum::extract::Request::from_parts(
        parts,
        axum::body::Body::from(bytes),
    ))
    .await
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

async fn status(State(state): State<AppState>) -> Json<crate::service::status::StatusDocument> {
    Json(state.status.document())
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
