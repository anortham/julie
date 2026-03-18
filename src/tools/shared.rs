use serde::{Deserialize, Serialize};

/// Lean response wrapper for search results.
///
/// Only carries what the lean formatters actually use: the result list
/// and the total count (for "showing X of Y" headers).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizedResponse<T> {
    /// The search results.
    pub results: Vec<T>,
    /// Total results found before limiting (may exceed results.len()).
    pub total_found: usize,
}

impl<T> OptimizedResponse<T> {
    pub fn new(results: Vec<T>) -> Self {
        let total_found = results.len();
        Self {
            results,
            total_found,
        }
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

/// Blacklisted filenames — files excluded by exact name, not extension.
/// Lockfiles use non-blacklisted extensions (.yaml, .json) but contain
/// generated dependency data that produces thousands of noise symbols.
pub const BLACKLISTED_FILENAMES: &[&str] = &[
    // Package manager lockfiles (extension-based blacklist misses these)
    "pnpm-lock.yaml",
    "package-lock.json",
    "composer.lock", // PHP (also caught by .lock ext, but explicit is clearer)
    "Pipfile.lock",  // Python
    "poetry.lock",   // Python
    "Gemfile.lock",  // Ruby
    "yarn.lock",     // JS/TS (also caught by .lock ext)
    "bun.lockb",     // Bun (binary, also caught by .lockb not being indexable)
    "shrinkwrap.json",
    // Other generated files with common extensions
    "npm-shrinkwrap.json",
    // Documentation build config files (produce spurious symbols from nav entries)
    "mkdocs.yml",
    "mkdocs.yaml",
    ".readthedocs.yml",
    ".readthedocs.yaml",
    "book.toml",
    "_config.yml",
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
    // NOTE: "packages" is NOT blacklisted — it's the standard monorepo layout
    // for npm/pnpm/Lerna/Nx/Turborepo projects (contains actual source code).
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
    // Windows system/app data directories (defense-in-depth for home dir indexing)
    "AppData",          // Windows: AppData\Local, AppData\Roaming, AppData\LocalLow
    "Application Data", // Windows: legacy name for AppData
    // Our own directories
    ".julie",
    ".coa",
    ".codenav",
    ".claude",
    ".memories", // Goldfish memory files (not code)
    // Other common exclusions
    ".sass-cache",
    ".nuxt",
    ".next",
    "Pods",
    "DerivedData",
];

/// Common method/function names that are too ambiguous to resolve reliably.
///
/// These names appear across virtually all 31 supported languages (e.g., `new`, `len`,
/// `get`, `from`). The relationship resolver often picks the wrong symbol because
/// dozens of definitions share the name. Filtering them from callee/neighbor lists
/// prevents misleading results and frees token budget for real symbols.
///
/// Used by: deep_dive (callee filtering), get_context (neighbor filtering).
pub const NOISE_CALLEE_NAMES: &[&str] = &[
    // Constructors / converters — every language has these
    "new",
    "default",
    "from",
    "into",
    "try_from",
    "try_into",
    // Accessors — too generic to resolve
    "as_ref",
    "as_mut",
    "borrow",
    "borrow_mut",
    "get",
    "set",
    // Unwrapping / error handling
    "unwrap",
    "expect",
    "ok",
    "err",
    // Trait / protocol boilerplate
    "clone",
    "to_string",
    "fmt",
    "eq",
    "ne",
    "cmp",
    "partial_cmp",
    "hash",
    "drop",
    "deref",
    "deref_mut",
    // Collection / iterator plumbing
    "is_empty",
    "len",
    "iter",
    "into_iter",
    "collect",
    "map",
    "filter",
    "push",
    "pop",
    "insert",
    "remove",
    "with_capacity",
    "and_then",
    "or_else",
    "push_str",
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
