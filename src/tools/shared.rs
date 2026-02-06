use serde::{Deserialize, Serialize};

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
