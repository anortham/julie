use anyhow::Result;
use rusqlite::params;

use super::{DaemonDatabase, now_unix};

impl DaemonDatabase {
    pub fn insert_search_compare_run(&self, run: &SearchCompareRunInput) -> Result<i64> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "INSERT INTO search_compare_runs
                (created_at, baseline_strategy, candidate_strategy, case_count,
                 baseline_top1_hits, candidate_top1_hits, baseline_top3_hits, candidate_top3_hits,
                 baseline_source_wins, candidate_source_wins, convergence_rate, stall_rate)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                now_unix(),
                run.baseline_strategy,
                run.candidate_strategy,
                run.case_count,
                run.baseline_top1_hits,
                run.candidate_top1_hits,
                run.baseline_top3_hits,
                run.candidate_top3_hits,
                run.baseline_source_wins,
                run.candidate_source_wins,
                run.convergence_rate,
                run.stall_rate,
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn replace_search_compare_cases(
        &self,
        run_id: i64,
        cases: &[SearchCompareCaseInput],
    ) -> Result<()> {
        let mut conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM search_compare_cases WHERE run_id = ?1",
            params![run_id],
        )?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO search_compare_cases
                    (run_id, session_id, workspace_id, query, search_target, expected_symbol_name,
                     expected_file_path, baseline_rank, candidate_rank, baseline_top_hit, candidate_top_hit)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            )?;
            for case in cases {
                stmt.execute(params![
                    run_id,
                    case.session_id,
                    case.workspace_id,
                    case.query,
                    case.search_target,
                    case.expected_symbol_name,
                    case.expected_file_path,
                    case.baseline_rank,
                    case.candidate_rank,
                    case.baseline_top_hit,
                    case.candidate_top_hit,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn list_search_compare_runs(&self, limit: u32) -> Result<Vec<SearchCompareRunRow>> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn.prepare_cached(
            "SELECT id, created_at, baseline_strategy, candidate_strategy, case_count,
                    baseline_top1_hits, candidate_top1_hits, baseline_top3_hits, candidate_top3_hits,
                    baseline_source_wins, candidate_source_wins, convergence_rate, stall_rate
             FROM search_compare_runs
             ORDER BY created_at DESC, id DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], SearchCompareRunRow::from_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn list_search_compare_cases(&self, run_id: i64) -> Result<Vec<SearchCompareCaseRow>> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn.prepare_cached(
            "SELECT id, run_id, session_id, workspace_id, query, search_target, expected_symbol_name,
                    expected_file_path, baseline_rank, candidate_rank, baseline_top_hit, candidate_top_hit
             FROM search_compare_cases
             WHERE run_id = ?1
             ORDER BY id",
        )?;
        let rows = stmt.query_map(params![run_id], SearchCompareCaseRow::from_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

pub struct SearchCompareRunInput {
    pub baseline_strategy: String,
    pub candidate_strategy: String,
    pub case_count: i64,
    pub baseline_top1_hits: i64,
    pub candidate_top1_hits: i64,
    pub baseline_top3_hits: i64,
    pub candidate_top3_hits: i64,
    pub baseline_source_wins: i64,
    pub candidate_source_wins: i64,
    pub convergence_rate: Option<f64>,
    pub stall_rate: Option<f64>,
}

pub struct SearchCompareCaseInput {
    pub session_id: String,
    pub workspace_id: String,
    pub query: String,
    pub search_target: String,
    pub expected_symbol_name: Option<String>,
    pub expected_file_path: Option<String>,
    pub baseline_rank: Option<i64>,
    pub candidate_rank: Option<i64>,
    pub baseline_top_hit: Option<String>,
    pub candidate_top_hit: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchCompareRunRow {
    pub id: i64,
    pub created_at: i64,
    pub baseline_strategy: String,
    pub candidate_strategy: String,
    pub case_count: i64,
    pub baseline_top1_hits: i64,
    pub candidate_top1_hits: i64,
    pub baseline_top3_hits: i64,
    pub candidate_top3_hits: i64,
    pub baseline_source_wins: i64,
    pub candidate_source_wins: i64,
    pub convergence_rate: Option<f64>,
    pub stall_rate: Option<f64>,
}

impl SearchCompareRunRow {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            created_at: row.get(1)?,
            baseline_strategy: row.get(2)?,
            candidate_strategy: row.get(3)?,
            case_count: row.get(4)?,
            baseline_top1_hits: row.get(5)?,
            candidate_top1_hits: row.get(6)?,
            baseline_top3_hits: row.get(7)?,
            candidate_top3_hits: row.get(8)?,
            baseline_source_wins: row.get(9)?,
            candidate_source_wins: row.get(10)?,
            convergence_rate: row.get(11)?,
            stall_rate: row.get(12)?,
        })
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchCompareCaseRow {
    pub id: i64,
    pub run_id: i64,
    pub session_id: String,
    pub workspace_id: String,
    pub query: String,
    pub search_target: String,
    pub expected_symbol_name: Option<String>,
    pub expected_file_path: Option<String>,
    pub baseline_rank: Option<i64>,
    pub candidate_rank: Option<i64>,
    pub baseline_top_hit: Option<String>,
    pub candidate_top_hit: Option<String>,
}

impl SearchCompareCaseRow {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            run_id: row.get(1)?,
            session_id: row.get(2)?,
            workspace_id: row.get(3)?,
            query: row.get(4)?,
            search_target: row.get(5)?,
            expected_symbol_name: row.get(6)?,
            expected_file_path: row.get(7)?,
            baseline_rank: row.get(8)?,
            candidate_rank: row.get(9)?,
            baseline_top_hit: row.get(10)?,
            candidate_top_hit: row.get(11)?,
        })
    }
}
