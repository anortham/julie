//! API route definitions for the Julie daemon HTTP server.

pub mod agents;
pub mod common;
pub mod dashboard;
pub mod diagnostics;
pub mod health;
pub mod projects;
pub mod search;

use std::sync::Arc;

use axum::{Json, Router, routing::{get, post}};
use utoipa::OpenApi;
use utoipa_scalar::{Scalar, Servable};

use crate::server::AppState;

/// OpenAPI documentation aggregator.
///
/// Collects all annotated paths and schemas into a single OpenAPI 3.1 spec.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Julie API",
        description = "Julie Code Intelligence Server — REST API for projects, search, agents, and dashboard.",
        version = "4.0.0",
        license(name = "MIT")
    ),
    paths(
        health::health_check,
        projects::list_projects,
        projects::create_project,
        projects::delete_project,
        projects::get_project_status,
        projects::trigger_index,
        projects::get_project_stats,
        projects::launch_editor,
        projects::launch_terminal,
        dashboard::stats,
        dashboard::check_embeddings,
        // diagnostics
        diagnostics::report,
        // search
        search::search,
        search::search_debug,
        // agents
        agents::list_backends,
        agents::list_dispatches,
        agents::get_dispatch,
        agents::stream_dispatch,
        agents::dispatch_agent,
    ),
    components(
        schemas(
            health::HealthResponse,
            projects::ProjectResponse,
            projects::CreateProjectRequest,
            projects::CreateProjectResponse,
            projects::ProjectStatusResponse,
            projects::TriggerIndexRequest,
            projects::TriggerIndexResponse,
            projects::ProjectStatsResponse,
            projects::LanguageCount,
            projects::SymbolKindCount,
            projects::LaunchEditorRequest,
            projects::LaunchTerminalRequest,
            projects::LaunchResponse,
            dashboard::DashboardStats,
            dashboard::ProjectStats,
            dashboard::AgentStats,
            dashboard::BackendStat,
            dashboard::EmbeddingProjectStatus,
            // search
            search::SearchRequest,
            search::SearchResponse,
            search::SymbolResultResponse,
            search::ContentResultResponse,
            search::DebugSearchResponse,
            crate::search::debug::SymbolDebugResults,
            crate::search::debug::SymbolDebugResult,
            crate::search::debug::ContentDebugResults,
            crate::search::debug::ContentDebugResult,
            // agents
            agents::DispatchRequest,
            agents::HintsInput,
            agents::DispatchResponse,
            agents::HistoryResponse,
            agents::DispatchSummary,
            agents::DispatchDetail,
            agents::BackendsResponse,
            crate::agent::backend::BackendInfo,
            // diagnostics
            diagnostics::DiagnosticReport,
            diagnostics::SystemInfo,
            diagnostics::DaemonHealth,
            diagnostics::ProjectDiagnostic,
            diagnostics::EmbeddingDiagnostic,
            diagnostics::DaemonLogs,
        )
    ),
    tags(
        (name = "health", description = "Server health"),
        (name = "projects", description = "Project management and indexing"),
        (name = "search", description = "Code search"),
        (name = "agents", description = "Agent dispatch and management"),
        (name = "dashboard", description = "Dashboard statistics"),
        (name = "diagnostics", description = "Diagnostic reports")
    )
)]
pub struct ApiDoc;

/// `GET /api/openapi.json` — returns the OpenAPI specification as JSON.
async fn openapi_spec() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

/// Build the `/api` router with all sub-routes.
pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health::health_check))
        .route("/projects", get(projects::list_projects).post(projects::create_project))
        .route("/projects/{id}", axum::routing::delete(projects::delete_project))
        .route("/projects/{id}/status", get(projects::get_project_status))
        .route("/projects/{id}/stats", get(projects::get_project_stats))
        .route("/projects/{id}/index", axum::routing::post(projects::trigger_index))
        .route("/launch/editor", post(projects::launch_editor))
        .route("/launch/terminal", post(projects::launch_terminal))
        .route("/search", axum::routing::post(search::search))
        .route("/search/debug", axum::routing::post(search::search_debug))
        // Agent dispatch routes (note: /agents/history and /agents/backends BEFORE /agents/{id})
        .route("/agents/dispatch", post(agents::dispatch_agent))
        .route("/agents/history", get(agents::list_dispatches))
        .route("/agents/backends", get(agents::list_backends))
        .route("/agents/{id}/stream", get(agents::stream_dispatch))
        .route("/agents/{id}", get(agents::get_dispatch))
        // Dashboard
        .route("/dashboard/stats", get(dashboard::stats))
        .route("/embeddings/check", post(dashboard::check_embeddings))
        // Diagnostics
        .route("/diagnostics/report", get(diagnostics::report))
        // OpenAPI spec + interactive docs
        .route("/openapi.json", get(openapi_spec))
        .merge(Scalar::with_url("/docs", ApiDoc::openapi()))
        .with_state(state)
}
