//! Test quality metrics engine.
//!
//! Analyzes test function bodies for assertion density, mock usage,
//! and quality tiering. Runs at post-indexing time after all symbols
//! are stored in SQLite with their code_context.
//!
//! Language-agnostic: patterns cover Rust, Python, Java, C#, JS/TS,
//! Go, Ruby, Swift, PHP, Kotlin, and more.

use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use tracing::{debug, info, warn};

use crate::database::SymbolDatabase;

// =============================================================================
// Public types
// =============================================================================

/// Quality metrics computed from analyzing a test function's body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestQualityMetrics {
    pub assertion_count: u32,
    pub mock_count: u32,
    pub body_lines: u32,
    pub assertion_density: f32,
    pub has_error_testing: bool,
    pub quality_tier: String,
}

/// Summary stats from running quality analysis across all test symbols.
#[derive(Debug, Clone, Default)]
pub struct TestQualityStats {
    pub total_tests: usize,
    pub thorough: usize,
    pub adequate: usize,
    pub thin: usize,
    pub stub: usize,
    pub no_body: usize,
}

// =============================================================================
// Compiled regex patterns (OnceLock for thread-safe lazy init)
// =============================================================================

/// All assertion patterns across languages. Each match increments assertion_count.
fn assertion_patterns() -> &'static [Regex] {
    static PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        let raw = [
            // Rust macros (no trailing \b — `!` is non-word so \b won't match before `(`)
            r"\bassert_eq!",
            r"\bassert_ne!",
            r"\bassert!\(",
            // Generic (function call style)
            r"\bassert\(",
            r"\bAssert\.",
            // Python
            r"\bself\.assert",
            r"\bpytest\.raises\b",
            // Java / C# / JUnit
            r"\bassertEquals\b",
            r"\bassertTrue\b",
            r"\bassertFalse\b",
            r"\bassertNull\b",
            r"\bassertNotNull\b",
            r"\bassertThrows\b",
            // JS / TS / Ruby (Jest, Vitest, Chai, RSpec)
            // Count `expect(` as the assertion anchor — don't also count
            // .toBe()/.toEqual() chains to avoid double-counting.
            r"\bexpect\(",
            // Go
            r"\bt\.Error\b",
            r"\bt\.Fatal\b",
            r"\bt\.Fail\b",
            r"\brequire\.\w+\(",
            // Swift (XCTest)
            r"\bXCTAssert",
            // PHP (PHPUnit) — assertEquals, assertTrue, etc. are already
            // matched by the Java/JUnit patterns above. No separate PHP
            // pattern needed to avoid double-counting.
            // C# FluentAssertions
            r"\bShould\b",
            r"\bExpect\(",
        ];
        raw.iter()
            .filter_map(|p| match Regex::new(p) {
                Ok(r) => Some(r),
                Err(e) => {
                    warn!("Bad assertion pattern '{}': {}", p, e);
                    None
                }
            })
            .collect()
    })
}

/// Mock/stub patterns. Each match increments mock_count.
fn mock_patterns() -> &'static [Regex] {
    static PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        let raw = [
            r"\bmock\b",
            r"\bMock\b",
            r"\bstub\b",
            r"\bspy\b",
            r"\bpatch\(",
            r"\bjest\.fn\(",
            r"\bMockito\b",
            // @Mock is already caught by \bMock\b above.
            // @InjectMocks needs its own pattern (no word boundary before "Mock" in "InjectMocks").
            r"@InjectMocks\b",
            r"\bsinon\b",
            r"\bgomock\b",
            r"\bMoq\b",
            r"\bmockk\b",
        ];
        raw.iter()
            .filter_map(|p| match Regex::new(p) {
                Ok(r) => Some(r),
                Err(e) => {
                    warn!("Bad mock pattern '{}': {}", p, e);
                    None
                }
            })
            .collect()
    })
}

/// Error testing patterns. Any match sets has_error_testing = true.
fn error_testing_patterns() -> &'static [Regex] {
    static PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        let raw = [
            r"\bexpects_err\b",
            r"\bshould_err\b",
            r"\bassertThrows\b",
            r"\bpytest\.raises\b",
            r"\.toThrow\b",
            r"\.rejects\b",
            r"\bExpectedException\b",
            r#"\b@Test\(expected"#,
        ];
        raw.iter()
            .filter_map(|p| match Regex::new(p) {
                Ok(r) => Some(r),
                Err(e) => {
                    warn!("Bad error-testing pattern '{}': {}", p, e);
                    None
                }
            })
            .collect()
    })
}

// =============================================================================
// Comment/string stripping (prevents false-positive pattern matches)
// =============================================================================

/// State machine for stripping comments and string literal contents.
///
/// Replaces content inside comments and strings with spaces, preserving
/// newlines so that line counts remain accurate. Language-agnostic: handles
/// the common comment/string syntaxes across all 33 supported languages.
#[derive(Debug, Clone, Copy, PartialEq)]
enum StripState {
    Normal,
    SingleLineComment,
    BlockComment,
    DoubleQuoteString,
    SingleQuoteString,
}

/// Strip comment and string literal contents from source text.
///
/// Walks the text char-by-char with a simple state machine.
/// - Single-line comments (`//`, `#`, `--`): content replaced with spaces
/// - Block comments (`/* ... */`): content replaced with spaces (newlines preserved)
/// - String literals (`"..."`, `'...'`): content replaced with spaces (delimiters kept)
/// - Escaped quotes (`\"`, `\'`) within strings are handled
///
/// This doesn't need to be perfect for every language edge case — it just
/// prevents the most common false positives (assertions in comments, mock
/// patterns in string literals).
fn strip_comments_and_strings(body: &str) -> String {
    let chars: Vec<char> = body.chars().collect();
    let len = chars.len();
    let mut result = String::with_capacity(body.len());
    let mut state = StripState::Normal;
    let mut i = 0;

    while i < len {
        let c = chars[i];
        let next = if i + 1 < len {
            Some(chars[i + 1])
        } else {
            None
        };

        match state {
            StripState::Normal => {
                // Detect start of block comment: /*
                if c == '/' && next == Some('*') {
                    result.push(' ');
                    result.push(' ');
                    state = StripState::BlockComment;
                    i += 2;
                }
                // Detect start of single-line comment: //
                else if c == '/' && next == Some('/') {
                    result.push(' ');
                    result.push(' ');
                    state = StripState::SingleLineComment;
                    i += 2;
                }
                // Detect start of single-line comment: # (Python, Ruby, Bash, etc.)
                else if c == '#' {
                    result.push(' ');
                    state = StripState::SingleLineComment;
                    i += 1;
                }
                // Detect start of single-line comment: -- (Lua, SQL, Haskell)
                else if c == '-' && next == Some('-') {
                    result.push(' ');
                    result.push(' ');
                    state = StripState::SingleLineComment;
                    i += 2;
                }
                // Detect start of double-quoted string
                else if c == '"' {
                    result.push(c); // keep delimiter
                    state = StripState::DoubleQuoteString;
                    i += 1;
                }
                // Detect start of single-quoted string
                else if c == '\'' {
                    result.push(c); // keep delimiter
                    state = StripState::SingleQuoteString;
                    i += 1;
                }
                // Normal character — keep as-is
                else {
                    result.push(c);
                    i += 1;
                }
            }

            StripState::SingleLineComment => {
                if c == '\n' {
                    result.push('\n');
                    state = StripState::Normal;
                } else {
                    result.push(' ');
                }
                i += 1;
            }

            StripState::BlockComment => {
                if c == '*' && next == Some('/') {
                    result.push(' ');
                    result.push(' ');
                    state = StripState::Normal;
                    i += 2;
                } else if c == '\n' {
                    result.push('\n');
                    i += 1;
                } else {
                    result.push(' ');
                    i += 1;
                }
            }

            StripState::DoubleQuoteString => {
                if c == '\\' && next.is_some() {
                    // Escaped character — replace both with spaces
                    result.push(' ');
                    result.push(' ');
                    i += 2;
                } else if c == '"' {
                    result.push(c); // keep closing delimiter
                    state = StripState::Normal;
                    i += 1;
                } else if c == '\n' {
                    result.push('\n');
                    i += 1;
                } else {
                    result.push(' ');
                    i += 1;
                }
            }

            StripState::SingleQuoteString => {
                if c == '\\' && next.is_some() {
                    // Escaped character — replace both with spaces
                    result.push(' ');
                    result.push(' ');
                    i += 2;
                } else if c == '\'' {
                    result.push(c); // keep closing delimiter
                    state = StripState::Normal;
                    i += 1;
                } else if c == '\n' {
                    result.push('\n');
                    i += 1;
                } else {
                    result.push(' ');
                    i += 1;
                }
            }
        }
    }

    result
}

// =============================================================================
// Core analysis function
// =============================================================================

/// Analyze a test function body and return quality metrics.
///
/// Strips comments and string literal contents before scanning for patterns,
/// preventing false positives from assertion/mock keywords in comments or strings.
/// Counts assertions, mocks, error-testing patterns, computes density,
/// and assigns a quality tier.
pub fn analyze_test_body(body: &str) -> TestQualityMetrics {
    let stripped = strip_comments_and_strings(body);
    let non_empty_lines: Vec<&str> = stripped
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let body_lines = non_empty_lines.len() as u32;

    // Count assertion matches across all patterns (on stripped text)
    let assertion_count = count_pattern_matches(&stripped, assertion_patterns());

    // Count mock matches (on stripped text)
    let mock_count = count_pattern_matches(&stripped, mock_patterns());

    // Check for error testing (on stripped text)
    let has_error_testing = error_testing_patterns()
        .iter()
        .any(|pat| pat.is_match(&stripped));

    // Compute density
    let assertion_density = if body_lines > 0 {
        assertion_count as f32 / body_lines as f32
    } else {
        0.0
    };

    // Classify quality tier
    let quality_tier = classify_tier(
        assertion_count,
        mock_count,
        has_error_testing,
        assertion_density,
    );

    TestQualityMetrics {
        assertion_count,
        mock_count,
        body_lines,
        assertion_density,
        has_error_testing,
        quality_tier,
    }
}

/// Count total non-overlapping pattern matches across all patterns in a body.
fn count_pattern_matches(body: &str, patterns: &[Regex]) -> u32 {
    let mut total = 0u32;
    for pat in patterns {
        total += pat.find_iter(body).count() as u32;
    }
    total
}

/// Classify test quality tier based on metrics.
///
/// - **stub**: 0 assertions
/// - **thin**: 1 assertion OR assertion_density < 0.05
/// - **thorough**: >=3 assertions, OR has_error_testing, OR (mock_count > 0 AND assertion_count >= 2)
/// - **adequate**: everything else
fn classify_tier(
    assertion_count: u32,
    mock_count: u32,
    has_error_testing: bool,
    assertion_density: f32,
) -> String {
    if assertion_count == 0 {
        return "stub".to_string();
    }
    // Check thorough BEFORE thin — a test with error testing or mocks
    // is thorough even with low assertion counts.
    if assertion_count >= 3 || has_error_testing || (mock_count > 0 && assertion_count >= 2) {
        return "thorough".to_string();
    }
    if assertion_count == 1 || (assertion_count > 0 && assertion_density < 0.05) {
        return "thin".to_string();
    }
    "adequate".to_string()
}

// =============================================================================
// Pipeline integration
// =============================================================================

/// Compute quality metrics for all test symbols in the database.
///
/// Queries symbols with `metadata["is_test"] = true`, analyzes their
/// `code_context`, and updates their metadata with `test_quality` metrics.
pub fn compute_test_quality_metrics(db: &SymbolDatabase) -> Result<TestQualityStats> {
    let mut stats = TestQualityStats::default();

    // Query all test symbols
    let mut stmt = db.conn.prepare(
        "SELECT id, code_context, metadata FROM symbols WHERE json_extract(metadata, '$.is_test') = 1",
    )?;

    let rows: Vec<(String, Option<String>, Option<String>)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();

    debug!(
        "Found {} test symbols to analyze for quality metrics",
        rows.len()
    );

    // Wrap all UPDATEs in a single transaction for performance on large codebases
    db.conn.execute_batch("BEGIN")?;
    let result = (|| -> Result<()> {
        for (id, code_context, metadata_str) in &rows {
            stats.total_tests += 1;

            // Analyze the body (or treat None as empty)
            let metrics = match code_context.as_deref() {
                Some(body) if !body.trim().is_empty() => analyze_test_body(body),
                _ => {
                    stats.no_body += 1;
                    analyze_test_body("")
                }
            };

            // Track tier stats
            match metrics.quality_tier.as_str() {
                "thorough" => stats.thorough += 1,
                "adequate" => stats.adequate += 1,
                "thin" => stats.thin += 1,
                "stub" => stats.stub += 1,
                _ => {}
            }

            // Parse existing metadata, merge in test_quality, update
            let mut meta: serde_json::Value = metadata_str
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_else(|| serde_json::json!({}));

            meta["test_quality"] = serde_json::to_value(&metrics)?;

            let updated_metadata = serde_json::to_string(&meta)?;
            db.conn.execute(
                "UPDATE symbols SET metadata = ?1 WHERE id = ?2",
                rusqlite::params![updated_metadata, id],
            )?;
        }
        Ok(())
    })();

    match result {
        Ok(()) => db.conn.execute_batch("COMMIT")?,
        Err(e) => {
            let _ = db.conn.execute_batch("ROLLBACK");
            return Err(e);
        }
    }

    info!(
        "Test quality metrics: {} total, {} thorough, {} adequate, {} thin, {} stub, {} no_body",
        stats.total_tests, stats.thorough, stats.adequate, stats.thin, stats.stub, stats.no_body
    );

    Ok(stats)
}
