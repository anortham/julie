//! Projects page route handlers.

use std::collections::HashMap;
use std::path::Path;

use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse};
use serde::Serialize;
use tera::Context;

use crate::daemon::database::{WorkspaceCleanupEventRow, WorkspaceRow};
use crate::dashboard::AppState;
use crate::dashboard::render_template;

/// A single language in the distribution bar.
#[derive(Debug, Clone, Serialize)]
pub struct LanguageEntry {
    pub name: String,
    pub file_count: i64,
    pub percentage: f64,
    pub css_var: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceSessionStateView {
    pub label: String,
    pub detail: String,
    pub badge_html: String,
    pub path_missing: bool,
    pub current_session_count: usize,
    pub active_session_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectWorkspaceView {
    #[serde(flatten)]
    pub workspace: WorkspaceRow,
    pub session_state: WorkspaceSessionStateView,
}

#[derive(Debug, Clone, Serialize)]
pub struct CleanupEventView {
    pub workspace_id: String,
    pub path: String,
    pub action: String,
    pub reason: String,
    pub timestamp_display: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectsNotice {
    pub kind: String,
    pub title: String,
    pub lines: Vec<String>,
}

struct ProjectsPageData {
    workspaces: Vec<ProjectWorkspaceView>,
    cleanup_events: Vec<CleanupEventView>,
    total_count: usize,
    current_count: usize,
    active_count: usize,
    known_count: usize,
    missing_count: usize,
    ready_count: usize,
    indexing_count: usize,
    error_count: usize,
}

impl ProjectsNotice {
    pub(crate) fn from_text(text: impl Into<String>) -> Self {
        let text = text.into();
        let mut lines = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();

        let title = if lines.is_empty() {
            "Workspace Update".to_string()
        } else {
            lines.remove(0)
        };

        let kind = if title.contains("Failed")
            || title.contains("Blocked")
            || title.contains("not found")
            || title.contains("Missing")
        {
            "danger"
        } else {
            "info"
        };

        Self {
            kind: kind.to_string(),
            title,
            lines,
        }
    }

    pub(crate) fn error(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: "danger".to_string(),
            title: title.into(),
            lines: vec![message.into()],
        }
    }
}

/// Map a language name to its CSS custom property name.
pub(crate) fn lang_css_var(lang: &str) -> &'static str {
    match lang.to_lowercase().as_str() {
        "rust" => "var(--lang-rust)",
        "typescript" | "tsx" => "var(--lang-typescript)",
        "javascript" | "jsx" => "var(--lang-javascript)",
        "python" => "var(--lang-python)",
        "java" => "var(--lang-java)",
        "c_sharp" | "csharp" | "c#" => "var(--lang-csharp)",
        "vbnet" | "vb.net" | "vb" => "var(--lang-vbnet)",
        "go" => "var(--lang-go)",
        "c" => "var(--lang-c)",
        "cpp" | "c++" => "var(--lang-cpp)",
        "ruby" => "var(--lang-ruby)",
        "swift" => "var(--lang-swift)",
        "php" => "var(--lang-php)",
        "kotlin" => "var(--lang-kotlin)",
        "html" => "var(--lang-html)",
        "css" => "var(--lang-css)",
        "scala" => "var(--lang-scala)",
        "elixir" => "var(--lang-elixir)",
        "lua" => "var(--lang-lua)",
        "dart" => "var(--lang-dart)",
        "zig" => "var(--lang-zig)",
        "r" => "var(--lang-r)",
        "gdscript" => "var(--lang-gdscript)",
        "vue" => "var(--lang-vue)",
        _ => "var(--lang-other)",
    }
}

fn session_state_badge(label: &str) -> String {
    match label {
        "CURRENT" => r#"<span class="badge-ready">CURRENT</span>"#.to_string(),
        "ACTIVE" => r#"<span class="badge-indexing">ACTIVE</span>"#.to_string(),
        "MISSING" => r#"<span class="badge-error">MISSING</span>"#.to_string(),
        _ => r#"<span style="color: var(--julie-text-muted); font-size: 0.8rem;">KNOWN</span>"#
            .to_string(),
    }
}

fn workspace_session_state(
    workspace: &WorkspaceRow,
    current_workspace_counts: &HashMap<String, usize>,
) -> WorkspaceSessionStateView {
    let path_missing = !Path::new(&workspace.path).exists();
    let current_session_count = current_workspace_counts
        .get(&workspace.workspace_id)
        .copied()
        .unwrap_or(0);
    let active_session_count = workspace.session_count.max(0) as usize;

    let (label, detail) = if path_missing {
        let suffix = if active_session_count == 1 { "" } else { "s" };
        let detail = if active_session_count > 0 {
            format!(
                "Path is gone. Auto-prune waits for {} live session{}.",
                active_session_count, suffix
            )
        } else {
            "Path is gone. Open will prune the stale row and index.".to_string()
        };
        ("MISSING", detail)
    } else if current_session_count > 0 {
        let suffix = if current_session_count == 1 { "" } else { "s" };
        let detail = if active_session_count > current_session_count {
            format!(
                "{} session{} have this as primary, {} total are attached.",
                current_session_count, suffix, active_session_count
            )
        } else {
            format!(
                "{} session{} have this as primary.",
                current_session_count, suffix
            )
        };
        ("CURRENT", detail)
    } else if active_session_count > 0 {
        let suffix = if active_session_count == 1 { "" } else { "s" };
        (
            "ACTIVE",
            format!(
                "{} session{} are attached without owning primary.",
                active_session_count, suffix
            ),
        )
    } else {
        ("KNOWN", "Indexed and inactive.".to_string())
    };

    WorkspaceSessionStateView {
        label: label.to_string(),
        detail,
        badge_html: session_state_badge(label),
        path_missing,
        current_session_count,
        active_session_count,
    }
}

fn cleanup_event_view(event: WorkspaceCleanupEventRow) -> CleanupEventView {
    let timestamp_display = chrono::DateTime::from_timestamp(event.timestamp, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| event.timestamp.to_string());

    CleanupEventView {
        workspace_id: event.workspace_id,
        path: event.path,
        action: event.action.replace('_', " "),
        reason: event.reason.replace('_', " "),
        timestamp_display,
    }
}

fn load_projects_page_data(state: &AppState) -> ProjectsPageData {
    let workspace_rows = state
        .dashboard
        .daemon_db()
        .and_then(|db| db.list_workspaces().ok())
        .unwrap_or_default();
    let current_workspace_counts = state.dashboard.sessions().current_workspace_counts();
    let workspaces = workspace_rows
        .into_iter()
        .map(|workspace| {
            let session_state = workspace_session_state(&workspace, &current_workspace_counts);
            ProjectWorkspaceView {
                workspace,
                session_state,
            }
        })
        .collect::<Vec<_>>();

    let cleanup_events = state
        .dashboard
        .daemon_db()
        .and_then(|db| db.list_cleanup_events(5).ok())
        .unwrap_or_default()
        .into_iter()
        .map(cleanup_event_view)
        .collect::<Vec<_>>();

    let total_count = workspaces.len();
    let current_count = workspaces
        .iter()
        .filter(|workspace| workspace.session_state.label == "CURRENT")
        .count();
    let active_count = workspaces
        .iter()
        .filter(|workspace| workspace.session_state.label == "ACTIVE")
        .count();
    let known_count = workspaces
        .iter()
        .filter(|workspace| workspace.session_state.label == "KNOWN")
        .count();
    let missing_count = workspaces
        .iter()
        .filter(|workspace| workspace.session_state.label == "MISSING")
        .count();
    let ready_count = workspaces
        .iter()
        .filter(|workspace| workspace.workspace.status == "ready")
        .count();
    let indexing_count = workspaces
        .iter()
        .filter(|workspace| workspace.workspace.status == "indexing")
        .count();
    let error_count = workspaces
        .iter()
        .filter(|workspace| workspace.workspace.status == "error")
        .count();

    ProjectsPageData {
        workspaces,
        cleanup_events,
        total_count,
        current_count,
        active_count,
        known_count,
        missing_count,
        ready_count,
        indexing_count,
        error_count,
    }
}

fn build_projects_context(data: &ProjectsPageData, notice: Option<&ProjectsNotice>) -> Context {
    let mut context = Context::new();
    context.insert("active_page", "projects");
    context.insert("workspaces", &data.workspaces);
    context.insert("cleanup_events", &data.cleanup_events);
    context.insert("total_count", &data.total_count);
    context.insert("current_count", &data.current_count);
    context.insert("active_count", &data.active_count);
    context.insert("known_count", &data.known_count);
    context.insert("missing_count", &data.missing_count);
    context.insert("ready_count", &data.ready_count);
    context.insert("indexing_count", &data.indexing_count);
    context.insert("error_count", &data.error_count);
    if let Some(notice) = notice {
        context.insert("notice", notice);
    }
    context
}

pub(crate) async fn render_projects_page(
    state: &AppState,
    notice: Option<ProjectsNotice>,
) -> Result<Html<String>, StatusCode> {
    let data = load_projects_page_data(state);
    let context = build_projects_context(&data, notice.as_ref());
    render_template(state, "projects.html", context).await
}

/// Fetch language distribution for a workspace via the WorkspacePool.
/// Returns up to `max_entries` named languages; the rest are grouped as "Other".
async fn fetch_language_data(
    state: &AppState,
    workspace_id: &str,
    max_entries: usize,
) -> Vec<LanguageEntry> {
    let pool = match state.dashboard.workspace_pool() {
        Some(pool) => pool,
        None => return vec![],
    };

    let workspace = match pool.get(workspace_id).await {
        Some(workspace) => workspace,
        None => return vec![],
    };

    let db = match &workspace.db {
        Some(db) => db,
        None => return vec![],
    };

    let counts = {
        let db_guard = match db.lock() {
            Ok(guard) => guard,
            Err(_) => return vec![],
        };
        match db_guard.count_files_by_language() {
            Ok(counts) => counts,
            Err(_) => return vec![],
        }
    };

    if counts.is_empty() {
        return vec![];
    }

    let total: i64 = counts.iter().map(|(_, count)| count).sum();
    if total == 0 {
        return vec![];
    }

    let mut entries = Vec::new();
    let mut other_count: i64 = 0;

    for (index, (lang, count)) in counts.iter().enumerate() {
        if index < max_entries {
            entries.push(LanguageEntry {
                name: lang.clone(),
                file_count: *count,
                percentage: (*count as f64 / total as f64) * 100.0,
                css_var: lang_css_var(lang).to_string(),
            });
        } else {
            other_count += count;
        }
    }

    if other_count > 0 {
        entries.push(LanguageEntry {
            name: "Other".to_string(),
            file_count: other_count,
            percentage: (other_count as f64 / total as f64) * 100.0,
            css_var: lang_css_var("other").to_string(),
        });
    }

    entries
}

/// Render a compact language bar as an HTML string for the statuses JSON response.
pub(crate) fn render_compact_lang_bar(languages: &[LanguageEntry]) -> String {
    if languages.is_empty() {
        return String::new();
    }

    let mut html = String::new();
    for lang in languages {
        html.push_str(&format!(
            r#"<div class="lang-bar-segment" style="width: {pct}%; background: {color};" title="{name}: {count} files ({pct_r}%)"></div>"#,
            pct = lang.percentage,
            color = lang.css_var,
            name = lang.name,
            count = lang.file_count,
            pct_r = format!("{:.1}", lang.percentage),
        ));
    }
    html
}

pub async fn index(State(state): State<AppState>) -> Result<Html<String>, StatusCode> {
    render_projects_page(&state, None).await
}

/// Returns workspace statuses as JSON for live polling.
///
/// Response shape:
/// `{ "_summary": "<html>", "workspace_id": { "badge": "<html>", ... }, ... }`
pub async fn statuses(State(state): State<AppState>) -> Result<impl IntoResponse, StatusCode> {
    let data = load_projects_page_data(&state);

    let mut summary_ctx = Context::new();
    summary_ctx.insert("total_count", &data.total_count);
    summary_ctx.insert("current_count", &data.current_count);
    summary_ctx.insert("active_count", &data.active_count);
    summary_ctx.insert("known_count", &data.known_count);
    summary_ctx.insert("missing_count", &data.missing_count);
    summary_ctx.insert("ready_count", &data.ready_count);
    summary_ctx.insert("indexing_count", &data.indexing_count);
    summary_ctx.insert("error_count", &data.error_count);
    let summary_html = render_template(&state, "partials/project_summary.html", summary_ctx)
        .await
        .map(|html| html.0)
        .unwrap_or_default();

    let mut map = serde_json::Map::new();
    map.insert("_summary".into(), serde_json::Value::String(summary_html));

    for workspace in &data.workspaces {
        let languages = fetch_language_data(&state, &workspace.workspace.workspace_id, 5).await;
        let lang_bar_html = render_compact_lang_bar(&languages);

        let badge = match workspace.workspace.status.as_str() {
            "ready" => r#"<span class="badge-ready">Ready</span>"#.to_string(),
            "indexing" => r#"<span class="badge-indexing">Indexing</span>"#.to_string(),
            "error" => r#"<span class="badge-error">Error</span>"#.to_string(),
            other => format!(
                r#"<span style="color: var(--julie-text-muted); font-size: 0.8rem;">{other}</span>"#
            ),
        };

        map.insert(
            workspace.workspace.workspace_id.clone(),
            serde_json::json!({
                "badge": badge,
                "session_state": workspace.session_state.badge_html,
                "symbols": workspace.workspace.symbol_count.map(|count| count.to_string()).unwrap_or_else(|| "\u{2014}".into()),
                "files": workspace.workspace.file_count.map(|count| count.to_string()).unwrap_or_else(|| "\u{2014}".into()),
                "vectors": workspace.workspace.vector_count.map(|count| count.to_string()).unwrap_or_else(|| "\u{2014}".into()),
                "lang_bar": lang_bar_html,
            }),
        );
    }

    let body = serde_json::Value::Object(map).to_string();
    Ok((
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        body,
    ))
}

/// Returns the project table partial.
pub async fn table(State(state): State<AppState>) -> Result<Html<String>, StatusCode> {
    let data = load_projects_page_data(&state);
    let context = build_projects_context(&data, None);
    render_template(&state, "partials/project_table.html", context).await
}

pub async fn detail(
    State(state): State<AppState>,
    AxumPath(workspace_id): AxumPath<String>,
) -> Result<Html<String>, StatusCode> {
    let db = state.dashboard.daemon_db().ok_or(StatusCode::NOT_FOUND)?;
    let workspace = db
        .get_workspace(&workspace_id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let current_workspace_counts = state.dashboard.sessions().current_workspace_counts();
    let workspace = ProjectWorkspaceView {
        session_state: workspace_session_state(&workspace, &current_workspace_counts),
        workspace,
    };
    let health = db.get_latest_snapshot(&workspace_id).ok().flatten();

    let last_indexed_str = workspace.workspace.last_indexed.map(|timestamp| {
        chrono::DateTime::from_timestamp(timestamp, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| timestamp.to_string())
    });

    let index_duration_str = workspace.workspace.last_index_duration_ms.map(|ms| {
        if ms >= 60_000 {
            format!("{}m {:.1}s", ms / 60_000, (ms % 60_000) as f64 / 1000.0)
        } else if ms >= 1_000 {
            format!("{:.1}s", ms as f64 / 1000.0)
        } else {
            format!("{}ms", ms)
        }
    });

    let languages = fetch_language_data(&state, &workspace_id, 8).await;
    let has_languages = !languages.is_empty();

    let kind_bar_html = {
        let pool = state.dashboard.workspace_pool();
        if let Some(pool) = pool {
            if let Some(workspace_arc) = pool.get(&workspace_id).await {
                if let Some(db) = &workspace_arc.db {
                    let guard = db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                    let (by_kind, _) = guard.get_symbol_statistics().unwrap_or_default();

                    let total: usize = by_kind.values().sum();
                    if total > 0 {
                        let mut entries: Vec<_> = by_kind.iter().collect();
                        entries.sort_by(|left, right| right.1.cmp(left.1));
                        let segments = entries
                            .iter()
                            .take(8)
                            .map(|(kind, count)| {
                                let pct = (**count as f64 / total as f64) * 100.0;
                                let css_var =
                                    crate::dashboard::routes::intelligence::kind_css_var(kind);
                                let escaped_kind = kind
                                    .replace('&', "&amp;")
                                    .replace('<', "&lt;")
                                    .replace('>', "&gt;")
                                    .replace('"', "&quot;");
                                format!(
                                    r#"<div class="kind-bar-segment" style="width: {:.1}%; background: var({});" title="{}: {} ({:.1}%)"></div>"#,
                                    pct, css_var, escaped_kind, count, pct
                                )
                            })
                            .collect::<Vec<_>>();
                        format!(r#"<div class="kind-bar-track">{}</div>"#, segments.join(""))
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    };

    let mut context = Context::new();
    context.insert("workspace", &workspace);
    context.insert("health", &health);
    context.insert("last_indexed_str", &last_indexed_str);
    context.insert("index_duration_str", &index_duration_str);
    context.insert("languages", &languages);
    context.insert("has_languages", &has_languages);
    context.insert("kind_bar_html", &kind_bar_html);

    render_template(&state, "partials/project_detail.html", context).await
}
