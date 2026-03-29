//! Projects page route handlers.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse};
use serde::Serialize;
use tera::Context;

use crate::dashboard::render_template;
use crate::dashboard::AppState;

/// A single language in the distribution bar.
#[derive(Debug, Clone, Serialize)]
pub struct LanguageEntry {
    pub name: String,
    pub file_count: i64,
    pub percentage: f64,
    pub css_var: String,
}

/// Map a language name to its CSS custom property name.
fn lang_css_var(lang: &str) -> &'static str {
    match lang.to_lowercase().as_str() {
        "rust" => "var(--lang-rust)",
        "typescript" | "tsx" => "var(--lang-typescript)",
        "javascript" | "jsx" => "var(--lang-javascript)",
        "python" => "var(--lang-python)",
        "java" => "var(--lang-java)",
        "c_sharp" | "csharp" | "c#" => "var(--lang-csharp)",
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

/// Fetch language distribution for a workspace via the WorkspacePool.
/// Returns up to `max_entries` named languages; the rest are grouped as "Other".
async fn fetch_language_data(
    state: &AppState,
    workspace_id: &str,
    max_entries: usize,
) -> Vec<LanguageEntry> {
    let pool = match state.dashboard.workspace_pool() {
        Some(p) => p,
        None => return vec![],
    };

    let workspace = match pool.get(workspace_id).await {
        Some(ws) => ws,
        None => return vec![],
    };

    let db = match &workspace.db {
        Some(db) => db,
        None => return vec![],
    };

    let counts = {
        let db_guard = match db.lock() {
            Ok(g) => g,
            Err(_) => return vec![],
        };
        match db_guard.count_files_by_language() {
            Ok(c) => c,
            Err(_) => return vec![],
        }
    };

    if counts.is_empty() {
        return vec![];
    }

    let total: i64 = counts.iter().map(|(_, n)| n).sum();
    if total == 0 {
        return vec![];
    }

    let mut entries = Vec::new();
    let mut other_count: i64 = 0;

    for (i, (lang, count)) in counts.iter().enumerate() {
        if i < max_entries {
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
fn render_compact_lang_bar(languages: &[LanguageEntry]) -> String {
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
    let workspaces = state
        .dashboard
        .daemon_db()
        .and_then(|db| db.list_workspaces().ok())
        .unwrap_or_default();

    let ready_count = workspaces.iter().filter(|w| w.status == "ready").count();
    let indexing_count = workspaces.iter().filter(|w| w.status == "indexing").count();
    let error_count = workspaces.iter().filter(|w| w.status == "error").count();

    let mut context = Context::new();
    context.insert("active_page", "projects");
    context.insert("workspaces", &workspaces);
    context.insert("total_count", &workspaces.len());
    context.insert("ready_count", &ready_count);
    context.insert("indexing_count", &indexing_count);
    context.insert("error_count", &error_count);

    render_template(&state, "projects.html", context).await
}

/// Returns workspace statuses as JSON for live polling.
///
/// Response shape: `{ "_summary": "<html>", "workspace_id": { "badge": "<html>", "symbols": "123", ... }, ... }`
pub async fn statuses(State(state): State<AppState>) -> Result<impl IntoResponse, StatusCode> {
    let workspaces = state
        .dashboard
        .daemon_db()
        .and_then(|db| db.list_workspaces().ok())
        .unwrap_or_default();

    let ready_count = workspaces.iter().filter(|w| w.status == "ready").count();
    let indexing_count = workspaces.iter().filter(|w| w.status == "indexing").count();
    let error_count = workspaces.iter().filter(|w| w.status == "error").count();

    // Render summary partial
    let mut summary_ctx = Context::new();
    summary_ctx.insert("total_count", &workspaces.len());
    summary_ctx.insert("ready_count", &ready_count);
    summary_ctx.insert("indexing_count", &indexing_count);
    summary_ctx.insert("error_count", &error_count);
    let summary_html = render_template(&state, "partials/project_summary.html", summary_ctx)
        .await
        .map(|h| h.0)
        .unwrap_or_default();

    let mut map = serde_json::Map::new();
    map.insert("_summary".into(), serde_json::Value::String(summary_html));

    for ws in &workspaces {
        let languages = fetch_language_data(&state, &ws.workspace_id, 5).await;
        let lang_bar_html = render_compact_lang_bar(&languages);

        let badge = match ws.status.as_str() {
            "ready" => r#"<span class="badge-ready">Ready</span>"#,
            "indexing" => r#"<span class="badge-indexing">Indexing</span>"#,
            "error" => r#"<span class="badge-error">Error</span>"#,
            other => {
                // For non-standard statuses, build inline
                map.insert(
                    ws.workspace_id.clone(),
                    serde_json::json!({
                        "badge": format!(r#"<span style="color: var(--julie-text-muted); font-size: 0.8rem;">{other}</span>"#),
                        "symbols": ws.symbol_count.map(|n| n.to_string()).unwrap_or_else(|| "\u{2014}".into()),
                        "files": ws.file_count.map(|n| n.to_string()).unwrap_or_else(|| "\u{2014}".into()),
                        "vectors": ws.vector_count.map(|n| n.to_string()).unwrap_or_else(|| "\u{2014}".into()),
                        "lang_bar": lang_bar_html,
                    }),
                );
                continue;
            }
        };
        map.insert(
            ws.workspace_id.clone(),
            serde_json::json!({
                "badge": badge,
                "symbols": ws.symbol_count.map(|n| n.to_string()).unwrap_or_else(|| "\u{2014}".into()),
                "files": ws.file_count.map(|n| n.to_string()).unwrap_or_else(|| "\u{2014}".into()),
                "vectors": ws.vector_count.map(|n| n.to_string()).unwrap_or_else(|| "\u{2014}".into()),
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

/// Returns just the project table rows (for htmx polling).
pub async fn table(State(state): State<AppState>) -> Result<Html<String>, StatusCode> {
    let workspaces = state
        .dashboard
        .daemon_db()
        .and_then(|db| db.list_workspaces().ok())
        .unwrap_or_default();

    let ready_count = workspaces.iter().filter(|w| w.status == "ready").count();
    let indexing_count = workspaces.iter().filter(|w| w.status == "indexing").count();
    let error_count = workspaces.iter().filter(|w| w.status == "error").count();

    let mut context = Context::new();
    context.insert("workspaces", &workspaces);
    context.insert("total_count", &workspaces.len());
    context.insert("ready_count", &ready_count);
    context.insert("indexing_count", &indexing_count);
    context.insert("error_count", &error_count);

    render_template(&state, "partials/project_table.html", context).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lang_css_var_known_languages() {
        assert_eq!(lang_css_var("rust"), "var(--lang-rust)");
        assert_eq!(lang_css_var("TypeScript"), "var(--lang-typescript)");
        assert_eq!(lang_css_var("tsx"), "var(--lang-typescript)");
        assert_eq!(lang_css_var("python"), "var(--lang-python)");
        assert_eq!(lang_css_var("c_sharp"), "var(--lang-csharp)");
    }

    #[test]
    fn test_lang_css_var_unknown_falls_back_to_other() {
        assert_eq!(lang_css_var("brainfuck"), "var(--lang-other)");
        assert_eq!(lang_css_var(""), "var(--lang-other)");
    }

    #[test]
    fn test_render_compact_lang_bar_empty() {
        assert_eq!(render_compact_lang_bar(&[]), "");
    }

    #[test]
    fn test_render_compact_lang_bar_single_language() {
        let entries = vec![LanguageEntry {
            name: "Rust".to_string(),
            file_count: 100,
            percentage: 100.0,
            css_var: "var(--lang-rust)".to_string(),
        }];
        let html = render_compact_lang_bar(&entries);
        assert!(html.contains("lang-bar-segment"));
        assert!(html.contains("--lang-rust"));
        assert!(html.contains("Rust: 100 files"));
    }

    #[test]
    fn test_render_compact_lang_bar_multiple_languages() {
        let entries = vec![
            LanguageEntry {
                name: "Rust".to_string(),
                file_count: 70,
                percentage: 70.0,
                css_var: "var(--lang-rust)".to_string(),
            },
            LanguageEntry {
                name: "Python".to_string(),
                file_count: 30,
                percentage: 30.0,
                css_var: "var(--lang-python)".to_string(),
            },
        ];
        let html = render_compact_lang_bar(&entries);
        assert!(html.contains("--lang-rust"));
        assert!(html.contains("--lang-python"));
        assert!(html.contains("width: 70%"));
        assert!(html.contains("width: 30%"));
    }
}

pub async fn detail(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
) -> Result<Html<String>, StatusCode> {
    let db = state.dashboard.daemon_db().ok_or(StatusCode::NOT_FOUND)?;

    let workspace = db
        .get_workspace(&workspace_id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let references = db.list_references(&workspace_id).unwrap_or_default();
    let health = db.get_latest_snapshot(&workspace_id).ok().flatten();

    // Format last_indexed as human-readable
    let last_indexed_str = workspace.last_indexed.map(|ts| {
        chrono::DateTime::from_timestamp(ts, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| ts.to_string())
    });

    // Format index duration as human-readable
    let index_duration_str = workspace.last_index_duration_ms.map(|ms| {
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

    let mut context = Context::new();
    context.insert("workspace", &workspace);
    context.insert("references", &references);
    context.insert("health", &health);
    context.insert("last_indexed_str", &last_indexed_str);
    context.insert("index_duration_str", &index_duration_str);
    context.insert("languages", &languages);
    context.insert("has_languages", &has_languages);

    render_template(&state, "partials/project_detail.html", context).await
}
