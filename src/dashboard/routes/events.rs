//! SSE event stream route handlers.

use std::convert::Infallible;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::dashboard::AppState;
use crate::dashboard::state::DashboardEvent;

/// SSE stream for live tool call activity.
pub async fn activity_stream(
    State(state): State<AppState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.dashboard.subscribe();
    let initial = tokio_stream::once(Ok::<Event, Infallible>(
        Event::default().comment("connected"),
    ));
    let stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(DashboardEvent::ToolCall { tool_name, workspace, duration_ms }) => {
            let ws_short = workspace.split('_').next().unwrap_or(&workspace);
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let html = format!(
                r#"<div class="is-flex" style="gap: 0.75rem; padding: 0.375rem 0.5rem; background: var(--julie-bg); border-radius: 4px; margin-bottom: 0.25rem; align-items: center;"><span class="mono" style="color: var(--julie-text-muted); min-width: 85px;">{tool_name}</span><span style="color: var(--julie-text-muted);">{ws_short}</span><span style="margin-left: auto; color: var(--julie-success);">{duration_ms:.0}ms</span><span class="activity-ts" data-ts="{ts}" style="color: var(--julie-text-muted); font-size: 0.72rem; min-width: 52px; text-align: right;">just now</span></div>"#
            );
            Some(Ok(Event::default().event("activity").data(html)))
        }
        Ok(_) => None,
        Err(e) => {
            tracing::warn!("SSE subscriber lagged: {e}");
            None
        }
    });
    Sse::new(initial.chain(stream)).keep_alive(KeepAlive::default())
}
