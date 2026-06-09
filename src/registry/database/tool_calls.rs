use std::collections::HashMap;

use anyhow::Result;
use rusqlite::params;

use crate::database::{HistorySummary, ToolCallSummary};

use super::{DaemonDatabase, now_unix};

impl DaemonDatabase {
    /// Insert one tool call record. `workspace_id` is the primary workspace for
    /// the session that made the call.
    #[allow(clippy::too_many_arguments)]
    pub fn insert_tool_call(
        &self,
        workspace_id: &str,
        session_id: &str,
        tool_name: &str,
        duration_ms: f64,
        result_count: Option<u32>,
        source_bytes: Option<u64>,
        output_bytes: Option<u64>,
        success: bool,
        metadata: Option<&str>,
    ) -> Result<()> {
        self.insert_tool_call_with_input_bytes(
            workspace_id,
            session_id,
            tool_name,
            duration_ms,
            result_count,
            source_bytes,
            None,
            output_bytes,
            success,
            metadata,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert_tool_call_with_input_bytes(
        &self,
        workspace_id: &str,
        session_id: &str,
        tool_name: &str,
        duration_ms: f64,
        result_count: Option<u32>,
        source_bytes: Option<u64>,
        input_bytes: Option<u64>,
        output_bytes: Option<u64>,
        success: bool,
        metadata: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "INSERT INTO tool_calls
                (workspace_id, session_id, timestamp, tool_name, duration_ms,
                 result_count, source_bytes, input_bytes, output_bytes, success, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                workspace_id,
                session_id,
                now_unix(),
                tool_name,
                duration_ms,
                result_count.map(|v| v as i64),
                source_bytes.map(|v| v as i64),
                input_bytes.map(|v| v as i64),
                output_bytes.map(|v| v as i64),
                if success { 1 } else { 0 },
                metadata,
            ],
        )?;
        Ok(())
    }

    /// Get tool call success rate for a workspace over the last N days.
    /// Returns (total_calls, succeeded_calls).
    pub fn get_tool_success_rate(&self, workspace_id: &str, days: u32) -> Result<(i64, i64)> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let cutoff = now_unix() - (days as i64 * 86400);

        let (total, succeeded): (i64, i64) = conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(CASE WHEN success = 1 THEN 1 ELSE 0 END), 0) \
             FROM tool_calls \
             WHERE workspace_id = ?1 AND timestamp >= ?2",
            params![workspace_id, cutoff],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        Ok((total, succeeded))
    }

    /// Query aggregated tool call history for a workspace over the last `days` days.
    pub fn query_tool_call_history(&self, workspace_id: &str, days: u32) -> Result<HistorySummary> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let cutoff = now_unix() - (days as i64 * 86400);

        let session_count: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT session_id) FROM tool_calls
             WHERE workspace_id = ?1 AND timestamp >= ?2",
            params![workspace_id, cutoff],
            |row| row.get(0),
        )?;

        let total_calls: i64 = conn.query_row(
            "SELECT COUNT(*) FROM tool_calls
             WHERE workspace_id = ?1 AND timestamp >= ?2",
            params![workspace_id, cutoff],
            |row| row.get(0),
        )?;

        // Only aggregate rows with source tracking so the "context saved"
        // ratio isn't diluted by older rows that predate source_bytes recording.
        let (total_input, total_source, total_output): (i64, i64, i64) = conn.query_row(
            "SELECT COALESCE(SUM(input_bytes), 0), COALESCE(SUM(source_bytes), 0), COALESCE(SUM(output_bytes), 0)
             FROM tool_calls
             WHERE workspace_id = ?1 AND timestamp >= ?2 AND source_bytes IS NOT NULL",
            params![workspace_id, cutoff],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;

        let mut stmt = conn.prepare(
            "SELECT tool_name, COUNT(*), AVG(duration_ms),
                    COALESCE(SUM(input_bytes), 0), COALESCE(SUM(source_bytes), 0), COALESCE(SUM(output_bytes), 0)
             FROM tool_calls WHERE workspace_id = ?1 AND timestamp >= ?2
             GROUP BY tool_name ORDER BY COUNT(*) DESC",
        )?;
        let per_tool = stmt
            .query_map(params![workspace_id, cutoff], |row| {
                Ok(ToolCallSummary {
                    tool_name: row.get(0)?,
                    call_count: row.get::<_, i64>(1)? as u64,
                    avg_duration_ms: row.get(2)?,
                    total_input_bytes: row.get::<_, i64>(3)? as u64,
                    total_source_bytes: row.get::<_, i64>(4)? as u64,
                    total_output_bytes: row.get::<_, i64>(5)? as u64,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut dur_stmt = conn.prepare(
            "SELECT tool_name, duration_ms FROM tool_calls
             WHERE workspace_id = ?1 AND timestamp >= ?2
             ORDER BY tool_name",
        )?;
        let mut durations_by_tool: HashMap<String, Vec<f64>> = HashMap::new();
        let rows = dur_stmt.query_map(params![workspace_id, cutoff], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
        })?;
        for row in rows {
            let (name, dur) = row?;
            durations_by_tool.entry(name).or_default().push(dur);
        }

        Ok(HistorySummary {
            session_count: session_count as u64,
            total_calls: total_calls as u64,
            total_input_bytes: total_input as u64,
            total_source_bytes: total_source as u64,
            total_output_bytes: total_output as u64,
            per_tool,
            durations_by_tool,
        })
    }

    /// Delete tool call records older than `retention_days`. Called on daemon startup.
    pub fn prune_tool_calls(&self, retention_days: u32) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let cutoff = now_unix() - (retention_days as i64 * 86400);
        conn.execute(
            "DELETE FROM tool_calls WHERE timestamp < ?1",
            params![cutoff],
        )?;
        Ok(())
    }

    pub fn list_tool_calls_for_search_analysis(
        &self,
        window_secs: i64,
    ) -> Result<Vec<SearchToolCallRow>> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let cutoff = now_unix() - window_secs;
        let mut stmt = conn.prepare_cached(
            "SELECT id, workspace_id, session_id, timestamp, tool_name, metadata
             FROM tool_calls
             WHERE timestamp >= ?1
             ORDER BY session_id, timestamp, id",
        )?;
        let rows = stmt.query_map(params![cutoff], |row| {
            Ok(SearchToolCallRow {
                id: row.get(0)?,
                workspace_id: row.get(1)?,
                session_id: row.get(2)?,
                timestamp: row.get(3)?,
                tool_name: row.get(4)?,
                metadata: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

pub struct SearchToolCallRow {
    pub id: i64,
    pub workspace_id: String,
    pub session_id: String,
    pub timestamp: i64,
    pub tool_name: String,
    pub metadata: Option<String>,
}
