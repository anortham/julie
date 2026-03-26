//! CRUD operations for the tool_calls metrics table.

use super::SymbolDatabase;
use anyhow::Result;
use rusqlite::params;
use std::collections::HashMap;

/// Per-tool summary for a session or time window.
#[derive(Default, Clone, serde::Serialize)]
pub struct ToolCallSummary {
    pub tool_name: String,
    pub call_count: u64,
    pub avg_duration_ms: f64,
    pub total_source_bytes: u64,
    pub total_output_bytes: u64,
}

/// Aggregated history across sessions.
#[derive(Default, serde::Serialize)]
pub struct HistorySummary {
    pub session_count: u64,
    pub total_calls: u64,
    pub total_source_bytes: u64,
    pub total_output_bytes: u64,
    pub per_tool: Vec<ToolCallSummary>,
    pub durations_by_tool: HashMap<String, Vec<f64>>,
}

impl SymbolDatabase {
    pub fn insert_tool_call(
        &self,
        session_id: &str,
        tool_name: &str,
        duration_ms: f64,
        result_count: Option<u32>,
        source_bytes: Option<u64>,
        output_bytes: Option<u64>,
        success: bool,
        metadata: Option<&str>,
    ) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs() as i64;

        self.conn.execute(
            "INSERT INTO tool_calls (session_id, timestamp, tool_name, duration_ms, result_count, source_bytes, output_bytes, success, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                session_id,
                now,
                tool_name,
                duration_ms,
                result_count.map(|v| v as i64),
                source_bytes.map(|v| v as i64),
                output_bytes.map(|v| v as i64),
                if success { 1 } else { 0 },
                metadata,
            ],
        )?;
        Ok(())
    }

    pub fn query_session_summary(&self, session_id: &str) -> Result<Vec<ToolCallSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT tool_name, COUNT(*), AVG(duration_ms), COALESCE(SUM(source_bytes), 0), COALESCE(SUM(output_bytes), 0)
             FROM tool_calls
             WHERE session_id = ?1
             GROUP BY tool_name
             ORDER BY COUNT(*) DESC",
        )?;

        let results = stmt
            .query_map(params![session_id], |row| {
                Ok(ToolCallSummary {
                    tool_name: row.get(0)?,
                    call_count: row.get::<_, i64>(1)? as u64,
                    avg_duration_ms: row.get(2)?,
                    total_source_bytes: row.get::<_, i64>(3)? as u64,
                    total_output_bytes: row.get::<_, i64>(4)? as u64,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(results)
    }

    pub fn query_history_summary(&self, days: u32) -> Result<HistorySummary> {
        let cutoff = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs() as i64
            - (days as i64 * 86400);

        let session_count: i64 = self.conn.query_row(
            "SELECT COUNT(DISTINCT session_id) FROM tool_calls WHERE timestamp >= ?1",
            params![cutoff],
            |row| row.get(0),
        )?;

        let total_calls: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM tool_calls WHERE timestamp >= ?1",
            params![cutoff],
            |row| row.get(0),
        )?;

        let (total_source, total_output): (i64, i64) = self.conn.query_row(
            "SELECT COALESCE(SUM(source_bytes), 0), COALESCE(SUM(output_bytes), 0) FROM tool_calls WHERE timestamp >= ?1",
            params![cutoff],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        let mut stmt = self.conn.prepare(
            "SELECT tool_name, COUNT(*), AVG(duration_ms), COALESCE(SUM(source_bytes), 0), COALESCE(SUM(output_bytes), 0)
             FROM tool_calls WHERE timestamp >= ?1
             GROUP BY tool_name ORDER BY COUNT(*) DESC",
        )?;
        let per_tool = stmt
            .query_map(params![cutoff], |row| {
                Ok(ToolCallSummary {
                    tool_name: row.get(0)?,
                    call_count: row.get::<_, i64>(1)? as u64,
                    avg_duration_ms: row.get(2)?,
                    total_source_bytes: row.get::<_, i64>(3)? as u64,
                    total_output_bytes: row.get::<_, i64>(4)? as u64,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // Fetch durations per tool for p95 calculation
        let mut dur_stmt = self.conn.prepare(
            "SELECT tool_name, duration_ms FROM tool_calls WHERE timestamp >= ?1 ORDER BY tool_name",
        )?;
        let mut durations_by_tool: HashMap<String, Vec<f64>> = HashMap::new();
        let rows = dur_stmt.query_map(params![cutoff], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
        })?;
        for row in rows {
            let (name, dur) = row?;
            durations_by_tool.entry(name).or_default().push(dur);
        }

        Ok(HistorySummary {
            session_count: session_count as u64,
            total_calls: total_calls as u64,
            total_source_bytes: total_source as u64,
            total_output_bytes: total_output as u64,
            per_tool,
            durations_by_tool,
        })
    }
}
