// Database analytics queries for the intelligence dashboard

use anyhow::Result;
use rusqlite::params;
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

/// SQL fragment for excluding test file paths. Matches the same patterns as
/// `is_test_path()` in `search/scoring.rs` and Step 1b/Step 4 in `compute_reference_scores`.
const TEST_PATH_EXCLUSION: &str = "
    AND NOT (
        file_path LIKE '%/test/%' OR file_path LIKE '%/tests/%'
        OR file_path LIKE 'test/%' OR file_path LIKE 'tests/%'
        OR file_path LIKE '%/spec/%' OR file_path LIKE '%/specs/%'
        OR file_path LIKE 'spec/%' OR file_path LIKE 'specs/%'
        OR file_path LIKE '%/__tests__/%' OR file_path LIKE '__tests__/%'
        OR file_path LIKE '%.Tests/%' OR file_path LIKE '%.Test/%'
        OR file_path LIKE '%/test\\_%' ESCAPE '\\'
        OR file_path LIKE 'test\\_%' ESCAPE '\\'
        OR file_path LIKE '%\\_test.go' ESCAPE '\\'
        OR file_path LIKE '%\\_test.c' ESCAPE '\\' OR file_path LIKE '%\\_test.cc' ESCAPE '\\'
        OR file_path LIKE '%\\_test.cpp' ESCAPE '\\'
        OR file_path LIKE '%.test.ts' OR file_path LIKE '%.test.tsx'
        OR file_path LIKE '%.test.js' OR file_path LIKE '%.test.jsx'
        OR file_path LIKE '%.spec.ts' OR file_path LIKE '%.spec.tsx'
        OR file_path LIKE '%.spec.js' OR file_path LIKE '%.spec.jsx'
    )";

/// Symbol kinds that are "documentable" (should have doc comments when public).
const DOCUMENTABLE_KINDS: &str =
    "'function','method','class','struct','interface','trait','enum','type','module','namespace'";

/// Symbol kinds checked for dead code (callable things, not type definitions).
const DEAD_CODE_KINDS: &str = "'function','method'";

/// SQL fragment for excluding fixture/example/doc directories.
/// These contain sample code that's intentionally unreferenced.
const NON_SOURCE_EXCLUSION: &str = "
    AND file_path NOT LIKE 'fixtures/%'
    AND file_path NOT LIKE '%/fixtures/%'
    AND file_path NOT LIKE 'examples/%'
    AND file_path NOT LIKE '%/examples/%'
    AND file_path NOT LIKE 'docs/%'
    AND file_path NOT LIKE '%/docs/%'";

/// Doc coverage statistics for a workspace.
#[derive(Debug, Clone, Serialize)]
pub struct DocCoverageStats {
    pub total_public: i64,
    pub documented: i64,
    pub coverage_pct: f64,
    pub by_language: Vec<LanguageDocCoverage>,
}

/// Per-language doc coverage breakdown.
#[derive(Debug, Clone, Serialize)]
pub struct LanguageDocCoverage {
    pub language: String,
    pub total: i64,
    pub documented: i64,
    pub coverage_pct: f64,
}

/// An undocumented public symbol, ranked by centrality.
#[derive(Debug, Clone, Serialize)]
pub struct UndocumentedSymbol {
    pub name: String,
    pub kind: String,
    pub language: String,
    pub file_path: String,
    pub signature: Option<String>,
    pub reference_score: f64,
}

/// A public symbol with zero incoming references (potential dead code).
#[derive(Debug, Clone, Serialize)]
pub struct DeadCodeCandidate {
    pub name: String,
    pub kind: String,
    pub language: String,
    pub file_path: String,
    pub signature: Option<String>,
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
             HAVING COUNT(s.id) > 0
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

        let total_symbols: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))?;

        let total_lines: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(line_count), 0) FROM files",
            [],
            |row| row.get(0),
        )?;

        let total_relationships: i64 =
            self.conn
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

    /// Return doc coverage statistics: how many public documentable symbols
    /// have a non-empty doc_comment.
    pub fn get_doc_coverage(&self) -> Result<DocCoverageStats> {
        let sql = format!(
            "SELECT
                COUNT(*) as total,
                COALESCE(SUM(CASE WHEN doc_comment IS NOT NULL AND doc_comment != '' THEN 1 ELSE 0 END), 0) as documented
             FROM symbols
             WHERE visibility = 'public'
               AND kind IN ({DOCUMENTABLE_KINDS})
               AND content_type IS NULL
               {TEST_PATH_EXCLUSION}
               {NON_SOURCE_EXCLUSION}"
        );
        let (total, documented): (i64, i64) =
            self.conn.query_row(&sql, [], |row| Ok((row.get(0)?, row.get(1)?)))?;

        let coverage_pct = if total > 0 {
            (documented as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        // Per-language breakdown
        let lang_sql = format!(
            "SELECT language,
                    COUNT(*) as total,
                    SUM(CASE WHEN doc_comment IS NOT NULL AND doc_comment != '' THEN 1 ELSE 0 END) as documented
             FROM symbols
             WHERE visibility = 'public'
               AND kind IN ({DOCUMENTABLE_KINDS})
               AND content_type IS NULL
               {TEST_PATH_EXCLUSION}
               {NON_SOURCE_EXCLUSION}
             GROUP BY language
             ORDER BY COUNT(*) DESC"
        );
        let mut stmt = self.conn.prepare(&lang_sql)?;
        let by_language = stmt
            .query_map([], |row| {
                let total: i64 = row.get(1)?;
                let documented: i64 = row.get(2)?;
                let pct = if total > 0 {
                    (documented as f64 / total as f64) * 100.0
                } else {
                    0.0
                };
                Ok(LanguageDocCoverage {
                    language: row.get(0)?,
                    total,
                    documented,
                    coverage_pct: pct,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(DocCoverageStats {
            total_public: total,
            documented,
            coverage_pct,
            by_language,
        })
    }

    /// Return public documentable symbols that lack doc comments,
    /// ordered by reference_score DESC (highest-centrality gaps first).
    pub fn get_undocumented_symbols(&self, limit: usize) -> Result<Vec<UndocumentedSymbol>> {
        let sql = format!(
            "SELECT name, kind, language, file_path, signature, reference_score
             FROM symbols
             WHERE visibility = 'public'
               AND kind IN ({DOCUMENTABLE_KINDS})
               AND (doc_comment IS NULL OR doc_comment = '')
               AND content_type IS NULL
               {TEST_PATH_EXCLUSION}
               {NON_SOURCE_EXCLUSION}
             ORDER BY reference_score DESC
             LIMIT ?1"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let results = stmt
            .query_map(params![limit as i64], |row| {
                Ok(UndocumentedSymbol {
                    name: row.get(0)?,
                    kind: row.get(1)?,
                    language: row.get(2)?,
                    file_path: row.get(3)?,
                    signature: row.get(4)?,
                    reference_score: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(results)
    }

    /// Count public functions/methods with zero incoming references.
    pub fn count_dead_code_candidates(&self) -> Result<i64> {
        let sql = format!(
            "SELECT COUNT(*)
             FROM symbols
             WHERE visibility = 'public'
               AND kind IN ({DEAD_CODE_KINDS})
               AND reference_score = 0.0
               AND content_type IS NULL
               {TEST_PATH_EXCLUSION}
               {NON_SOURCE_EXCLUSION}"
        );
        Ok(self.conn.query_row(&sql, [], |row| row.get(0))?)
    }

    /// Return public functions/methods with zero incoming references (dead code candidates).
    /// Excludes test files, fixture directories, and non-callable symbol kinds.
    pub fn get_dead_code_candidates(&self, limit: usize) -> Result<Vec<DeadCodeCandidate>> {
        let sql = format!(
            "SELECT name, kind, language, file_path, signature
             FROM symbols
             WHERE visibility = 'public'
               AND kind IN ({DEAD_CODE_KINDS})
               AND reference_score = 0.0
               AND content_type IS NULL
               {TEST_PATH_EXCLUSION}
               {NON_SOURCE_EXCLUSION}
             ORDER BY name
             LIMIT ?1"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let results = stmt
            .query_map(params![limit as i64], |row| {
                Ok(DeadCodeCandidate {
                    name: row.get(0)?,
                    kind: row.get(1)?,
                    language: row.get(2)?,
                    file_path: row.get(3)?,
                    signature: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(results)
    }
}
