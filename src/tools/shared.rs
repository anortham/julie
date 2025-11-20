use anyhow::{anyhow, Result};
use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use serde::{Deserialize, Serialize};
use toon_format;
use tracing::{debug, warn};

/// Token-optimized response wrapper with confidence-based limiting
/// Inspired by codesearch's AIOptimizedResponse pattern
///
/// Designed for structured MCP output - agents parse JSON, format for humans
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizedResponse<T> {
    /// Tool that generated this response (enables routing and schema detection)
    /// Examples: "fast_search", "fast_refs", "fast_goto", "fuzzy_replace", "rename_symbol", "edit_symbol"
    pub tool: String,
    /// The main results (will be limited based on confidence)
    pub results: Vec<T>,
    /// Confidence score 0.0-1.0 (higher = more confident)
    pub confidence: f32,
    /// Total results found before limiting
    pub total_found: usize,
    /// Key insights or patterns discovered
    pub insights: Option<String>,
    /// Suggested next actions for the user (enables tool chaining)
    pub next_actions: Vec<String>,
}

impl<T> OptimizedResponse<T> {
    pub fn new(tool: impl Into<String>, results: Vec<T>, confidence: f32) -> Self {
        let total_found = results.len();
        Self {
            tool: tool.into(),
            results,
            confidence,
            total_found,
            insights: None,
            next_actions: Vec::new(),
        }
    }

    /// Limit results based on confidence and token constraints
    pub fn optimize_for_tokens(&mut self, max_results: Option<usize>) {
        let limit = if let Some(max) = max_results {
            max
        } else {
            // Dynamic limiting based on confidence
            if self.confidence > 0.9 {
                3
            }
            // High confidence = fewer results needed
            else if self.confidence > 0.7 {
                5
            }
            // Medium confidence
            else if self.confidence > 0.5 {
                8
            }
            // Lower confidence
            else {
                12
            } // Very low confidence = more results
        };

        if self.results.len() > limit {
            self.results.truncate(limit);
        }
    }

    pub fn with_insights(mut self, insights: String) -> Self {
        self.insights = Some(insights);
        self
    }

    pub fn with_next_actions(mut self, actions: Vec<String>) -> Self {
        self.next_actions = actions;
        self
    }
}

/// Blacklisted file extensions - binary and temporary files to exclude from indexing
pub const BLACKLISTED_EXTENSIONS: &[&str] = &[
    // Binary files
    ".dll", ".exe", ".pdb", ".so", ".dylib", ".lib", ".a", ".o", ".obj", ".bin",
    // Media files
    ".jpg", ".jpeg", ".png", ".gif", ".bmp", ".ico", ".svg", ".webp", ".tiff", ".mp3", ".mp4",
    ".avi", ".mov", ".wmv", ".flv", ".webm", ".mkv", ".wav", // Archives
    ".zip", ".rar", ".7z", ".tar", ".gz", ".bz2", ".xz", ".dmg", ".pkg",
    // Database files
    ".db", ".sqlite", ".sqlite3", ".mdf", ".ldf", ".bak", // Temporary files
    ".tmp", ".temp", ".cache", ".swp", ".swo", ".lock", ".pid",
    // Logs and other large files
    ".log", ".dump", ".core", // Font files
    ".ttf", ".otf", ".woff", ".woff2", ".eot", // Other binary formats
    ".pdf", ".doc", ".docx", ".xls", ".xlsx", ".ppt", ".pptx",
];

/// Blacklisted directory names - directories to exclude from indexing
pub const BLACKLISTED_DIRECTORIES: &[&str] = &[
    // Version control
    ".git",
    ".svn",
    ".hg",
    ".bzr",
    // IDE and editor directories
    ".vs",
    ".vscode",
    ".idea",
    ".eclipse",
    // Build and output directories
    "bin",
    "obj",
    "build",
    "dist",
    "out",
    "target",
    "Debug",
    "Release",
    // Package managers
    "node_modules",
    "packages",
    ".npm",
    "bower_components",
    "vendor",
    // Test and coverage
    "TestResults",
    "coverage",
    "__pycache__",
    ".pytest_cache",
    ".coverage",
    // Temporary and cache
    ".cache",
    ".temp",
    ".tmp",
    "tmp",
    "temp",
    // Our own directories
    ".julie",
    ".coa",
    ".codenav",
    // Other common exclusions
    ".sass-cache",
    ".nuxt",
    ".next",
    "Pods",
    "DerivedData",
];

/// File extensions that are likely to contain code and should be indexed
#[allow(dead_code)]
pub const KNOWN_CODE_EXTENSIONS: &[&str] = &[
    // Core languages (supported by extractors)
    ".rs",
    ".ts",
    ".tsx",
    ".js",
    ".jsx",
    ".py",
    ".java",
    ".cs",
    ".php",
    ".rb",
    ".swift",
    ".kt",
    ".go",
    ".cpp",
    ".cc",
    ".cxx",
    ".c",
    ".h",
    ".hpp",
    ".lua",
    ".sql",
    ".html",
    ".css",
    ".vue",
    ".razor",
    ".bash",
    ".sh",
    ".ps1",
    ".zig",
    ".dart",
    // Additional text-based formats worth indexing
    ".json",
    ".xml",
    ".yaml",
    ".yml",
    ".toml",
    ".ini",
    ".cfg",
    ".conf",
    ".md",
    ".txt",
    ".rst",
    ".asciidoc",
    ".tex",
    ".org",
    ".dockerfile",
    ".gitignore",
    ".gitattributes",
    ".editorconfig",
    ".eslintrc",
    ".prettierrc",
    ".babelrc",
    ".tsconfig",
    ".jsconfig",
    ".cargo",
    ".gradle",
    ".maven",
    ".sbt",
    ".mix",
    ".cabal",
    ".stack",
];

/// Generic helper for TOON/JSON output formatting
///
/// Centralizes TOON encoding, auto mode logic, and fallback handling across all tools.
/// Eliminates ~120 lines of duplicated code across fast_refs, fast_goto, find_logic, and trace_call_path.
///
/// # Parameters
/// - `result_data`: The serializable result data to encode
/// - `output_format`: Output format option ("toon", "auto", "json", None)
/// - `auto_threshold`: Threshold for auto mode (if result_count >= threshold, use TOON)
/// - `result_count`: Number of results (used for auto mode decision)
/// - `tool_name`: Name of the tool (for debug logging)
///
/// # Returns
/// - TOON mode: `CallToolResult` with text_content only (no structured_content)
/// - JSON mode: `CallToolResult` with structured_content only (empty text_content)
/// - Auto mode: TOON if >= threshold, otherwise JSON
/// - Fallback: Always falls back to structured JSON if TOON encoding fails
///
/// # Example
/// ```rust
/// let result = MyToolResult { /* ... */ };
/// let call_result = create_toonable_result(
///     &result,
///     Some("auto"),
///     5,
///     result.items.len(),
///     "my_tool"
/// )?;
/// ```
pub fn create_toonable_result<T: Serialize>(
    result_data: &T,
    output_format: Option<&str>,
    auto_threshold: usize,
    result_count: usize,
    tool_name: &str,
) -> Result<CallToolResult> {
    match output_format {
        Some("toon") => {
            // TOON mode: Return ONLY TOON in text, NO structured content
            match toon_format::encode_default(result_data) {
                Ok(toon) => {
                    debug!("✅ Encoded {} results to TOON ({} chars)", tool_name, toon.len());
                    Ok(CallToolResult::text_content(vec![TextContent::from(toon)]))
                }
                Err(e) => {
                    warn!("❌ TOON encoding failed for {}: {}, falling back to JSON", tool_name, e);
                    // Fallback to structured JSON
                    let structured = serde_json::to_value(result_data)?;
                    let structured_map = if let serde_json::Value::Object(map) = structured {
                        map
                    } else {
                        return Err(anyhow!("Expected JSON object"));
                    };
                    Ok(CallToolResult::text_content(vec![])
                        .with_structured_content(structured_map))
                }
            }
        }
        Some("auto") => {
            // Auto mode: TOON for >= threshold results (text only), JSON for small responses
            if result_count >= auto_threshold {
                match toon_format::encode_default(result_data) {
                    Ok(toon) => {
                        debug!("✅ Auto-selected TOON for {} results ({} chars)", result_count, toon.len());
                        return Ok(CallToolResult::text_content(vec![TextContent::from(toon)]));
                    }
                    Err(e) => {
                        debug!("⚠️ TOON encoding failed: {}, falling back to JSON", e);
                        // Fall through to JSON
                    }
                }
            }

            // Small response or TOON failed: use JSON-only (no redundant text)
            let structured = serde_json::to_value(result_data)?;
            let structured_map = if let serde_json::Value::Object(map) = structured {
                map
            } else {
                return Err(anyhow!("Expected JSON object"));
            };
            debug!("✅ Auto-selected JSON for {} results (no redundant text_content)", result_count);
            Ok(CallToolResult::text_content(vec![])
                .with_structured_content(structured_map))
        }
        _ => {
            // Default (JSON/None): ONLY structured content (no redundant text)
            let structured = serde_json::to_value(result_data)?;
            let structured_map = if let serde_json::Value::Object(map) = structured {
                map
            } else {
                return Err(anyhow!("Expected JSON object"));
            };
            debug!("✅ Returning {} as JSON-only (no redundant text_content)", tool_name);
            Ok(CallToolResult::text_content(vec![])
                .with_structured_content(structured_map))
        }
    }
}
