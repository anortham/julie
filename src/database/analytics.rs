// Database analytics queries for the intelligence dashboard

use anyhow::Result;
use serde::Serialize;

/// A high-centrality symbol returned by get_top_symbols_by_centrality.
#[derive(Debug, Clone, Serialize)]
pub struct CentralitySymbol {
    pub name: String,
    pub kind: String,
    pub language: String,
    pub file_path: String,
    pub signature: Option<String>,
    pub reference_score: f64,
}

/// A high-activity file returned by get_file_hotspots.
#[derive(Debug, Clone, Serialize)]
pub struct FileHotspot {
    pub path: String,
    pub language: String,
    pub line_count: i32,
    pub size: i64,
    pub symbol_count: i64,
}

/// Workspace-wide aggregate counts.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AggregateStats {
    pub total_files: i64,
    pub total_symbols: i64,
    pub total_lines: i64,
    pub total_relationships: i64,
    pub language_count: i64,
}

impl super::SymbolDatabase {
    /// Return the top `limit` symbols by reference_score, excluding zero scores.
    ///
    /// Results are ordered by reference_score DESC.
    pub fn get_top_symbols_by_centrality(&self, limit: usize) -> Result<Vec<CentralitySymbol>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, kind, language, file_path, signature, reference_score
             FROM symbols
             WHERE reference_score > 0
             ORDER BY reference_score DESC
             LIMIT ?1",
        )?;

        let rows = stmt.query_map([limit as i64], |row| {
            Ok(CentralitySymbol {
                name: row.get(0)?,
                kind: row.get(1)?,
                language: row.get(2)?,
                file_path: row.get(3)?,
                signature: row.get(4)?,
                reference_score: row.get(5)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Return the top `limit` files ranked by composite score:
    /// `COALESCE(line_count, 0) + COUNT(symbols) * 10`
    ///
    /// Results are ordered by composite score DESC.
    pub fn get_file_hotspots(&self, limit: usize) -> Result<Vec<FileHotspot>> {
        let mut stmt = self.conn.prepare(
            "SELECT f.path,
                    f.language,
                    COALESCE(f.line_count, 0)  AS line_count,
                    f.size,
                    COUNT(s.id)                AS symbol_count
             FROM files f
             LEFT JOIN symbols s ON s.file_path = f.path
             GROUP BY f.path
             ORDER BY (COALESCE(f.line_count, 0) + COUNT(s.id) * 10) DESC
             LIMIT ?1",
        )?;

        let rows = stmt.query_map([limit as i64], |row| {
            Ok(FileHotspot {
                path: row.get(0)?,
                language: row.get(1)?,
                line_count: row.get(2)?,
                size: row.get(3)?,
                symbol_count: row.get(4)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Return aggregate workspace statistics:
    /// total files, symbols, lines, relationships, and distinct language count.
    ///
    /// Files with an empty language string are excluded from language_count.
    pub fn get_aggregate_stats(&self) -> Result<AggregateStats> {
        let total_files: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))?;

        let total_symbols: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))?;

        let total_lines: i64 = self
            .conn
            .query_row("SELECT COALESCE(SUM(line_count), 0) FROM files", [], |row| {
                row.get(0)
            })?;

        let total_relationships: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM relationships", [], |row| row.get(0))?;

        let language_count: i64 = self.conn.query_row(
            "SELECT COUNT(DISTINCT language) FROM files WHERE language != ''",
            [],
            |row| row.get(0),
        )?;

        Ok(AggregateStats {
            total_files,
            total_symbols,
            total_lines,
            total_relationships,
            language_count,
        })
    }
}
