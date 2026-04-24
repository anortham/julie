//! Test quality metrics engine.
//!
//! Analyzes test function bodies for assertion density, mock usage,
//! and quality tiering. Runs at post-indexing time after all symbols
//! are stored in SQLite with their code_context.
//!
//! Two evidence paths:
//! 1. **Identifier-based** (high confidence): counts Call-kind identifiers
//!    matching framework-specific lists from TestEvidenceConfig TOML.
//! 2. **Regex fallback** (low confidence): scans stripped body text with
//!    language-agnostic patterns when identifier data is unavailable.
//!
//! Language-agnostic: patterns cover Rust, Python, Java, C#, JS/TS,
//! Go, Ruby, Swift, PHP, Kotlin, and more.

use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use tracing::{debug, info, warn};

use crate::database::SymbolDatabase;
use crate::search::LanguageConfigs;

// =============================================================================
// Public types
// =============================================================================

/// Assessment of a test function's quality, with confidence scoring.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TestQualityAssessment {
    pub tier: TestQualityTier,
    pub confidence: f32,
    pub evidence: QualityEvidence,
}

/// Quality tier classification for a test function.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TestQualityTier {
    Thorough,
    Adequate,
    Thin,
    Stub,
    Unknown,
    NotApplicable,
}

impl TestQualityTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Thorough => "thorough",
            Self::Adequate => "adequate",
            Self::Thin => "thin",
            Self::Stub => "stub",
            Self::Unknown => "unknown",
            Self::NotApplicable => "n/a",
        }
    }
}

/// Evidence collected from analyzing a test function body.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QualityEvidence {
    pub assertion_count: u32,
    pub assertion_source: EvidenceSource,
    pub has_error_testing: bool,
    pub mock_count: u32,
    pub body_lines: u32,
}

/// Where the assertion evidence came from.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceSource {
    Identifier,
    Regex,
    None,
}

/// Summary stats from running quality analysis across all test symbols.
#[derive(Debug, Clone, Default)]
pub struct TestQualityStats {
    pub total_tests: usize,
    pub thorough: usize,
    pub adequate: usize,
    pub thin: usize,
    pub stub: usize,
    pub unknown: usize,
    pub not_applicable: usize,
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
            // Rust macros (no trailing \b -- `!` is non-word so \b won't match before `(`)
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
            // Count `expect(` as the assertion anchor -- don't also count
            // .toBe()/.toEqual() chains to avoid double-counting.
            r"\bexpect\(",
            // Go
            r"\bt\.Error\b",
            r"\bt\.Fatal\b",
            r"\bt\.Fail\b",
            r"\brequire\.\w+\(",
            // Swift (XCTest)
            r"\bXCTAssert",
            // PHP (PHPUnit) -- assertEquals, assertTrue, etc. are already
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
// Placeholder detection
// =============================================================================

/// Placeholder patterns that indicate a stub test body.
const PLACEHOLDER_PATTERNS: &[&str] = &[
    "pass",
    "...",
    "todo!()",
    "unimplemented!()",
    "// todo",
    "# todo",
    "// fixme",
    "# fixme",
];

/// Check if a body is a placeholder (empty, whitespace-only, or contains
/// only a placeholder statement like `pass`, `todo!()`, etc.).
fn is_placeholder_body(body: &str) -> bool {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return true;
    }

    // Strip leading/trailing braces for Rust/C-style function bodies
    let inner = trimmed
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .map(|s| s.trim())
        .unwrap_or(trimmed);

    if inner.is_empty() {
        return true;
    }

    let lower = inner.to_lowercase();
    PLACEHOLDER_PATTERNS
        .iter()
        .any(|p| lower == *p || lower.starts_with(&format!("{} ", p)))
}

// =============================================================================
// Comment/string stripping (prevents false-positive pattern matches)
// =============================================================================

/// State machine for stripping comments and string literal contents.
///
/// Replaces content inside comments and strings with spaces, preserving
/// newlines so that line counts remain accurate. Language-agnostic: handles
/// the common comment/string syntaxes across all 34 supported languages.
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
/// This doesn't need to be perfect for every language edge case; it prevents
/// the most common false positives (assertions in comments, mock patterns in
/// string literals).
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
                // Normal character -- keep as-is
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
                    // Escaped character -- replace both with spaces
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
                    // Escaped character -- replace both with spaces
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
// Core analysis functions
// =============================================================================

/// Analyze a test function body using regex patterns (fallback path).
///
/// Strips comments and string literal contents before scanning for patterns,
/// preventing false positives from assertion/mock keywords in comments or strings.
/// Counts assertions, mocks, error-testing patterns, and computes body lines.
pub fn analyze_test_body(body: &str) -> TestQualityAssessment {
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

    // Regex path: use assess_test_quality with no identifier evidence
    assess_test_quality(
        Some("test_case"),
        Some(body),
        assertion_count,
        has_error_testing,
        mock_count,
        false, // no identifier evidence
    )
}

/// Count total non-overlapping pattern matches across all patterns in a body.
fn count_pattern_matches(body: &str, patterns: &[Regex]) -> u32 {
    let mut total = 0u32;
    for pat in patterns {
        total += pat.find_iter(body).count() as u32;
    }
    total
}

/// Core scoring function: classify test quality from evidence.
///
/// Supports two evidence paths:
/// - **Identifier-based** (`has_identifier_evidence = true`): high confidence
///   (0.8-0.9) using framework-specific identifier counts.
/// - **Regex fallback** (`has_identifier_evidence = false`): lower confidence
///   (0.3-0.5) using pattern matching on body text.
///
/// Key invariant: regex with 0 assertions yields Unknown, not Stub. We admit
/// ignorance rather than claiming deficiency when evidence is weak.
pub fn assess_test_quality(
    test_role: Option<&str>,
    body: Option<&str>,
    assertion_count: u32,
    has_error_testing: bool,
    mock_count: u32,
    has_identifier_evidence: bool,
) -> TestQualityAssessment {
    // Non-scorable roles are not applicable
    match test_role {
        Some("fixture_setup" | "fixture_teardown" | "test_container") => {
            return TestQualityAssessment {
                tier: TestQualityTier::NotApplicable,
                confidence: 1.0,
                evidence: QualityEvidence {
                    assertion_count: 0,
                    assertion_source: EvidenceSource::None,
                    has_error_testing: false,
                    mock_count: 0,
                    body_lines: body.map(|b| count_non_empty_lines(b)).unwrap_or(0),
                },
            };
        }
        _ => {}
    }

    // No body or placeholder body -> Stub with full confidence
    match body {
        None => {
            return TestQualityAssessment {
                tier: TestQualityTier::Stub,
                confidence: 1.0,
                evidence: QualityEvidence {
                    assertion_count: 0,
                    assertion_source: EvidenceSource::None,
                    has_error_testing: false,
                    mock_count: 0,
                    body_lines: 0,
                },
            };
        }
        Some(b) if is_placeholder_body(b) => {
            return TestQualityAssessment {
                tier: TestQualityTier::Stub,
                confidence: 1.0,
                evidence: QualityEvidence {
                    assertion_count: 0,
                    assertion_source: EvidenceSource::None,
                    has_error_testing: false,
                    mock_count: 0,
                    body_lines: count_non_empty_lines(b),
                },
            };
        }
        _ => {}
    }

    let body_lines = body.map(|b| count_non_empty_lines(b)).unwrap_or(0);

    if has_identifier_evidence {
        // Identifier path: higher confidence
        let tier = classify_tier_from_counts(assertion_count, mock_count, has_error_testing);
        let confidence = if has_error_testing || assertion_count >= 3 {
            0.9
        } else {
            0.85
        };
        TestQualityAssessment {
            tier,
            confidence,
            evidence: QualityEvidence {
                assertion_count,
                assertion_source: EvidenceSource::Identifier,
                has_error_testing,
                mock_count,
                body_lines,
            },
        }
    } else {
        // Regex fallback: lower confidence
        if assertion_count == 0 {
            // Key invariant: regex found nothing -> Unknown, not Stub
            TestQualityAssessment {
                tier: TestQualityTier::Unknown,
                confidence: 0.3,
                evidence: QualityEvidence {
                    assertion_count: 0,
                    assertion_source: EvidenceSource::Regex,
                    has_error_testing,
                    mock_count,
                    body_lines,
                },
            }
        } else {
            let tier = classify_tier_from_counts(assertion_count, mock_count, has_error_testing);
            let confidence = if assertion_count >= 3 { 0.5 } else { 0.4 };
            TestQualityAssessment {
                tier,
                confidence,
                evidence: QualityEvidence {
                    assertion_count,
                    assertion_source: EvidenceSource::Regex,
                    has_error_testing,
                    mock_count,
                    body_lines,
                },
            }
        }
    }
}

/// Classify quality tier from assertion/mock/error counts.
///
/// - 3+ assertions + error testing -> Thorough
/// - 3+ assertions -> Thorough
/// - mock_count > 0 + 2+ assertions -> Thorough
/// - has_error_testing + 1+ assertions -> Thorough
/// - 2+ assertions -> Adequate
/// - 1 assertion -> Thin
/// - 0 assertions -> Stub
fn classify_tier_from_counts(
    assertion_count: u32,
    mock_count: u32,
    has_error_testing: bool,
) -> TestQualityTier {
    if assertion_count == 0 {
        return TestQualityTier::Stub;
    }
    // Check thorough BEFORE thin: a test with error testing or mocks
    // is thorough even with low assertion counts.
    if assertion_count >= 3 || has_error_testing || (mock_count > 0 && assertion_count >= 2) {
        return TestQualityTier::Thorough;
    }
    if assertion_count == 1 {
        return TestQualityTier::Thin;
    }
    TestQualityTier::Adequate
}

/// Count non-empty lines in a body string.
fn count_non_empty_lines(body: &str) -> u32 {
    body.lines().filter(|line| !line.trim().is_empty()).count() as u32
}

// =============================================================================
// Pipeline integration
// =============================================================================

/// Compute quality metrics for all test symbols in the database.
///
/// Queries symbols with `metadata["is_test"] = true`, gathers identifier
/// evidence from the identifiers table, and updates metadata with quality
/// assessments including confidence scores.
pub fn compute_test_quality_metrics(
    db: &SymbolDatabase,
    language_configs: &LanguageConfigs,
) -> Result<TestQualityStats> {
    let mut stats = TestQualityStats::default();

    // Query all test symbols
    let mut stmt = db.conn.prepare(
        "SELECT id, code_context, metadata, language FROM symbols \
         WHERE json_extract(metadata, '$.is_test') = 1",
    )?;

    let rows: Vec<(String, Option<String>, Option<String>, String)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();

    debug!(
        "Found {} test symbols to analyze for quality metrics",
        rows.len()
    );

    // Prepare identifier query (reused per symbol)
    let mut id_stmt = db.conn.prepare(
        "SELECT LOWER(name) FROM identifiers \
         WHERE containing_symbol_id = ?1 AND kind = 'call'",
    )?;

    // Wrap all UPDATEs in a single transaction for performance on large codebases
    db.conn.execute_batch("BEGIN")?;
    let result = (|| -> Result<()> {
        for (id, code_context, metadata_str, language) in &rows {
            stats.total_tests += 1;

            // Extract test_role from existing metadata
            let existing_meta: serde_json::Value = metadata_str
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_else(|| serde_json::json!({}));

            let test_role = existing_meta
                .get("test_role")
                .and_then(|v| v.as_str())
                .map(String::from);

            // Gather identifier evidence for this symbol
            let call_names: Vec<String> = id_stmt
                .query_map(rusqlite::params![id], |row| row.get::<_, String>(0))?
                .filter_map(|r| r.ok())
                .collect();

            // Match identifiers against framework-specific evidence config
            let evidence_config = language_configs.get(language).map(|cfg| &cfg.test_evidence);

            let has_identifier_evidence = evidence_config.is_some() && !call_names.is_empty();

            let (id_assertion_count, id_error_count, id_mock_count) =
                if let Some(ev_cfg) = evidence_config {
                    count_identifier_evidence(&call_names, ev_cfg)
                } else {
                    (0, 0, 0)
                };

            // Determine body status
            let body = code_context.as_deref().filter(|b| !b.trim().is_empty());
            let has_body = body.is_some();

            if !has_body {
                stats.no_body += 1;
            }

            // Build assessment
            let assessment = if has_identifier_evidence {
                // Identifier path
                assess_test_quality(
                    test_role.as_deref(),
                    body,
                    id_assertion_count,
                    id_error_count > 0,
                    id_mock_count,
                    true,
                )
            } else {
                // Regex fallback: analyze body text
                let (regex_assertions, regex_has_error, regex_mocks) = match body {
                    Some(b) => {
                        let stripped = strip_comments_and_strings(b);
                        let a = count_pattern_matches(&stripped, assertion_patterns());
                        let e = error_testing_patterns()
                            .iter()
                            .any(|pat| pat.is_match(&stripped));
                        let m = count_pattern_matches(&stripped, mock_patterns());
                        (a, e, m)
                    }
                    None => (0, false, 0),
                };
                assess_test_quality(
                    test_role.as_deref(),
                    body,
                    regex_assertions,
                    regex_has_error,
                    regex_mocks,
                    false,
                )
            };

            // Track tier stats
            match assessment.tier {
                TestQualityTier::Thorough => stats.thorough += 1,
                TestQualityTier::Adequate => stats.adequate += 1,
                TestQualityTier::Thin => stats.thin += 1,
                TestQualityTier::Stub => stats.stub += 1,
                TestQualityTier::Unknown => stats.unknown += 1,
                TestQualityTier::NotApplicable => stats.not_applicable += 1,
            }

            // Build quality JSON for metadata
            let quality_json = serde_json::json!({
                "quality_tier": assessment.tier.as_str(),
                "confidence": assessment.confidence,
                "assertion_count": assessment.evidence.assertion_count,
                "assertion_source": match assessment.evidence.assertion_source {
                    EvidenceSource::Identifier => "identifier",
                    EvidenceSource::Regex => "regex",
                    EvidenceSource::None => "none",
                },
                "has_error_testing": assessment.evidence.has_error_testing,
                "mock_count": assessment.evidence.mock_count,
                "body_lines": assessment.evidence.body_lines,
            });

            // Parse existing metadata, merge in test_quality, update
            let mut meta = existing_meta.clone();
            meta["test_quality"] = quality_json;

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
        "Test quality metrics: {} total, {} thorough, {} adequate, {} thin, {} stub, {} unknown, {} n/a, {} no_body",
        stats.total_tests,
        stats.thorough,
        stats.adequate,
        stats.thin,
        stats.stub,
        stats.unknown,
        stats.not_applicable,
        stats.no_body
    );

    Ok(stats)
}

/// Count identifier matches against framework-specific evidence lists.
///
/// Returns (assertion_count, error_assertion_count, mock_count).
fn count_identifier_evidence(
    call_names: &[String],
    evidence_config: &crate::search::language_config::TestEvidenceConfig,
) -> (u32, u32, u32) {
    let mut assertion_count = 0u32;
    let mut error_count = 0u32;
    let mut mock_count = 0u32;

    for name in call_names {
        let lower = name.to_lowercase();
        if evidence_config
            .assertion_identifiers
            .iter()
            .any(|a| lower == *a || lower.contains(a.as_str()))
        {
            assertion_count += 1;
        }
        if evidence_config
            .error_assertion_identifiers
            .iter()
            .any(|e| lower == *e || lower.contains(e.as_str()))
        {
            error_count += 1;
        }
        if evidence_config
            .mock_identifiers
            .iter()
            .any(|m| lower == *m || lower.contains(m.as_str()))
        {
            mock_count += 1;
        }
    }

    (assertion_count, error_count, mock_count)
}
