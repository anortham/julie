//! SSE event stream route handlers.

use std::convert::Infallible;

use axum::extract::State;
use axum::response::sse::{Event, Sse};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::dashboard::state::DashboardEvent;
use crate::dashboard::AppState;

/// SSE stream for status-related events (session changes).
pub async fn status_stream(
    State(state): State<AppState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.dashboard.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(DashboardEvent::SessionChange { .. }) => {
            Some(Ok(Event::default().data("update")))
        }
        Ok(_) => None,
        Err(e) => {
            tracing::warn!("SSE subscriber lagged: {e}");
            None
        }
    });
    Sse::new(stream)
}

/// SSE stream for metrics-related events (tool calls).
pub async fn metrics_stream(
    State(state): State<AppState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.dashboard.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(DashboardEvent::ToolCall { .. }) => {
            Some(Ok(Event::default().data("update")))
        }
        Ok(_) => None,
        Err(e) => {
            tracing::warn!("SSE subscriber lagged: {e}");
            None
        }
    });
    Sse::new(stream)
}

/// SSE stream for activity events (log entries).
pub async fn activity_stream(
    State(state): State<AppState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.dashboard.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(DashboardEvent::LogEntry(_)) => {
            Some(Ok(Event::default().data("update")))
        }
        Ok(_) => None,
        Err(e) => {
            tracing::warn!("SSE subscriber lagged: {e}");
            None
        }
    });
    Sse::new(stream)
}
