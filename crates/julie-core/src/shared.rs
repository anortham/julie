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

    /// Create a response where the total found differs from the result count
    /// (i.e., results were truncated to a limit). Used to produce "showing X of Y" output.
    pub fn with_total(results: Vec<T>, total_found: usize) -> Self {
        Self {
            results,
            total_found,
        }
    }
}

/// Blacklisted file extensions: binary and temporary files to exclude from indexing.
///
/// Used by the initial-indexing discovery path (`discover_indexable_files`).
/// Format: dot-prefixed, lowercase (e.g., `.ogg`).
pub const BLACKLISTED_EXTENSIONS: &[&str] = &[
    // Compiled / native binaries
    ".dll", ".exe", ".pdb", ".so", ".dylib", ".lib", ".a", ".o", ".obj", ".bin", ".class", ".pyc",
    ".pyo", ".wasm", // Audio
    ".mp3", ".wav", ".ogg", ".flac", ".aac", ".wma", ".m4a", ".opus", ".aiff", ".mid", ".midi",
    // Video
    ".mp4", ".avi", ".mov", ".wmv", ".flv", ".webm", ".mkv", ".m4v", ".mpg", ".mpeg", ".3gp",
    // Images
    ".jpg", ".jpeg", ".png", ".gif", ".bmp", ".ico", ".svg", ".webp", ".tiff", ".tif", ".heic",
    ".heif", ".psd", ".raw", ".cr2", ".nef", ".dng", // Archives
    ".zip", ".rar", ".7z", ".tar", ".gz", ".bz2", ".xz", ".zst",
    // Disk images / packages
    ".dmg", ".pkg", ".iso", ".deb", ".rpm", ".msi", // Database files
    ".db", ".sqlite", ".sqlite3", ".mdf", ".ldf", ".bak", // Fonts
    ".ttf", ".otf", ".woff", ".woff2", ".eot", // Documents (binary)
    ".pdf", ".doc", ".docx", ".xls", ".xlsx", ".ppt", ".pptx", // Temporary / ephemeral
    ".tmp", ".temp", ".cache", ".swp", ".swo", ".lock", ".pid", ".log", ".dump", ".core",
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
    // Julie's own auto-generated config — never contains source symbols and would
    // otherwise cause a scan/index asymmetry (scan_workspace_files filters by
    // extension and excludes it; the indexer accepts extensionless text files).
    ".julieignore",
    // Julie daemon state files. The primary defense for these files is
    // `RegistryPaths::is_under_julie_home`, which catches anything under the
    // configured `JULIE_HOME` regardless of name. The filename blacklist below
    // is belt-and-suspenders for the small set of filenames that are
    // specific enough to Julie that no real project would legitimately
    // ship one with the same name.
    //
    // Intentionally NOT blacklisted: `discovery.json`, `migration.json` —
    // those names are generic enough that some unrelated project might
    // ship one, and the `is_under_julie_home` check already covers them
    // when they appear under a configured `JULIE_HOME`.
    "daemon.token",
    "daemon-mcp.token",
    "daemon-mcp-transport.json",
    "daemon.state",
];

/// Blacklisted directory names - directories to exclude from indexing
pub const BLACKLISTED_DIRECTORIES: &[&str] = &[
    // Version control — keep in parity with crate::paths::VCS_ROOT_MARKERS
    ".git",
    ".svn",
    ".hg",
    ".bzr",
    ".jj",
    "_darcs",
    // IDE and editor directories
    ".vs",
    ".vscode",
    ".idea",
    ".eclipse",
    // Build and output directories
    // NOTE: "bin" is intentionally NOT blacklisted — it commonly holds user CLI
    // scripts (npm package bin entries, install/deploy scripts). .NET-style
    // bin/Release/ output is still excluded via the "Release" blacklist below.
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
    ".gradle",    // Gradle build cache (Java, Android)
    ".dart_tool", // Dart/Flutter build cache
    "Pods",
    "DerivedData",
];

/// Common method/function names that are too ambiguous to resolve reliably.
///
/// These names appear across virtually all 34 supported languages (e.g., `new`, `len`,
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
