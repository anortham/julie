use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use rust_mcp_sdk::{macros::mcp_tool, tool_box};
use rust_mcp_sdk::macros::JsonSchema;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use tracing::{info, debug, warn, error};
use std::path::{Path, PathBuf};
use std::fs;
use std::collections::HashSet;

use crate::handler::JulieServerHandler;
use crate::extractors::{Symbol, SymbolKind, Relationship};
use crate::workspace::JulieWorkspace;

/// Token-optimized response wrapper with confidence-based limiting
/// Inspired by codesearch's AIOptimizedResponse pattern
#[derive(Debug, Clone, Serialize)]
pub struct OptimizedResponse<T> {
    /// The main results (will be limited based on confidence)
    pub results: Vec<T>,
    /// Confidence score 0.0-1.0 (higher = more confident)
    pub confidence: f32,
    /// Total results found before limiting
    pub total_found: usize,
    /// Key insights or patterns discovered
    pub insights: Option<String>,
    /// Suggested next actions for the user
    pub next_actions: Vec<String>,
}

impl<T> OptimizedResponse<T> {
    pub fn new(results: Vec<T>, confidence: f32) -> Self {
        let total_found = results.len();
        Self {
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
            if self.confidence > 0.9 { 3 }        // High confidence = fewer results needed
            else if self.confidence > 0.7 { 5 }   // Medium confidence
            else if self.confidence > 0.5 { 8 }   // Lower confidence
            else { 12 }                          // Very low confidence = more results
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
const BLACKLISTED_EXTENSIONS: &[&str] = &[
    // Binary files
    ".dll", ".exe", ".pdb", ".so", ".dylib", ".lib", ".a", ".o", ".obj", ".bin",
    // Media files
    ".jpg", ".jpeg", ".png", ".gif", ".bmp", ".ico", ".svg", ".webp", ".tiff",
    ".mp3", ".mp4", ".avi", ".mov", ".wmv", ".flv", ".webm", ".mkv", ".wav",
    // Archives
    ".zip", ".rar", ".7z", ".tar", ".gz", ".bz2", ".xz", ".dmg", ".pkg",
    // Database files
    ".db", ".sqlite", ".sqlite3", ".mdf", ".ldf", ".bak",
    // Temporary files
    ".tmp", ".temp", ".cache", ".swp", ".swo", ".lock", ".pid",
    // Logs and other large files
    ".log", ".dump", ".core",
    // Font files
    ".ttf", ".otf", ".woff", ".woff2", ".eot",
    // Other binary formats
    ".pdf", ".doc", ".docx", ".xls", ".xlsx", ".ppt", ".pptx",
];

/// Blacklisted directory names - directories to exclude from indexing
const BLACKLISTED_DIRECTORIES: &[&str] = &[
    // Version control
    ".git", ".svn", ".hg", ".bzr",
    // IDE and editor directories
    ".vs", ".vscode", ".idea", ".eclipse",
    // Build and output directories
    "bin", "obj", "build", "dist", "out", "target", "Debug", "Release",
    // Package managers
    "node_modules", "packages", ".npm", "bower_components", "vendor",
    // Test and coverage
    "TestResults", "coverage", "__pycache__", ".pytest_cache", ".coverage",
    // Temporary and cache
    ".cache", ".temp", ".tmp", "tmp", "temp",
    // Our own directories
    ".julie", ".coa", ".codenav",
    // Other common exclusions
    ".sass-cache", ".nuxt", ".next", "Pods", "DerivedData",
];

/// File extensions that are likely to contain code and should be indexed
#[allow(dead_code)]
const KNOWN_CODE_EXTENSIONS: &[&str] = &[
    // Core languages (supported by extractors)
    ".rs", ".ts", ".tsx", ".js", ".jsx", ".py", ".java", ".cs", ".php",
    ".rb", ".swift", ".kt", ".go", ".cpp", ".cc", ".cxx", ".c", ".h",
    ".hpp", ".lua", ".sql", ".html", ".css", ".vue", ".razor", ".bash",
    ".sh", ".ps1", ".zig", ".dart",
    // Additional text-based formats worth indexing
    ".json", ".xml", ".yaml", ".yml", ".toml", ".ini", ".cfg", ".conf",
    ".md", ".txt", ".rst", ".asciidoc", ".tex", ".org",
    ".dockerfile", ".gitignore", ".gitattributes", ".editorconfig",
    ".eslintrc", ".prettierrc", ".babelrc", ".tsconfig", ".jsconfig",
    ".cargo", ".gradle", ".maven", ".sbt", ".mix", ".cabal", ".stack",
];

//******************//
// Index Workspace  //
//******************//
#[mcp_tool(
    name = "index_workspace",
    description = "Index the current workspace for fast code intelligence. Must be run first to enable semantic search.",
    title = "Index Workspace for Code Intelligence",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true,
    meta = r#"{"priority": "high", "category": "initialization"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct IndexWorkspaceTool {
    /// Optional workspace path (defaults to current directory)
    #[serde(default)]
    pub workspace_path: Option<String>,
    /// Force re-indexing even if index exists
    #[serde(default)]
    pub force_reindex: Option<bool>,
}

impl IndexWorkspaceTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        info!("📚 Starting workspace indexing...");

        let workspace_path = self.resolve_workspace_path()?;
        let force_reindex = self.force_reindex.unwrap_or(false);

        info!("🎯 Resolved workspace path: {}", workspace_path.display());

        // Initialize or load workspace
        let _workspace = self.initialize_workspace(&workspace_path, force_reindex)?;

        // Check if already indexed and not forcing reindex
        if !force_reindex {
            let is_indexed = *handler.is_indexed.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
            if is_indexed {
                let symbol_count = handler.symbols.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?.len();
                let message = format!(
                    "✅ Workspace already indexed!\n\
                    📊 Found {} symbols\n\
                    💡 Use force_reindex: true to re-index",
                    symbol_count
                );
                return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
            }
        }

        // Perform indexing
        match self.index_workspace_files(handler, &workspace_path).await {
            Ok((symbol_count, file_count, relationship_count)) => {
                // Mark as indexed
                *handler.is_indexed.write().map_err(|e| anyhow::anyhow!("Lock error: {}", e))? = true;

                let message = format!(
                    "🎉 Workspace indexing complete!\n\
                    📁 Indexed {} files\n\
                    🔍 Extracted {} symbols\n\
                    🔗 Found {} relationships\n\
                    ⚡ Ready for search and navigation!",
                    file_count, symbol_count, relationship_count
                );
                Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
            },
            Err(e) => {
                error!("Failed to index workspace: {}", e);
                let message = format!(
                    "❌ Workspace indexing failed: {}\n\
                    💡 Check that the path exists and contains source files",
                    e
                );
                Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
            }
        }
    }

    /// Resolve workspace path with proper root detection
    /// Supports both explicit paths and automatic workspace root detection
    fn resolve_workspace_path(&self) -> Result<PathBuf> {
        let target_path = match &self.workspace_path {
            Some(path) => {
                let expanded_path = shellexpand::tilde(path).to_string();
                PathBuf::from(expanded_path)
            },
            None => std::env::current_dir()?,
        };

        // Ensure path exists
        if !target_path.exists() {
            return Err(anyhow::anyhow!("Path does not exist: {}", target_path.display()));
        }

        // If it's a file, get its directory
        let workspace_candidate = if target_path.is_file() {
            target_path.parent()
                .ok_or_else(|| anyhow::anyhow!("Cannot determine parent directory"))?
                .to_path_buf()
        } else {
            target_path
        };

        // Find the actual workspace root (look for .git, .julie, or use the directory itself)
        self.find_workspace_root(&workspace_candidate)
    }

    /// Find workspace root by looking for common workspace markers
    fn find_workspace_root(&self, start_path: &Path) -> Result<PathBuf> {
        let workspace_markers = [".git", ".julie", ".vscode", "Cargo.toml", "package.json", ".project"];

        let mut current_path = start_path.to_path_buf();

        // Walk up the directory tree looking for workspace markers
        loop {
            for marker in &workspace_markers {
                let marker_path = current_path.join(marker);
                if marker_path.exists() {
                    info!("🎯 Found workspace marker '{}' at: {}", marker, current_path.display());
                    return Ok(current_path);
                }
            }

            match current_path.parent() {
                Some(parent) => current_path = parent.to_path_buf(),
                None => break,
            }
        }

        // No markers found, use the original path as workspace root
        info!("🎯 No workspace markers found, using directory as root: {}", start_path.display());
        Ok(start_path.to_path_buf())
    }

    /// Initialize or load Julie workspace
    fn initialize_workspace(&self, workspace_path: &Path, force_reindex: bool) -> Result<JulieWorkspace> {
        // Try to detect existing workspace first
        if let Ok(Some(existing_workspace)) = JulieWorkspace::detect_and_load(workspace_path.to_path_buf()) {
            if !force_reindex {
                info!("📂 Using existing Julie workspace at: {}", existing_workspace.julie_dir.display());
                return Ok(existing_workspace);
            } else {
                info!("🔄 Force reindex requested, reinitializing workspace");
            }
        }

        // Initialize new workspace
        info!("🆕 Initializing new Julie workspace");
        JulieWorkspace::initialize(workspace_path.to_path_buf())
    }

    async fn index_workspace_files(&self, handler: &JulieServerHandler, workspace_path: &Path) -> Result<(usize, usize, usize)> {
        info!("🔍 Scanning workspace: {}", workspace_path.display());

        // Clear existing data if force reindex
        if self.force_reindex.unwrap_or(false) {
            handler.symbols.write().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?.clear();
            handler.relationships.write().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?.clear();
        }

        let mut total_files = 0;

        // Use blacklist-based file discovery (index everything except blacklisted items)
        let files_to_index = self.discover_indexable_files(workspace_path)?;

        info!("📊 Found {} files to index after filtering", files_to_index.len());

        for file_path in files_to_index {
            match self.process_file(handler, &file_path).await {
                Ok(_) => {
                    total_files += 1;
                    if total_files % 50 == 0 {
                        debug!("📈 Processed {} files so far...", total_files);
                    }
                }
                Err(e) => {
                    warn!("Failed to process file {:?}: {}", file_path, e);
                }
            }
        }

        // Get final counts
        let total_symbols = handler.symbols.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?.len();
        let total_relationships = handler.relationships.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?.len();

        info!("✅ Indexing complete: {} files, {} symbols, {} relationships",
              total_files, total_symbols, total_relationships);

        Ok((total_symbols, total_files, total_relationships))
    }

    /// Discover all indexable files using blacklist approach
    fn discover_indexable_files(&self, workspace_path: &Path) -> Result<Vec<PathBuf>> {
        let mut indexable_files = Vec::new();
        let blacklisted_dirs: HashSet<&str> = BLACKLISTED_DIRECTORIES.iter().copied().collect();
        let blacklisted_exts: HashSet<&str> = BLACKLISTED_EXTENSIONS.iter().copied().collect();
        let max_file_size = 1024 * 1024; // 1MB limit for files

        debug!("🔍 Starting recursive file discovery from: {}", workspace_path.display());

        self.walk_directory_recursive(
            workspace_path,
            &blacklisted_dirs,
            &blacklisted_exts,
            max_file_size,
            &mut indexable_files,
        )?;

        debug!("📊 File discovery summary:");
        debug!("  - Total indexable files: {}", indexable_files.len());

        Ok(indexable_files)
    }

    /// Recursively walk directory tree, excluding blacklisted paths
    fn walk_directory_recursive(
        &self,
        dir_path: &Path,
        blacklisted_dirs: &HashSet<&str>,
        blacklisted_exts: &HashSet<&str>,
        max_file_size: u64,
        indexable_files: &mut Vec<PathBuf>,
    ) -> Result<()> {
        let entries = fs::read_dir(dir_path)
            .map_err(|e| anyhow::anyhow!("Failed to read directory {:?}: {}", dir_path, e))?;

        for entry in entries {
            let entry = entry.map_err(|e| anyhow::anyhow!("Failed to read directory entry: {}", e))?;
            let path = entry.path();
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            // Skip hidden files/directories that start with . (except known code files)
            if file_name.starts_with('.') && !self.is_known_dotfile(&path) {
                continue;
            }

            if path.is_dir() {
                // Check if directory should be blacklisted
                if blacklisted_dirs.contains(file_name) {
                    debug!("⏭️  Skipping blacklisted directory: {}", path.display());
                    continue;
                }

                // Recursively process subdirectory
                self.walk_directory_recursive(&path, blacklisted_dirs, blacklisted_exts, max_file_size, indexable_files)?;
            } else if path.is_file() {
                // Check file extension and size
                if self.should_index_file(&path, blacklisted_exts, max_file_size)? {
                    indexable_files.push(path);
                }
            }
        }

        Ok(())
    }

    /// Check if a file should be indexed based on blacklist and size limits
    fn should_index_file(&self, file_path: &Path, blacklisted_exts: &HashSet<&str>, max_file_size: u64) -> Result<bool> {
        // Get file extension
        let extension = file_path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| format!(".{}", ext.to_lowercase()))
            .unwrap_or_default();

        // Skip blacklisted extensions
        if blacklisted_exts.contains(extension.as_str()) {
            return Ok(false);
        }

        // Check file size
        let metadata = fs::metadata(file_path)
            .map_err(|e| anyhow::anyhow!("Failed to get metadata for {:?}: {}", file_path, e))?;

        if metadata.len() > max_file_size {
            debug!("⏭️  Skipping large file ({} bytes): {}", metadata.len(), file_path.display());
            return Ok(false);
        }

        // If no extension, check if it's likely a text file by reading first few bytes
        if extension.is_empty() {
            return Ok(self.is_likely_text_file(file_path)?);
        }

        // Index any non-blacklisted file
        Ok(true)
    }

    /// Check if a dotfile is a known configuration file that should be indexed
    fn is_known_dotfile(&self, path: &Path) -> bool {
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        matches!(file_name,
            ".gitignore" | ".gitattributes" | ".editorconfig" | ".eslintrc" |
            ".prettierrc" | ".babelrc" | ".tsconfig" | ".jsconfig" |
            ".cargo" | ".env" | ".npmrc"
        )
    }

    /// Heuristic to determine if a file without extension is likely a text file
    fn is_likely_text_file(&self, file_path: &Path) -> Result<bool> {
        // Read first 512 bytes to check for binary content
        let mut file = fs::File::open(file_path)
            .map_err(|e| anyhow::anyhow!("Failed to open file {:?}: {}", file_path, e))?;

        let mut buffer = [0; 512];
        let bytes_read = std::io::Read::read(&mut file, &mut buffer)
            .map_err(|e| anyhow::anyhow!("Failed to read file {:?}: {}", file_path, e))?;

        if bytes_read == 0 {
            return Ok(false); // Empty file
        }

        // Check for null bytes (common in binary files)
        let has_null_bytes = buffer[..bytes_read].contains(&0);
        if has_null_bytes {
            return Ok(false);
        }

        // Check if most bytes are printable ASCII/UTF-8
        let printable_count = buffer[..bytes_read]
            .iter()
            .filter(|&&b| b >= 32 && b <= 126 || b == 9 || b == 10 || b == 13 || b >= 128)
            .count();

        let text_ratio = printable_count as f64 / bytes_read as f64;
        Ok(text_ratio > 0.8) // At least 80% printable characters
    }

    async fn process_file(&self, handler: &JulieServerHandler, file_path: &Path) -> Result<()> {
        debug!("Processing file: {:?}", file_path);

        // Read file content
        let content = fs::read_to_string(file_path)
            .map_err(|e| anyhow::anyhow!("Failed to read file {:?}: {}", file_path, e))?;

        // Skip empty files
        if content.trim().is_empty() {
            return Ok(());
        }

        // Determine language and extract symbols
        let language = self.detect_language(file_path);
        let file_path_str = file_path.to_string_lossy().to_string();

        self.extract_symbols_for_language(handler, &file_path_str, &content, &language).await
    }

    /// Extract symbols using the appropriate extractor for the detected language
    async fn extract_symbols_for_language(
        &self,
        handler: &JulieServerHandler,
        file_path: &str,
        content: &str,
        language: &str
    ) -> Result<()> {
        // Only process languages that we have both tree-sitter support and extractors for
        match language {
            "rust" | "typescript" | "javascript" | "python" => {
                self.extract_symbols_with_parser(handler, file_path, content, language).await
            },
            _ => {
                // For unsupported languages, just skip extraction but log it
                debug!("No extractor available for language: {} (file: {})", language, file_path);
                Ok(())
            }
        }
    }

    /// Extract symbols using the appropriate extractor - specific implementation per language
    async fn extract_symbols_with_parser(
        &self,
        handler: &JulieServerHandler,
        file_path: &str,
        content: &str,
        language: &str
    ) -> Result<()> {
        // Create parser for the language
        let mut parser = tree_sitter::Parser::new();
        let tree_sitter_language = self.get_tree_sitter_language(language)?;

        parser.set_language(&tree_sitter_language)
            .map_err(|e| anyhow::anyhow!("Failed to set parser language for {}: {}", language, e))?;

        // Parse the file
        let tree = parser.parse(content, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse file: {}", file_path))?;

        // Extract symbols and relationships using language-specific extractor
        let (symbols, relationships) = match language {
            "rust" => {
                let mut extractor = crate::extractors::rust::RustExtractor::new(
                    language.to_string(), file_path.to_string(), content.to_string());
                let symbols = extractor.extract_symbols(&tree);
                let relationships = extractor.extract_relationships(&tree, &symbols);
                (symbols, relationships)
            },
            "typescript" => {
                let mut extractor = crate::extractors::typescript::TypeScriptExtractor::new(
                    language.to_string(), file_path.to_string(), content.to_string());
                let symbols = extractor.extract_symbols(&tree);
                let relationships = extractor.extract_relationships(&tree, &symbols);
                (symbols, relationships)
            },
            "javascript" => {
                let mut extractor = crate::extractors::javascript::JavaScriptExtractor::new(
                    language.to_string(), file_path.to_string(), content.to_string());
                let symbols = extractor.extract_symbols(&tree);
                let relationships = extractor.extract_relationships(&tree, &symbols);
                (symbols, relationships)
            },
            "python" => {
                let mut extractor = crate::extractors::python::PythonExtractor::new(
                    file_path.to_string(), content.to_string());
                let symbols = extractor.extract_symbols(&tree);
                let relationships = extractor.extract_relationships(&tree, &symbols);
                (symbols, relationships)
            },
            _ => {
                // For unsupported languages, just return empty collections
                debug!("Language '{}' supported for parsing but no extractor available", language);
                (Vec::new(), Vec::new())
            }
        };

        debug!("📊 Extracted {} symbols and {} relationships from {}",
               symbols.len(), relationships.len(), file_path);

        // Store results in handler
        {
            let mut symbol_storage = handler.symbols.write()
                .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
            symbol_storage.extend(symbols);
        }

        {
            let mut relationship_storage = handler.relationships.write()
                .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
            relationship_storage.extend(relationships);
        }

        Ok(())
    }

    /// Get the appropriate tree-sitter language for a detected language
    fn get_tree_sitter_language(&self, language: &str) -> Result<tree_sitter::Language> {
        match language {
            "rust" => Ok(tree_sitter_rust::LANGUAGE.into()),
            "typescript" => Ok(tree_sitter_typescript::LANGUAGE_TSX.into()),
            "javascript" => Ok(tree_sitter_javascript::LANGUAGE.into()),
            "python" => Ok(tree_sitter_python::LANGUAGE.into()),
            _ => Err(anyhow::anyhow!("No tree-sitter language available for: {}", language))
        }
    }

    /// Detect programming language from file extension
    fn detect_language(&self, file_path: &Path) -> String {
        let extension = file_path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");

        let file_name = file_path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");

        // Match by extension first
        match extension.to_lowercase().as_str() {
            // Rust
            "rs" => "rust".to_string(),

            // TypeScript/JavaScript
            "ts" | "mts" | "cts" => "typescript".to_string(),
            "tsx" => "typescript".to_string(),
            "js" | "mjs" | "cjs" => "javascript".to_string(),
            "jsx" => "javascript".to_string(),

            // Python
            "py" | "pyi" | "pyw" => "python".to_string(),

            // Java
            "java" => "java".to_string(),

            // C#
            "cs" => "csharp".to_string(),

            // PHP
            "php" | "phtml" | "php3" | "php4" | "php5" => "php".to_string(),

            // Ruby
            "rb" | "rbw" => "ruby".to_string(),

            // Swift
            "swift" => "swift".to_string(),

            // Kotlin
            "kt" | "kts" => "kotlin".to_string(),

            // Go
            "go" => "go".to_string(),

            // C
            "c" => "c".to_string(),

            // C++
            "cpp" | "cc" | "cxx" | "c++" | "hpp" | "hh" | "hxx" | "h++" => "cpp".to_string(),
            "h" => {
                // Could be C or C++ header, default to C
                if file_path.to_string_lossy().contains("cpp") ||
                   file_path.to_string_lossy().contains("c++") {
                    "cpp".to_string()
                } else {
                    "c".to_string()
                }
            },

            // Lua
            "lua" => "lua".to_string(),

            // SQL
            "sql" | "mysql" | "pgsql" | "sqlite" => "sql".to_string(),

            // HTML
            "html" | "htm" => "html".to_string(),

            // CSS
            "css" => "css".to_string(),

            // Vue
            "vue" => "vue".to_string(),

            // Razor
            "cshtml" | "razor" => "razor".to_string(),

            // Shell scripts
            "sh" | "bash" | "zsh" | "fish" => "bash".to_string(),

            // PowerShell
            "ps1" | "psm1" | "psd1" => "powershell".to_string(),

            // GDScript
            "gd" => "gdscript".to_string(),

            // Zig
            "zig" => "zig".to_string(),

            // Dart
            "dart" => "dart".to_string(),

            // Regex patterns (special handling)
            "regex" | "regexp" => "regex".to_string(),

            // Default case - check filename
            _ => {
                // Handle files without extensions or special cases
                match file_name.to_lowercase().as_str() {
                    // Build files
                    "dockerfile" | "containerfile" => "dockerfile".to_string(),
                    "makefile" | "gnumakefile" => "makefile".to_string(),
                    "cargo.toml" | "cargo.lock" => "toml".to_string(),
                    "package.json" | "tsconfig.json" | "jsconfig.json" => "json".to_string(),

                    // Shell scripts
                    name if name.starts_with("bash") || name.contains("bashrc") || name.contains("bash_") => "bash".to_string(),

                    // Default to unknown
                    _ => "text".to_string(),
                }
            }
        }
    }
}

//******************//
//   Search Code    //
//******************//
#[mcp_tool(
    name = "fast_search",
    description = "SEARCH BEFORE CODING - Find existing implementations to avoid duplication with lightning speed",
    title = "Fast Unified Search (Text + Semantic)",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true,
    meta = r#"{"category": "search", "performance": "sub_10ms"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FastSearchTool {
    /// Search query (symbol name, function name, etc.)
    pub query: String,
    /// Search mode: text (classic code search), semantic (AI understanding), hybrid (both)
    #[serde(default = "default_text")]
    pub mode: String,
    /// Optional language filter
    #[serde(default)]
    pub language: Option<String>,
    /// Optional file path pattern filter
    #[serde(default)]
    pub file_pattern: Option<String>,
    /// Maximum number of results
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 { 50 }
fn default_text() -> String { "text".to_string() }

impl FastSearchTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("🔍 Fast search: {} (mode: {})", self.query, self.mode);

        // Check if workspace is indexed
        let is_indexed = *handler.is_indexed.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        if !is_indexed {
            let message = "❌ Workspace not indexed yet!\n💡 Run index_workspace first to enable fast search.";
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // Perform search based on mode
        let symbols = match self.mode.as_str() {
            "semantic" => self.semantic_search(handler).await?,
            "hybrid" => self.hybrid_search(handler).await?,
            "text" | _ => self.text_search(handler)?,
        };

        // Create optimized response with confidence scoring
        let confidence = self.calculate_search_confidence(&symbols);
        let mut optimized = OptimizedResponse::new(symbols, confidence);

        // Add insights based on patterns found
        if let Some(insights) = self.generate_search_insights(&optimized.results) {
            optimized = optimized.with_insights(insights);
        }

        // Add smart next actions
        let next_actions = self.suggest_next_actions(&optimized.results);
        optimized = optimized.with_next_actions(next_actions);

        // Optimize for tokens
        optimized.optimize_for_tokens(Some(self.limit as usize));

        if optimized.results.is_empty() {
            let message = format!(
                "🔍 No results found for: '{}'\n\
                💡 Try a broader search term, different mode, or check spelling",
                self.query
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // Format optimized results
        let message = self.format_optimized_results(&optimized);
        Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
    }

    fn text_search(&self, handler: &JulieServerHandler) -> Result<Vec<Symbol>> {
        let symbols = handler.symbols.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        let query_lower = self.query.to_lowercase();

        let mut results: Vec<Symbol> = symbols.iter()
            .filter(|symbol| {
                // Name matching (case insensitive)
                let name_match = symbol.name.to_lowercase().contains(&query_lower);

                // Language filter
                let language_match = self.language.as_ref()
                    .map(|lang| symbol.language.eq_ignore_ascii_case(lang))
                    .unwrap_or(true);

                // File pattern filter (basic implementation)
                let file_match = self.file_pattern.as_ref()
                    .map(|pattern| symbol.file_path.contains(pattern))
                    .unwrap_or(true);

                name_match && language_match && file_match
            })
            .cloned()
            .collect();

        // Sort by relevance (exact matches first, then by symbol kind)
        results.sort_by(|a, b| {
            let a_exact = a.name.eq_ignore_ascii_case(&self.query);
            let b_exact = b.name.eq_ignore_ascii_case(&self.query);

            match (a_exact, b_exact) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => {
                    // Sort by symbol kind priority
                    let a_priority = self.symbol_priority(&a.kind);
                    let b_priority = self.symbol_priority(&b.kind);
                    a_priority.cmp(&b_priority)
                }
            }
        });

        Ok(results)
    }

    fn symbol_priority(&self, kind: &SymbolKind) -> u8 {
        match kind {
            SymbolKind::Function => 1,
            SymbolKind::Class => 2,
            SymbolKind::Interface => 3,
            SymbolKind::Method => 4,
            SymbolKind::Variable => 5,
            SymbolKind::Type => 6,
            _ => 10,
        }
    }

    async fn semantic_search(&self, handler: &JulieServerHandler) -> Result<Vec<Symbol>> {
        // For now, delegate to text search - full semantic implementation coming soon
        debug!("🧠 Semantic search mode (using text fallback)");
        self.text_search(handler)
    }

    async fn hybrid_search(&self, handler: &JulieServerHandler) -> Result<Vec<Symbol>> {
        // For now, delegate to text search - full hybrid implementation coming soon
        debug!("🔄 Hybrid search mode (using text fallback)");
        self.text_search(handler)
    }

    /// Calculate confidence score based on search quality and result relevance
    fn calculate_search_confidence(&self, symbols: &[Symbol]) -> f32 {
        if symbols.is_empty() { return 0.0; }

        let mut confidence: f32 = 0.5; // Base confidence

        // Exact name matches boost confidence
        let exact_matches = symbols.iter()
            .filter(|s| s.name.to_lowercase() == self.query.to_lowercase())
            .count();
        if exact_matches > 0 {
            confidence += 0.3;
        }

        // Partial matches are medium confidence
        let partial_matches = symbols.iter()
            .filter(|s| s.name.to_lowercase().contains(&self.query.to_lowercase()))
            .count();
        if partial_matches > exact_matches {
            confidence += 0.2;
        }

        // More results can indicate ambiguity (lower confidence)
        if symbols.len() > 20 {
            confidence -= 0.1;
        } else if symbols.len() < 5 {
            confidence += 0.1;
        }

        confidence.clamp(0.0, 1.0)
    }

    /// Generate intelligent insights about search patterns
    fn generate_search_insights(&self, symbols: &[Symbol]) -> Option<String> {
        if symbols.is_empty() { return None; }

        let mut insights = Vec::new();

        // Language distribution
        let mut lang_counts = std::collections::HashMap::new();
        for symbol in symbols {
            *lang_counts.entry(&symbol.language).or_insert(0) += 1;
        }

        if lang_counts.len() > 1 {
            let main_lang = lang_counts.iter().max_by_key(|(_, count)| *count).unwrap();
            insights.push(format!("Found across {} languages (mainly {})",
                lang_counts.len(), main_lang.0));
        }

        // Kind distribution
        let mut kind_counts = std::collections::HashMap::new();
        for symbol in symbols {
            *kind_counts.entry(&symbol.kind).or_insert(0) += 1;
        }

        if let Some((dominant_kind, count)) = kind_counts.iter().max_by_key(|(_, count)| *count) {
            if *count > symbols.len() / 2 {
                insights.push(format!("Mostly {:?}s ({} of {})",
                    dominant_kind, count, symbols.len()));
            }
        }

        if insights.is_empty() { None } else { Some(insights.join(", ")) }
    }

    /// Suggest intelligent next actions based on search results
    fn suggest_next_actions(&self, symbols: &[Symbol]) -> Vec<String> {
        let mut actions = Vec::new();

        if symbols.len() == 1 {
            actions.push("Use fast_goto to jump to definition".to_string());
            actions.push("Use fast_refs to see all usages".to_string());
        } else if symbols.len() > 1 {
            actions.push("Narrow search with language filter".to_string());
            actions.push("Use fast_refs on specific symbols".to_string());
        }

        // Check if we have functions that might be entry points
        if symbols.iter().any(|s| matches!(s.kind, SymbolKind::Function) && s.name.contains("main")) {
            actions.push("Use fast_explore to understand architecture".to_string());
        }

        if symbols.iter().any(|s| s.name.to_lowercase().contains(&self.query.to_lowercase())) {
            actions.push("Consider exact name match for precision".to_string());
        }

        actions
    }

    /// Format optimized response with insights and next actions
    fn format_optimized_results(&self, optimized: &OptimizedResponse<Symbol>) -> String {
        let mut lines = vec![
            format!("⚡ Fast Search: '{}' (mode: {})", self.query, self.mode),
            format!("📊 Showing {} of {} results (confidence: {:.1})",
                    optimized.results.len(), optimized.total_found, optimized.confidence),
        ];

        // Add insights if available
        if let Some(insights) = &optimized.insights {
            lines.push(format!("💡 {}", insights));
        }

        lines.push(String::new());

        // Format results
        for (i, symbol) in optimized.results.iter().enumerate() {
            lines.push(format!(
                "{}. {} [{}]",
                i + 1, symbol.name, symbol.language
            ));
            lines.push(format!(
                "   📁 {}:{}-{}",
                symbol.file_path, symbol.start_line, symbol.end_line
            ));

            if let Some(signature) = &symbol.signature {
                lines.push(format!("   📝 {}", signature));
            }
            lines.push(String::new());
        }

        // Add next actions
        if !optimized.next_actions.is_empty() {
            lines.push("🎯 Suggested next actions:".to_string());
            for action in &optimized.next_actions {
                lines.push(format!("   • {}", action));
            }
        }

        lines.join("\n")
    }
}

//******************//
// Goto Definition  //
//******************//
#[mcp_tool(
    name = "fast_goto",
    description = "JUMP TO SOURCE - Navigate directly to where symbols are defined with lightning speed",
    title = "Fast Navigate to Definition",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true,
    meta = r#"{"category": "navigation", "precision": "line_level"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FastGotoTool {
    /// Symbol name to find definition for
    pub symbol: String,
    /// Optional context file path for better resolution
    #[serde(default)]
    pub context_file: Option<String>,
    /// Optional line number for context
    #[serde(default)]
    pub line_number: Option<u32>,
}

impl FastGotoTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("🎯 Finding definition for: {}", self.symbol);

        // Check if workspace is indexed
        let is_indexed = *handler.is_indexed.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        if !is_indexed {
            let message = "❌ Workspace not indexed yet!\n💡 Run index_workspace first to enable navigation.";
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // Find symbol definitions
        let definitions = self.find_definitions(handler)?;

        if definitions.is_empty() {
            let message = format!(
                "🔍 No definition found for: '{}'\n\
                💡 Check the symbol name and ensure it exists in the indexed files",
                self.symbol
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // Format results
        let mut message = format!(
            "🎯 Found {} definition(s) for: '{}'\n\n",
            definitions.len(),
            self.symbol
        );

        for (i, symbol) in definitions.iter().enumerate() {
            message.push_str(&format!(
                "{}. {} [{}]\n\
                   📁 {}:{}:{}\n\
                   🏷️ Kind: {:?}\n",
                i + 1,
                symbol.name,
                symbol.language,
                symbol.file_path,
                symbol.start_line,
                symbol.start_column,
                symbol.kind
            ));

            if let Some(signature) = &symbol.signature {
                message.push_str(&format!("   📝 {}", signature));
            }
            message.push('\n');
        }

        Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
    }

    fn find_definitions(&self, handler: &JulieServerHandler) -> Result<Vec<Symbol>> {
        let symbols = handler.symbols.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        let relationships = handler.relationships.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        debug!("🔍 Searching for '{}' across {} symbols", self.symbol, symbols.len());

        // Strategy 1: Exact name matches with cross-language resolution
        let mut definitions: Vec<Symbol> = Vec::new();

        // Find all symbols with matching names
        let mut exact_matches: Vec<Symbol> = symbols.iter()
            .filter(|symbol| symbol.name == self.symbol)
            .cloned()
            .collect();

        // Strategy 2: Use relationships to find actual definitions
        // Look for symbols that are referenced/imported with this name
        for relationship in relationships.iter() {
            if let Some(target_symbol) = symbols.iter().find(|s| s.id == relationship.to_symbol_id) {
                // Check if this relationship represents a definition or import
                match &relationship.kind {
                    crate::extractors::base::RelationshipKind::Imports => {
                        if target_symbol.name == self.symbol {
                            exact_matches.push(target_symbol.clone());
                        }
                    }
                    crate::extractors::base::RelationshipKind::Defines => {
                        if target_symbol.name == self.symbol {
                            exact_matches.push(target_symbol.clone());
                        }
                    }
                    crate::extractors::base::RelationshipKind::Extends => {
                        if target_symbol.name == self.symbol {
                            exact_matches.push(target_symbol.clone());
                        }
                    }
                    _ => {}
                }
            }
        }

        // Remove duplicates based on symbol id
        exact_matches.sort_by(|a, b| a.id.cmp(&b.id));
        exact_matches.dedup_by(|a, b| a.id == b.id);

        // Strategy 3: Cross-language resolution - look for symbols with similar signatures
        if exact_matches.is_empty() {
            debug!("🌍 Attempting cross-language resolution for '{}'", self.symbol);

            // Look for similar names across languages (handle different naming conventions)
            let symbol_lower = self.symbol.to_lowercase();
            let snake_case_version = self.to_snake_case(&self.symbol);
            let camel_case_version = self.to_camel_case(&self.symbol);
            let pascal_case_version = self.to_pascal_case(&self.symbol);

            for symbol in symbols.iter() {
                let name_lower = symbol.name.to_lowercase();
                if name_lower == symbol_lower ||
                   symbol.name == snake_case_version ||
                   symbol.name == camel_case_version ||
                   symbol.name == pascal_case_version {
                    exact_matches.push(symbol.clone());
                }
            }
        }

        // Strategy 4: Semantic matching if still no results
        if exact_matches.is_empty() {
            debug!("🧠 Using semantic matching for '{}'", self.symbol);

            // Initialize embedding engine for semantic search
            if let Ok(cache_dir) = std::env::temp_dir().canonicalize() {
                let model_cache = cache_dir.join("julie_models");
                if let Ok(mut embedding_engine) = crate::embeddings::EmbeddingEngine::new("bge-small", model_cache) {
                    if let Ok(query_embedding) = embedding_engine.embed_text(&self.symbol) {
                        for symbol in symbols.iter() {
                            let symbol_text = format!("{} {:?}", symbol.name, symbol.kind);
                            if let Ok(symbol_embedding) = embedding_engine.embed_text(&symbol_text) {
                                let similarity = crate::embeddings::cosine_similarity(&query_embedding, &symbol_embedding);
                                if similarity > 0.7 { // High similarity threshold for definitions
                                    exact_matches.push(symbol.clone());
                                }
                            }
                        }
                    }
                }
            }
        }

        // Prioritize results
        exact_matches.sort_by(|a, b| {
            // First by definition priority (classes > functions > variables)
            let priority_cmp = self.definition_priority(&a.kind).cmp(&self.definition_priority(&b.kind));
            if priority_cmp != std::cmp::Ordering::Equal {
                return priority_cmp;
            }

            // Then by context file preference if provided
            if let Some(context_file) = &self.context_file {
                let a_in_context = a.file_path.contains(context_file);
                let b_in_context = b.file_path.contains(context_file);
                match (a_in_context, b_in_context) {
                    (true, false) => return std::cmp::Ordering::Less,
                    (false, true) => return std::cmp::Ordering::Greater,
                    _ => {}
                }
            }

            // Finally by line number if provided (prefer definitions closer to context)
            if let Some(line_number) = self.line_number {
                let a_distance = (a.start_line as i32 - line_number as i32).abs();
                let b_distance = (b.start_line as i32 - line_number as i32).abs();
                return a_distance.cmp(&b_distance);
            }

            std::cmp::Ordering::Equal
        });

        debug!("✅ Found {} definitions for '{}'", exact_matches.len(), self.symbol);
        Ok(exact_matches)
    }

    // Helper functions for cross-language naming convention conversion
    fn to_snake_case(&self, s: &str) -> String {
        let mut result = String::new();
        let mut chars = s.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch.is_uppercase() {
                if !result.is_empty() && chars.peek().map_or(false, |c| c.is_lowercase()) {
                    result.push('_');
                }
                result.push(ch.to_lowercase().next().unwrap());
            } else {
                result.push(ch);
            }
        }
        result
    }

    fn to_camel_case(&self, s: &str) -> String {
        let mut result = String::new();
        let mut capitalize_next = false;

        for ch in s.chars() {
            if ch == '_' {
                capitalize_next = true;
            } else if capitalize_next {
                result.push(ch.to_uppercase().next().unwrap());
                capitalize_next = false;
            } else {
                result.push(ch);
            }
        }
        result
    }

    fn to_pascal_case(&self, s: &str) -> String {
        let camel = self.to_camel_case(s);
        if camel.is_empty() {
            return camel;
        }

        let mut chars = camel.chars();
        match chars.next() {
            None => String::new(),
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        }
    }

    fn definition_priority(&self, kind: &SymbolKind) -> u8 {
        match kind {
            SymbolKind::Class | SymbolKind::Interface => 1,
            SymbolKind::Function => 2,
            SymbolKind::Method | SymbolKind::Constructor => 3,
            SymbolKind::Type | SymbolKind::Enum => 4,
            SymbolKind::Variable | SymbolKind::Constant => 5,
            _ => 10,
        }
    }
}

//******************//
// Find References  //
//******************//
#[mcp_tool(
    name = "fast_refs",
    description = "FIND ALL IMPACT - See all references before you change code (prevents surprises)",
    title = "Fast Find All References",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true,
    meta = r#"{"category": "navigation", "scope": "workspace"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FastRefsTool {
    /// Symbol name to find references for
    pub symbol: String,
    /// Include definition in results
    #[serde(default = "default_true")]
    pub include_definition: bool,
    /// Maximum number of results
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_true() -> bool { true }

impl FastRefsTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("🔗 Finding references for: {}", self.symbol);

        // Check if workspace is indexed
        let is_indexed = *handler.is_indexed.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        if !is_indexed {
            let message = "❌ Workspace not indexed yet!\n💡 Run index_workspace first to enable navigation.";
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // Find references
        let (definitions, references) = self.find_references_and_definitions(handler)?;

        if definitions.is_empty() && references.is_empty() {
            let message = format!(
                "🔍 No references found for: '{}'\n\
                💡 Check the symbol name and ensure it exists in the indexed files",
                self.symbol
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // Format results
        let total_results = if self.include_definition { definitions.len() + references.len() } else { references.len() };
        let mut message = format!(
            "🔗 Found {} reference(s) for: '{}'\n\n",
            total_results,
            self.symbol
        );

        let mut count = 0;

        // Include definitions if requested
        if self.include_definition && !definitions.is_empty() {
            message.push_str("🎯 Definitions:\n");
            for symbol in &definitions {
                if count >= self.limit as usize { break; }
                message.push_str(&format!(
                    "  {} [{}] - {}:{}:{}\n",
                    symbol.name,
                    format!("{:?}", symbol.kind).to_lowercase(),
                    symbol.file_path,
                    symbol.start_line,
                    symbol.start_column
                ));
                count += 1;
            }
            message.push('\n');
        }

        // Include references
        if !references.is_empty() {
            message.push_str("🔗 References:\n");
            for relationship in references.iter().take((self.limit as usize).saturating_sub(count)) {
                message.push_str(&format!(
                    "  {} - {}:{} ({})",
                    format!("{:?}", relationship.kind),
                    relationship.file_path,
                    relationship.line_number,
                    relationship.kind
                ));

                // Add confidence if not 1.0
                if relationship.confidence < 1.0 {
                    message.push_str(&format!(" [confidence: {:.1}]", relationship.confidence));
                }
                message.push('\n');
                count += 1;
            }
        }

        if total_results > self.limit as usize {
            message.push_str(&format!("\n... and {} more references\n", total_results - self.limit as usize));
        }

        Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
    }

    fn find_references_and_definitions(&self, handler: &JulieServerHandler) -> Result<(Vec<Symbol>, Vec<Relationship>)> {
        let symbols = handler.symbols.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        let relationships = handler.relationships.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        debug!("🔍 Searching for references to '{}' across {} symbols", self.symbol, symbols.len());

        // Strategy 1: Find symbol definitions (exact matches and cross-language variants)
        let mut definitions = Vec::new();

        // Find exact name matches
        for symbol in symbols.iter() {
            if symbol.name == self.symbol {
                definitions.push(symbol.clone());
            }
        }

        // Cross-language naming convention matching
        let snake_case_version = self.to_snake_case(&self.symbol);
        let camel_case_version = self.to_camel_case(&self.symbol);
        let pascal_case_version = self.to_pascal_case(&self.symbol);

        for symbol in symbols.iter() {
            if symbol.name == snake_case_version ||
               symbol.name == camel_case_version ||
               symbol.name == pascal_case_version {
                definitions.push(symbol.clone());
            }
        }

        // Remove duplicates
        definitions.sort_by(|a, b| a.id.cmp(&b.id));
        definitions.dedup_by(|a, b| a.id == b.id);

        // Strategy 2: Find direct relationships
        let symbol_ids: Vec<String> = definitions.iter().map(|s| s.id.clone()).collect();
        let mut references: Vec<Relationship> = relationships.iter()
            .filter(|rel| {
                symbol_ids.iter().any(|id| rel.to_symbol_id == *id || rel.from_symbol_id == *id)
            })
            .cloned()
            .collect();

        // Strategy 3: Semantic similarity matching for cross-language references
        debug!("🧠 Performing semantic similarity analysis for references");

        // Initialize embedding engine for semantic analysis
        if let Ok(cache_dir) = std::env::temp_dir().canonicalize() {
            let model_cache = cache_dir.join("julie_models");
            if let Ok(mut embedding_engine) = crate::embeddings::EmbeddingEngine::new("bge-small", model_cache) {
                if let Ok(query_embedding) = embedding_engine.embed_text(&self.symbol) {

                    // Find semantically similar symbols that might be references
                    for symbol in symbols.iter() {
                        // Skip if we already found this as a definition
                        if definitions.iter().any(|def| def.id == symbol.id) {
                            continue;
                        }

                        // Create semantic text for comparison
                        let symbol_text = format!("{} {:?}", symbol.name, symbol.kind);
                        if let Ok(symbol_embedding) = embedding_engine.embed_text(&symbol_text) {
                            let similarity = crate::embeddings::cosine_similarity(&query_embedding, &symbol_embedding);

                            // Medium similarity threshold for references (lower than definitions)
                            if similarity > 0.6 && similarity < 0.9 {
                                // Create a semantic relationship
                                let mut metadata = std::collections::HashMap::new();
                                metadata.insert("similarity".to_string(), serde_json::json!(similarity));
                                metadata.insert("context".to_string(), serde_json::json!("Semantic similarity"));
                                metadata.insert("column".to_string(), serde_json::json!(symbol.start_column));

                                let semantic_ref = Relationship {
                                    id: uuid::Uuid::new_v4().to_string(),
                                    from_symbol_id: symbol.id.clone(),
                                    to_symbol_id: definitions.first().map(|d| d.id.clone()).unwrap_or_else(|| "unknown".to_string()),
                                    kind: crate::extractors::base::RelationshipKind::References,
                                    file_path: symbol.file_path.clone(),
                                    line_number: symbol.start_line,
                                    confidence: similarity,
                                    metadata: Some(metadata),
                                };
                                references.push(semantic_ref);
                            }
                        }
                    }

                    // Strategy 4: Find potential usages in signatures/comments
                    for symbol in symbols.iter() {
                        if let Some(signature) = &symbol.signature {
                            if signature.contains(&self.symbol) {
                                let mut metadata = std::collections::HashMap::new();
                                metadata.insert("context".to_string(), serde_json::json!("Found in signature"));
                                metadata.insert("column".to_string(), serde_json::json!(symbol.start_column));

                                let usage_ref = Relationship {
                                    id: uuid::Uuid::new_v4().to_string(),
                                    from_symbol_id: symbol.id.clone(),
                                    to_symbol_id: definitions.first().map(|d| d.id.clone()).unwrap_or_else(|| "unknown".to_string()),
                                    kind: crate::extractors::base::RelationshipKind::Uses,
                                    file_path: symbol.file_path.clone(),
                                    line_number: symbol.start_line,
                                    confidence: 0.8, // High confidence for signature usage
                                    metadata: Some(metadata),
                                };
                                references.push(usage_ref);
                            }
                        }
                    }
                }
            }
        }

        // Sort references by confidence and location
        references.sort_by(|a, b| {
            // First by confidence (descending)
            let conf_cmp = b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal);
            if conf_cmp != std::cmp::Ordering::Equal {
                return conf_cmp;
            }
            // Then by file path
            let file_cmp = a.file_path.cmp(&b.file_path);
            if file_cmp != std::cmp::Ordering::Equal {
                return file_cmp;
            }
            // Finally by line number
            a.line_number.cmp(&b.line_number)
        });

        debug!("✅ Found {} definitions and {} references for '{}'", definitions.len(), references.len(), self.symbol);

        Ok((definitions, references))
    }

    // Helper functions for cross-language naming convention conversion
    // (reuse implementation from GotoDefinitionTool)
    fn to_snake_case(&self, s: &str) -> String {
        let mut result = String::new();
        let mut chars = s.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch.is_uppercase() {
                if !result.is_empty() && chars.peek().map_or(false, |c| c.is_lowercase()) {
                    result.push('_');
                }
                result.push(ch.to_lowercase().next().unwrap());
            } else {
                result.push(ch);
            }
        }
        result
    }

    fn to_camel_case(&self, s: &str) -> String {
        let mut result = String::new();
        let mut capitalize_next = false;

        for ch in s.chars() {
            if ch == '_' {
                capitalize_next = true;
            } else if capitalize_next {
                result.push(ch.to_uppercase().next().unwrap());
                capitalize_next = false;
            } else {
                result.push(ch);
            }
        }
        result
    }

    fn to_pascal_case(&self, s: &str) -> String {
        let camel = self.to_camel_case(s);
        if camel.is_empty() {
            return camel;
        }

        let mut chars = camel.chars();
        match chars.next() {
            None => String::new(),
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        }
    }
}

//******************//
// Semantic Search  //
//******************//
#[mcp_tool(
    name = "semantic_search",
    description = "Search code by meaning and intent using AI embeddings for conceptual matches.",
    title = "Semantic Code Search",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true,
    meta = r#"{"category": "ai_search", "requires": "embeddings"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SemanticSearchTool {
    /// Natural language description of what you're looking for
    pub query: String,
    /// Search mode: hybrid (text + semantic), semantic_only, text_only
    #[serde(default = "default_hybrid")]
    pub mode: String,
    /// Maximum number of results
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_hybrid() -> String { "hybrid".to_string() }

impl SemanticSearchTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("🧠 Semantic search for: {}", self.query);

        // Check if workspace is indexed
        let is_indexed = *handler.is_indexed.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        if !is_indexed {
            let message = "❌ Workspace not indexed yet!\n💡 Run index_workspace first to enable semantic search.";
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // Perform semantic search based on mode
        match self.mode.as_str() {
            "hybrid" => self.hybrid_search(handler).await,
            "semantic_only" => self.semantic_only_search(handler).await,
            "text_only" => self.text_only_search(handler).await,
            _ => {
                let error_msg = format!("❌ Unknown search mode: '{}'\n💡 Supported modes: hybrid, semantic_only, text_only", self.mode);
                Ok(CallToolResult::text_content(vec![TextContent::from(error_msg)]))
            }
        }
    }

    async fn hybrid_search(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("🔄 Performing hybrid semantic + text search");

        // Get symbols from handler (basic implementation)
        let symbols = handler.symbols.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        if symbols.is_empty() {
            let message = "🔍 No symbols found in workspace\n💡 The workspace may need to be re-indexed";
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // Initialize embedding engine for semantic search
        let cache_dir = std::env::temp_dir().join("julie_models");
        std::fs::create_dir_all(&cache_dir)?;

        let mut embedding_engine = crate::embeddings::EmbeddingEngine::new("bge-small", cache_dir)
            .map_err(|e| anyhow::anyhow!("Failed to initialize embedding engine: {}", e))?;

        // Embed the search query
        let query_embedding = embedding_engine.embed_text(&self.query)?;

        // Perform semantic search by comparing with symbol embeddings
        let mut semantic_results = Vec::new();

        for symbol in symbols.iter() {
            // Create embedding text for symbol (similar to what we'd do during indexing)
            let symbol_text = format!("{} {:?} {}",
                symbol.name,
                symbol.kind,
                symbol.signature.as_deref().unwrap_or("")
            );

            // Embed the symbol
            if let Ok(symbol_embedding) = embedding_engine.embed_text(&symbol_text) {
                let similarity = crate::embeddings::cosine_similarity(&query_embedding, &symbol_embedding);

                if similarity > 0.3 { // Minimum similarity threshold
                    semantic_results.push((symbol.clone(), similarity));
                }
            }
        }

        // Sort by similarity score (descending)
        semantic_results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Take only the requested number of results
        semantic_results.truncate(self.limit as usize);

        // Format results
        if semantic_results.is_empty() {
            let message = format!(
                "🧠 Semantic Search for: '{}'\n\
                🔄 Mode: {}\n\
                📊 Searched {} symbols\n\n\
                🔍 No semantically similar symbols found\n\
                💡 Try a broader search term or use text_only mode",
                self.query, self.mode, symbols.len()
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        let mut result_lines = vec![
            format!("🧠 Semantic Search for: '{}'", self.query),
            format!("🔄 Mode: {}", self.mode),
            format!("📊 Found {} results from {} symbols", semantic_results.len(), symbols.len()),
            String::new(),
        ];

        for (i, (symbol, similarity)) in semantic_results.iter().enumerate() {
            result_lines.push(format!(
                "{}. {} [{}]",
                i + 1,
                symbol.name,
                symbol.language
            ));
            result_lines.push(format!(
                "📁 {}/{}:{}-{}",
                std::path::Path::new(&symbol.file_path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown"),
                symbol.file_path,
                symbol.start_line,
symbol.end_line
            ));
            result_lines.push(format!("🏷️ Kind: {:?}", symbol.kind));
            result_lines.push(format!("🎯 Similarity: {:.3}", similarity));
            if let Some(sig) = &symbol.signature {
                result_lines.push(format!("📝 {}", sig));
            }
            result_lines.push(String::new());
        }

        let message = result_lines.join("\n");
        Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
    }

    async fn semantic_only_search(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        // For now, semantic_only is the same as hybrid (pure embedding-based search)
        self.hybrid_search(handler).await
    }

    async fn text_only_search(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("📝 Performing text-only search");

        // Get symbols from handler
        let symbols = handler.symbols.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        if symbols.is_empty() {
            let message = "🔍 No symbols found in workspace\n💡 The workspace may need to be re-indexed";
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // Simple text matching
        let query_lower = self.query.to_lowercase();
        let mut text_results = Vec::new();

        for symbol in symbols.iter() {
            let symbol_text = format!("{} {:?} {}",
                symbol.name.to_lowercase(),
                symbol.kind,
                symbol.signature.as_deref().unwrap_or("").to_lowercase()
            );

            if symbol_text.contains(&query_lower) {
                // Simple scoring based on exact matches and position
                let mut score = 0.0;
                if symbol.name.to_lowercase().contains(&query_lower) {
                    score += 1.0;
                }
                if let Some(sig) = &symbol.signature {
                    if sig.to_lowercase().contains(&query_lower) {
                        score += 0.5;
                    }
                }
                text_results.push((symbol.clone(), score));
            }
        }

        // Sort by score (descending)
        text_results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        text_results.truncate(self.limit as usize);

        // Format results
        if text_results.is_empty() {
            let message = format!(
                "📝 Text Search for: '{}'\n\
                🔄 Mode: {}\n\
                📊 Searched {} symbols\n\n\
                🔍 No matching symbols found\n\
                💡 Try different keywords or use semantic search",
                self.query, self.mode, symbols.len()
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        let mut result_lines = vec![
            format!("📝 Text Search for: '{}'", self.query),
            format!("🔄 Mode: {}", self.mode),
            format!("📊 Found {} results from {} symbols", text_results.len(), symbols.len()),
            String::new(),
        ];

        for (i, (symbol, score)) in text_results.iter().enumerate() {
            result_lines.push(format!(
                "{}. {} [{}]",
                i + 1,
                symbol.name,
                symbol.language
            ));
            result_lines.push(format!(
                "📁 {}/{}:{}-{}",
                std::path::Path::new(&symbol.file_path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown"),
                symbol.file_path,
                symbol.start_line,
symbol.end_line
            ));
            result_lines.push(format!("🏷️ Kind: {:?}", symbol.kind));
            result_lines.push(format!("📊 Score: {:.1}", score));
            if let Some(sig) = &symbol.signature {
                result_lines.push(format!("📝 {}", sig));
            }
            result_lines.push(String::new());
        }

        let message = result_lines.join("\n");
        Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
    }
}

//******************//
//     Explore      //
//******************//
#[mcp_tool(
    name = "fast_explore",
    description = "UNDERSTAND FIRST - Multi-mode codebase exploration (overview/dependencies/trace/hotspots)",
    title = "Fast Codebase Architecture Explorer",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true,
    meta = r#"{"category": "analysis", "scope": "architectural"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FastExploreTool {
    /// Exploration type: overview, dependencies, trace, hotspots
    pub mode: String,
    /// Optional focus area (file, module, class)
    #[serde(default)]
    pub focus: Option<String>,
    /// Analysis depth: shallow, medium, deep
    #[serde(default = "default_medium")]
    pub depth: String,
}

fn default_medium() -> String { "medium".to_string() }

impl FastExploreTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("🧭 Exploring codebase: mode={}, focus={:?}", self.mode, self.focus);

        // Check if workspace is indexed
        let is_indexed = *handler.is_indexed.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        if !is_indexed {
            let message = "❌ Workspace not indexed yet!\n💡 Run index_workspace first to enable exploration.";
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // Perform exploration based on mode
        let message = match self.mode.as_str() {
            "overview" => self.generate_overview(handler)?,
            "dependencies" => self.analyze_dependencies(handler)?,
            "hotspots" => self.find_hotspots(handler)?,
            "trace" => self.trace_relationships(handler)?,
            _ => format!(
                "❌ Unknown exploration mode: '{}'\n\
                💡 Supported modes: overview, dependencies, hotspots, trace",
                self.mode
            ),
        };

        Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
    }

    fn generate_overview(&self, handler: &JulieServerHandler) -> Result<String> {
        let symbols = handler.symbols.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        let relationships = handler.relationships.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        // Count by symbol type
        let mut counts = std::collections::HashMap::new();
        let mut file_counts = std::collections::HashMap::new();
        let mut language_counts = std::collections::HashMap::new();

        for symbol in symbols.iter() {
            *counts.entry(&symbol.kind).or_insert(0) += 1;
            *file_counts.entry(&symbol.file_path).or_insert(0) += 1;
            *language_counts.entry(&symbol.language).or_insert(0) += 1;
        }

        let mut message = format!(
            "🧭 Codebase Overview\n\
            ========================\n\
            📊 Total Symbols: {}\n\
            📁 Total Files: {}\n\
            🔗 Total Relationships: {}\n\n",
            symbols.len(),
            file_counts.len(),
            relationships.len()
        );

        // Symbol breakdown
        message.push_str("🏷️ Symbol Types:\n");
        let mut sorted_counts: Vec<_> = counts.iter().collect();
        sorted_counts.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
        for (kind, count) in sorted_counts {
            message.push_str(&format!("  {:?}: {}\n", kind, count));
        }

        // Language breakdown
        message.push_str("\n💻 Languages:\n");
        let mut sorted_languages: Vec<_> = language_counts.iter().collect();
        sorted_languages.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
        for (lang, count) in sorted_languages {
            message.push_str(&format!("  {}: {} symbols\n", lang, count));
        }

        // Top files by symbol count
        if matches!(self.depth.as_str(), "medium" | "deep") {
            message.push_str("\n📁 Top Files by Symbol Count:\n");
            let mut sorted_files: Vec<_> = file_counts.iter().collect();
            sorted_files.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
            for (file, count) in sorted_files.iter().take(10) {
                let file_name = std::path::Path::new(file)
                    .file_name()
                    .unwrap_or_else(|| std::ffi::OsStr::new(file))
                    .to_string_lossy();
                message.push_str(&format!("  {}: {} symbols\n", file_name, count));
            }
        }

        Ok(message)
    }

    fn analyze_dependencies(&self, handler: &JulieServerHandler) -> Result<String> {
        let relationships = handler.relationships.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        let symbols = handler.symbols.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let mut relationship_counts = std::collections::HashMap::new();
        let mut symbol_references = std::collections::HashMap::new();

        for rel in relationships.iter() {
            *relationship_counts.entry(&rel.kind).or_insert(0) += 1;
            *symbol_references.entry(&rel.to_symbol_id).or_insert(0) += 1;
        }

        let mut message = format!(
            "🔗 Dependency Analysis\n\
            =====================\n\
            Total Relationships: {}\n\n",
            relationships.len()
        );

        // Relationship type breakdown
        message.push_str("🏷️ Relationship Types:\n");
        let mut sorted_rels: Vec<_> = relationship_counts.iter().collect();
        sorted_rels.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
        for (kind, count) in sorted_rels {
            message.push_str(&format!("  {}: {}\n", kind, count));
        }

        // Most referenced symbols
        if matches!(self.depth.as_str(), "medium" | "deep") {
            message.push_str("\n🔥 Most Referenced Symbols:\n");
            let mut sorted_refs: Vec<_> = symbol_references.iter().collect();
            sorted_refs.sort_by_key(|(_, count)| std::cmp::Reverse(**count));

            for (symbol_id, count) in sorted_refs.iter().take(10) {
                if let Some(symbol) = symbols.iter().find(|s| s.id == ***symbol_id) {
                    message.push_str(&format!("  {} [{}]: {} references\n", symbol.name, format!("{:?}", symbol.kind).to_lowercase(), count));
                }
            }
        }

        Ok(message)
    }

    fn find_hotspots(&self, handler: &JulieServerHandler) -> Result<String> {
        let symbols = handler.symbols.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        let relationships = handler.relationships.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        // Find files with most symbols (complexity hotspots)
        let mut file_symbol_counts = std::collections::HashMap::new();
        let mut file_relationship_counts = std::collections::HashMap::new();

        for symbol in symbols.iter() {
            *file_symbol_counts.entry(&symbol.file_path).or_insert(0) += 1;
        }

        for rel in relationships.iter() {
            *file_relationship_counts.entry(&rel.file_path).or_insert(0) += 1;
        }

        let mut message = "🔥 Complexity Hotspots\n=====================\n".to_string();

        message.push_str("📁 Files with Most Symbols:\n");
        let mut sorted_files: Vec<_> = file_symbol_counts.iter().collect();
        sorted_files.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
        for (file, count) in sorted_files.iter().take(10) {
            let file_name = std::path::Path::new(file)
                .file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new(file))
                .to_string_lossy();
            message.push_str(&format!("  {}: {} symbols\n", file_name, count));
        }

        message.push_str("\n🔗 Files with Most Relationships:\n");
        let mut sorted_rel_files: Vec<_> = file_relationship_counts.iter().collect();
        sorted_rel_files.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
        for (file, count) in sorted_rel_files.iter().take(10) {
            let file_name = std::path::Path::new(file)
                .file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new(file))
                .to_string_lossy();
            message.push_str(&format!("  {}: {} relationships\n", file_name, count));
        }

        Ok(message)
    }

    fn trace_relationships(&self, handler: &JulieServerHandler) -> Result<String> {
        let relationships = handler.relationships.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        let symbols = handler.symbols.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let mut message = "🔍 Relationship Tracing\n=====================\n".to_string();

        if let Some(focus) = &self.focus {
            // Find the focused symbol
            if let Some(target_symbol) = symbols.iter().find(|s| s.name == *focus) {
                message.push_str(&format!("Tracing relationships for: {}\n\n", focus));

                // Find incoming relationships (what references this symbol)
                let incoming: Vec<_> = relationships.iter()
                    .filter(|rel| rel.to_symbol_id == target_symbol.id)
                    .collect();

                // Find outgoing relationships (what this symbol references)
                let outgoing: Vec<_> = relationships.iter()
                    .filter(|rel| rel.from_symbol_id == target_symbol.id)
                    .collect();

                message.push_str(&format!("← Incoming ({} relationships):\n", incoming.len()));
                for rel in incoming.iter().take(10) {
                    if let Some(from_symbol) = symbols.iter().find(|s| s.id == rel.from_symbol_id) {
                        message.push_str(&format!("  {} {} this symbol\n", from_symbol.name, rel.kind));
                    }
                }

                message.push_str(&format!("\n→ Outgoing ({} relationships):\n", outgoing.len()));
                for rel in outgoing.iter().take(10) {
                    if let Some(to_symbol) = symbols.iter().find(|s| s.id == rel.to_symbol_id) {
                        message.push_str(&format!("  This symbol {} {}\n", rel.kind, to_symbol.name));
                    }
                }
            } else {
                message.push_str(&format!("❌ Symbol '{}' not found\n", focus));
            }
        } else {
            message.push_str("💡 Use focus parameter to trace a specific symbol\n");
            message.push_str("Example: { \"mode\": \"trace\", \"focus\": \"functionName\" }");
        }

        Ok(message)
    }
}

//******************//
//     Navigate     //
//******************//
#[mcp_tool(
    name = "navigate",
    description = "Navigate through code with surgical precision using various navigation modes.",
    title = "Precise Code Navigation",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true,
    meta = r#"{"category": "navigation", "precision": "surgical"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct NavigateTool {
    /// Navigation mode: definition, references, implementations, callers, callees
    pub mode: String,
    /// Symbol or identifier to navigate from
    pub target: String,
    /// Optional context for disambiguation
    #[serde(default)]
    pub context: Option<String>,
}

impl NavigateTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("🚀 Navigating: mode={}, target={}", self.mode, self.target);

        // Check if workspace is indexed
        let is_indexed = *handler.is_indexed.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        if !is_indexed {
            let message = "❌ Workspace not indexed yet!\n💡 Run index_workspace first to enable navigation.";
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // Perform navigation based on mode
        let message = match self.mode.as_str() {
            "definition" => self.navigate_to_definition(handler)?,
            "references" => self.navigate_to_references(handler)?,
            "implementations" => self.navigate_to_implementations(handler)?,
            "callers" => self.navigate_to_callers(handler)?,
            "callees" => self.navigate_to_callees(handler)?,
            _ => format!(
                "❌ Unknown navigation mode: '{}'\n\
                💡 Supported modes: definition, references, implementations, callers, callees",
                self.mode
            ),
        };

        Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
    }

    fn navigate_to_definition(&self, handler: &JulieServerHandler) -> Result<String> {
        let symbols = handler.symbols.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let definitions: Vec<_> = symbols.iter()
            .filter(|s| s.name == self.target)
            .collect();

        if definitions.is_empty() {
            return Ok(format!("❌ No definition found for: '{}'\n", self.target));
        }

        let mut message = format!("🎯 Definition of '{}':\n", self.target);
        for symbol in definitions {
            message.push_str(&format!(
                "📁 {}:{}:{} [{}]\n",
                symbol.file_path,
                symbol.start_line,
                symbol.start_column,
                format!("{:?}", symbol.kind).to_lowercase()
            ));
            if let Some(signature) = &symbol.signature {
                message.push_str(&format!("📝 {}", signature));
            }
            message.push('\n');
        }

        Ok(message)
    }

    fn navigate_to_references(&self, handler: &JulieServerHandler) -> Result<String> {
        let symbols = handler.symbols.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        let relationships = handler.relationships.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        // Find the target symbol
        let target_symbols: Vec<_> = symbols.iter()
            .filter(|s| s.name == self.target)
            .collect();

        if target_symbols.is_empty() {
            return Ok(format!("❌ Symbol '{}' not found\n", self.target));
        }

        let target_ids: Vec<_> = target_symbols.iter().map(|s| s.id.clone()).collect();

        // Find references in relationships
        let references: Vec<_> = relationships.iter()
            .filter(|rel| target_ids.iter().any(|id| rel.to_symbol_id == *id))
            .collect();

        let mut message = format!("🔗 References to '{}':\n", self.target);
        if references.is_empty() {
            message.push_str("ℹ️ No references found\n");
        } else {
            for rel in references.iter().take(20) {
                message.push_str(&format!(
                    "📁 {}:{} - {} relationship\n",
                    rel.file_path,
                    rel.line_number,
                    rel.kind
                ));
            }
            if references.len() > 20 {
                message.push_str(&format!("... and {} more references\n", references.len() - 20));
            }
        }

        Ok(message)
    }

    fn navigate_to_implementations(&self, handler: &JulieServerHandler) -> Result<String> {
        let relationships = handler.relationships.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        let symbols = handler.symbols.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        // Find symbols that implement the target (interfaces/abstract classes)
        let target_symbols: Vec<_> = symbols.iter()
            .filter(|s| s.name == self.target)
            .collect();

        if target_symbols.is_empty() {
            return Ok(format!("❌ Symbol '{}' not found\n", self.target));
        }

        let target_ids: Vec<_> = target_symbols.iter().map(|s| s.id.clone()).collect();

        let implementations: Vec<_> = relationships.iter()
            .filter(|rel| {
                matches!(rel.kind, crate::extractors::RelationshipKind::Implements) &&
                target_ids.iter().any(|id| rel.to_symbol_id == *id)
            })
            .collect();

        let mut message = format!("🛠️ Implementations of '{}':\n", self.target);
        if implementations.is_empty() {
            message.push_str("ℹ️ No implementations found\n");
        } else {
            for rel in implementations {
                if let Some(impl_symbol) = symbols.iter().find(|s| s.id == rel.from_symbol_id) {
                    message.push_str(&format!(
                        "📁 {} - {}:{}:{}\n",
                        impl_symbol.name,
                        impl_symbol.file_path,
                        impl_symbol.start_line,
                        impl_symbol.start_column
                    ));
                }
            }
        }

        Ok(message)
    }

    fn navigate_to_callers(&self, handler: &JulieServerHandler) -> Result<String> {
        let relationships = handler.relationships.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        let symbols = handler.symbols.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        // Find the target function
        let target_symbols: Vec<_> = symbols.iter()
            .filter(|s| s.name == self.target && matches!(s.kind, SymbolKind::Function | SymbolKind::Method))
            .collect();

        if target_symbols.is_empty() {
            return Ok(format!("❌ Function '{}' not found\n", self.target));
        }

        let target_ids: Vec<_> = target_symbols.iter().map(|s| s.id.clone()).collect();

        let callers: Vec<_> = relationships.iter()
            .filter(|rel| {
                matches!(rel.kind, crate::extractors::RelationshipKind::Calls) &&
                target_ids.iter().any(|id| rel.to_symbol_id == *id)
            })
            .collect();

        let mut message = format!("📞 Callers of '{}':\n", self.target);
        if callers.is_empty() {
            message.push_str("ℹ️ No callers found\n");
        } else {
            for rel in callers {
                if let Some(caller_symbol) = symbols.iter().find(|s| s.id == rel.from_symbol_id) {
                    message.push_str(&format!(
                        "📁 {} calls this at {}:{}\n",
                        caller_symbol.name,
                        rel.file_path,
                        rel.line_number
                    ));
                }
            }
        }

        Ok(message)
    }

    fn navigate_to_callees(&self, handler: &JulieServerHandler) -> Result<String> {
        let relationships = handler.relationships.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        let symbols = handler.symbols.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        // Find the target function
        let target_symbols: Vec<_> = symbols.iter()
            .filter(|s| s.name == self.target && matches!(s.kind, SymbolKind::Function | SymbolKind::Method))
            .collect();

        if target_symbols.is_empty() {
            return Ok(format!("❌ Function '{}' not found\n", self.target));
        }

        let target_ids: Vec<_> = target_symbols.iter().map(|s| s.id.clone()).collect();

        let callees: Vec<_> = relationships.iter()
            .filter(|rel| {
                matches!(rel.kind, crate::extractors::RelationshipKind::Calls) &&
                target_ids.iter().any(|id| rel.from_symbol_id == *id)
            })
            .collect();

        let mut message = format!("📤 Functions called by '{}':\n", self.target);
        if callees.is_empty() {
            message.push_str("ℹ️ No function calls found\n");
        } else {
            for rel in callees {
                if let Some(callee_symbol) = symbols.iter().find(|s| s.id == rel.to_symbol_id) {
                    message.push_str(&format!(
                        "📁 calls {} at {}:{}\n",
                        callee_symbol.name,
                        rel.file_path,
                        rel.line_number
                    ));
                }
            }
        }

        Ok(message)
    }
}

//******************//
// Phase 6.1: Heart of Codebase Intelligence Tools //
//******************//

/// Find critical files, filter noise, and provide architectural overview
#[mcp_tool(
    name = "explore_overview",
    description = "Intelligent codebase overview - find critical files, filter noise, detect architecture patterns.",
    title = "Heart of Codebase - Overview",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true,
    meta = r#"{"category": "intelligence", "priority": "critical"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ExploreOverviewTool {
    /// Focus area: "critical_files", "architecture", "entry_points", "data_flows"
    #[serde(default = "default_critical_files")]
    pub focus: String,
    /// Maximum number of critical files to return
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Include architectural pattern detection
    #[serde(default = "default_true")]
    pub include_architecture: bool,
    /// Filter out boilerplate/framework code
    #[serde(default = "default_true")]
    pub filter_noise: bool,
}

fn default_critical_files() -> String { "critical_files".to_string() }

/// Trace execution flow across the entire polyglot stack
#[mcp_tool(
    name = "trace_execution",
    description = "Revolutionary cross-language execution tracing - follow data flow from UI to database across all languages.",
    title = "Polyglot Execution Tracer",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true,
    meta = r#"{"category": "intelligence", "feature": "revolutionary"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct TraceExecutionTool {
    /// Starting symbol/function name to trace from
    pub start_point: String,
    /// Maximum trace depth (layers to follow)
    #[serde(default = "default_trace_depth")]
    pub max_depth: u32,
    /// Include semantic connections (embedding-based)
    #[serde(default = "default_true")]
    pub include_semantic: bool,
    /// Minimum confidence threshold for trace steps
    #[serde(default = "default_confidence")]
    pub min_confidence: f32,
}

fn default_trace_depth() -> u32 { 10 }
fn default_confidence() -> f32 { 0.6 }

/// Get exactly the context needed for AI - no more, no less
#[mcp_tool(
    name = "get_minimal_context",
    description = "Smart AI context optimization - get exactly the code context needed within token limits.",
    title = "AI Context Optimizer",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true,
    meta = r#"{"category": "intelligence", "purpose": "ai_optimization"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetMinimalContextTool {
    /// Target symbol/function to get context for
    pub target: String,
    /// Maximum tokens for context (approximate)
    #[serde(default = "default_context_tokens")]
    pub max_tokens: u32,
    /// Include dependency context
    #[serde(default = "default_true")]
    pub include_dependencies: bool,
    /// Include usage examples
    #[serde(default = "default_false")]
    pub include_examples: bool,
}

fn default_context_tokens() -> u32 { 4000 }
fn default_false() -> bool { false }

/// Find business logic, filter out framework/boilerplate noise
#[mcp_tool(
    name = "find_logic",
    description = "DISCOVER CORE LOGIC - Filter framework noise, focus on domain business logic",
    title = "Find Business Logic",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true,
    meta = r#"{"category": "intelligence", "filter": "business_logic"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FindLogicTool {
    /// Domain concept to search for (e.g., "user authentication", "payment processing")
    pub domain: String,
    /// Maximum results to return
    #[serde(default = "default_limit")]
    pub max_results: u32,
    /// Group by architectural layer
    #[serde(default = "default_true")]
    pub group_by_layer: bool,
    /// Minimum business logic confidence score
    #[serde(default = "default_business_confidence")]
    pub min_business_score: f32,
}

fn default_business_confidence() -> f32 { 0.7 }

/// Score code criticality and importance (0-100)
#[mcp_tool(
    name = "score_criticality",
    description = "Calculate code criticality scores - identify the most important symbols/files for AI focus.",
    title = "Criticality Scoring Engine",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true,
    meta = r#"{"category": "intelligence", "metric": "criticality"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ScoreCriticalityTool {
    /// Target to score: symbol name, file path, or "all" for overview
    pub target: String,
    /// Include detailed breakdown of scoring factors
    #[serde(default = "default_true")]
    pub include_breakdown: bool,
    /// Score type: "symbol", "file", "overview"
    #[serde(default = "default_symbol")]
    pub score_type: String,
}

fn default_symbol() -> String { "symbol".to_string() }

//******************//
// Phase 6.1 Intelligence Tool Implementations //
//******************//

impl ExploreOverviewTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("🧭 Exploring codebase overview: focus={}", self.focus);

        // Check if workspace is indexed
        let is_indexed = *handler.is_indexed.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        if !is_indexed {
            let message = "❌ Workspace not indexed yet!\n💡 Run index_workspace first to enable intelligent overview.";
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        match self.focus.as_str() {
            "critical_files" => self.find_critical_files(handler).await,
            "architecture" => self.detect_architecture(handler).await,
            "entry_points" => self.find_entry_points(handler).await,
            "data_flows" => self.analyze_data_flows(handler).await,
            _ => {
                let message = format!(
                    "❌ Unknown focus area: '{}'\n\
                    💡 Supported: critical_files, architecture, entry_points, data_flows",
                    self.focus
                );
                Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
            }
        }
    }

    /// Find the most critical files in the codebase - the "heart" files
    async fn find_critical_files(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        let symbols = handler.symbols.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        let relationships = handler.relationships.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        // Calculate criticality scores for each file
        let mut file_scores = std::collections::HashMap::new();
        let mut file_symbol_counts = std::collections::HashMap::new();
        let mut file_languages = std::collections::HashMap::new();

        // Count symbols and relationships per file
        for symbol in symbols.iter() {
            *file_symbol_counts.entry(&symbol.file_path).or_insert(0) += 1;
            file_languages.insert(symbol.file_path.clone(), symbol.language.clone());

            // Base score from symbol importance
            let symbol_score = match symbol.kind {
                SymbolKind::Class | SymbolKind::Interface => 10.0,
                SymbolKind::Function | SymbolKind::Method => 5.0,
                SymbolKind::Type | SymbolKind::Enum => 3.0,
                _ => 1.0,
            };
            *file_scores.entry(&symbol.file_path).or_insert(0.0) += symbol_score;
        }

        // Boost scores based on relationships (how connected the file is)
        for rel in relationships.iter() {
            *file_scores.entry(&rel.file_path).or_insert(0.0) += 2.0;
        }

        // Apply noise filtering if enabled
        let mut scored_files: Vec<_> = file_scores.iter()
            .map(|(path, score)| {
                let adjusted_score = if self.filter_noise {
                    self.adjust_score_for_noise(path, *score)
                } else {
                    *score
                };
                ((*path).clone(), adjusted_score)
            })
            .collect();

        // Sort by criticality score (highest first)
        scored_files.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Format results
        let mut message = format!(
            "🎯 **Critical Files Analysis** (Top {})\n\
            ====================================\n\n",
            self.limit.min(scored_files.len() as u32)
        );

        for (i, (file_path, score)) in scored_files.iter().take(self.limit as usize).enumerate() {
            let file_name = std::path::Path::new(file_path)
                .file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new(file_path))
                .to_string_lossy();

            let symbol_count = file_symbol_counts.get(file_path).unwrap_or(&0);
            let language = file_languages.get(file_path).map(|s| s.as_str()).unwrap_or("unknown");

            message.push_str(&format!(
                "{}. **{}** [{}]\n\
                   📊 Criticality: {:.1} | 🔍 Symbols: {} | 📁 {}\n\n",
                i + 1, file_name, language, score, symbol_count, file_path
            ));
        }

        // Add architectural insights if requested
        if self.include_architecture {
            message.push_str(&self.add_architectural_insights(&scored_files, &file_languages)?);
        }

        Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
    }

    /// Detect architectural patterns in the codebase
    async fn detect_architecture(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        let symbols = handler.symbols.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        let relationships = handler.relationships.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let mut message = "🏗️ **Architecture Analysis**\n========================\n\n".to_string();

        // Language distribution
        let mut language_counts = std::collections::HashMap::new();
        let mut layer_detection = std::collections::HashMap::new();

        for symbol in symbols.iter() {
            *language_counts.entry(&symbol.language).or_insert(0) += 1;

            // Detect architectural layers based on file paths
            let layer = self.detect_layer_from_path(&symbol.file_path);
            layer_detection.insert(layer.clone(), layer_detection.get(&layer).unwrap_or(&0) + 1);
        }

        // Multi-language architecture detection
        message.push_str("🌐 **Technology Stack:**\n");
        let mut sorted_langs: Vec<_> = language_counts.iter().collect();
        sorted_langs.sort_by_key(|(_, count)| std::cmp::Reverse(**count));

        for (lang, count) in sorted_langs {
            let percentage = (*count as f32 / symbols.len() as f32) * 100.0;
            message.push_str(&format!("  • {}: {} symbols ({:.1}%)\n", lang, count, percentage));
        }

        // Architectural pattern detection
        message.push_str("\n🏛️ **Detected Patterns:**\n");
        let patterns = self.detect_architectural_patterns(&symbols, &relationships);
        for pattern in patterns {
            message.push_str(&format!("  • {}\n", pattern));
        }

        // Layer analysis
        message.push_str("\n📚 **Architectural Layers:**\n");
        let mut sorted_layers: Vec<_> = layer_detection.iter().collect();
        sorted_layers.sort_by_key(|(_, count)| std::cmp::Reverse(**count));

        for (layer, count) in sorted_layers {
            if *count > 5 { // Only show significant layers
                message.push_str(&format!("  • {}: {} symbols\n", layer, count));
            }
        }

        Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
    }

    /// Find main entry points (main functions, controllers, etc.)
    async fn find_entry_points(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        let symbols = handler.symbols.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let mut entry_points = Vec::new();

        for symbol in symbols.iter() {
            if self.is_entry_point(symbol) {
                entry_points.push(symbol.clone());
            }
        }

        // Sort by importance (main functions first, then controllers, etc.)
        entry_points.sort_by_key(|symbol| self.entry_point_priority(symbol));

        let mut message = format!(
            "🚪 **Entry Points Analysis** ({} found)\n\
            =======================================\n\n",
            entry_points.len()
        );

        if entry_points.is_empty() {
            message.push_str("ℹ️ No clear entry points detected.\n💡 This might be a library or the analysis needs refinement.");
        } else {
            for (i, symbol) in entry_points.iter().take(self.limit as usize).enumerate() {
                let entry_type = self.classify_entry_point(symbol);
                message.push_str(&format!(
                    "{}. **{}** [{}]\n\
                       🏷️ Type: {} | 📁 {}:{}:{}\n",
                    i + 1,
                    symbol.name,
                    symbol.language,
                    entry_type,
                    symbol.file_path,
                    symbol.start_line,
                    symbol.start_column
                ));

                if let Some(signature) = &symbol.signature {
                    message.push_str(&format!("   📝 {}\n", signature));
                }
                message.push('\n');
            }
        }

        Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
    }

    /// Analyze main data flow patterns
    async fn analyze_data_flows(&self, _handler: &JulieServerHandler) -> Result<CallToolResult> {
        let message = "🌊 **Data Flow Analysis**\n\
                      =======================\n\n\
                      🚧 Advanced data flow analysis coming soon!\n\
                      🎯 Will trace data movement across:\n\
                      • UI Components → Services → APIs\n\
                      • Controllers → Business Logic → Databases\n\
                      • Cross-language data transformations\n\n\
                      💡 Use trace_execution for now to trace specific flows.".to_string();

        Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
    }

    /// Adjust criticality score based on noise filtering
    fn adjust_score_for_noise(&self, file_path: &str, base_score: f32) -> f32 {
        let path_lower = file_path.to_lowercase();

        // Reduce score for likely boilerplate/framework files
        if path_lower.contains("test") || path_lower.contains("spec") {
            return base_score * 0.3; // Test files are less critical for understanding
        }

        if path_lower.contains("config") || path_lower.contains("setting") {
            return base_score * 0.5; // Config files are important but not core logic
        }

        if path_lower.contains("migration") || path_lower.contains("seed") {
            return base_score * 0.4; // Database migrations are less critical
        }

        // Boost score for likely core business files
        if path_lower.contains("service") || path_lower.contains("controller") ||
           path_lower.contains("model") || path_lower.contains("entity") ||
           path_lower.contains("domain") || path_lower.contains("business") {
            return base_score * 1.5; // Business logic is more critical
        }

        if path_lower.contains("main") || path_lower.contains("index") ||
           path_lower.contains("app") {
            return base_score * 2.0; // Entry points are very critical
        }

        base_score
    }

    /// Add architectural insights to the analysis
    fn add_architectural_insights(&self, scored_files: &[(String, f32)], file_languages: &std::collections::HashMap<String, String>) -> Result<String> {
        let mut insights = "\n🏗️ **Architectural Insights:**\n".to_string();

        // Multi-language detection
        let unique_languages: std::collections::HashSet<_> = file_languages.values().collect();
        if unique_languages.len() > 3 {
            insights.push_str(&format!("  🌐 Polyglot architecture detected ({} languages)\n", unique_languages.len()));
        }

        // High-criticality file concentration
        let top_10_avg = scored_files.iter().take(10).map(|(_, score)| score).sum::<f32>() / 10.0;
        let overall_avg = scored_files.iter().map(|(_, score)| score).sum::<f32>() / scored_files.len() as f32;

        if top_10_avg > overall_avg * 3.0 {
            insights.push_str("  🎯 High concentration of critical code (potential refactoring opportunity)\n");
        }

        // Framework detection
        let framework_indicators = [
            ("React", "tsx"), ("Vue", "vue"), ("Angular", "component"),
            ("Spring", "Controller"), ("Django", "models"), ("Rails", "controller"),
            ("Express", "router"), ("FastAPI", "endpoint")
        ];

        for (framework, indicator) in framework_indicators {
            if scored_files.iter().any(|(path, _)| path.to_lowercase().contains(&indicator.to_lowercase())) {
                insights.push_str(&format!("  🚀 {} framework detected\n", framework));
            }
        }

        Ok(insights)
    }

    /// Detect architectural layer from file path
    fn detect_layer_from_path(&self, path: &str) -> String {
        let path_lower = path.to_lowercase();

        if path_lower.contains("controller") || path_lower.contains("router") || path_lower.contains("endpoint") {
            "API Layer".to_string()
        } else if path_lower.contains("service") || path_lower.contains("business") || path_lower.contains("domain") {
            "Business Layer".to_string()
        } else if path_lower.contains("model") || path_lower.contains("entity") || path_lower.contains("repository") {
            "Data Layer".to_string()
        } else if path_lower.contains("component") || path_lower.contains("view") || path_lower.contains("ui") {
            "Presentation Layer".to_string()
        } else if path_lower.contains("config") || path_lower.contains("util") || path_lower.contains("helper") {
            "Infrastructure Layer".to_string()
        } else {
            "Core Logic".to_string()
        }
    }

    /// Detect architectural patterns based on symbols and relationships
    fn detect_architectural_patterns(&self, symbols: &[Symbol], relationships: &[Relationship]) -> Vec<String> {
        let mut patterns = Vec::new();

        // MVC pattern detection
        let has_controllers = symbols.iter().any(|s| s.name.to_lowercase().contains("controller"));
        let has_models = symbols.iter().any(|s| s.name.to_lowercase().contains("model") ||
                                                 matches!(s.kind, SymbolKind::Class));
        let has_views = symbols.iter().any(|s| s.file_path.to_lowercase().contains("view") ||
                                               s.file_path.to_lowercase().contains("template"));

        if has_controllers && has_models && has_views {
            patterns.push("MVC (Model-View-Controller) Architecture".to_string());
        }

        // Microservices indicators
        let service_count = symbols.iter()
            .filter(|s| s.name.to_lowercase().contains("service"))
            .count();
        if service_count > 5 {
            patterns.push(format!("Service-Oriented Architecture ({} services)", service_count));
        }

        // Repository pattern
        let has_repositories = symbols.iter().any(|s| s.name.to_lowercase().contains("repository"));
        if has_repositories {
            patterns.push("Repository Pattern".to_string());
        }

        // High relationship density (complex architecture)
        let relationship_density = relationships.len() as f32 / symbols.len() as f32;
        if relationship_density > 2.0 {
            patterns.push("High Interconnectivity (Complex Architecture)".to_string());
        }

        if patterns.is_empty() {
            patterns.push("Custom/Unknown Architecture Pattern".to_string());
        }

        patterns
    }

    /// Check if a symbol represents an entry point
    fn is_entry_point(&self, symbol: &Symbol) -> bool {
        // Main functions
        if symbol.name == "main" || symbol.name == "Main" {
            return true;
        }

        // HTTP controllers/endpoints
        if symbol.name.to_lowercase().contains("controller") ||
           symbol.name.to_lowercase().contains("endpoint") ||
           symbol.name.to_lowercase().contains("handler") {
            return true;
        }

        // React/Vue components that might be root components
        if (symbol.language == "typescript" || symbol.language == "javascript") &&
           (symbol.name == "App" || symbol.name == "Root" || symbol.name == "Main") {
            return true;
        }

        // CLI entry points
        if symbol.signature.as_ref().map_or(false, |sig| sig.contains("args")) &&
           matches!(symbol.kind, SymbolKind::Function | SymbolKind::Method) {
            return true;
        }

        false
    }

    /// Get priority for entry point (lower = higher priority)
    fn entry_point_priority(&self, symbol: &Symbol) -> u8 {
        if symbol.name == "main" || symbol.name == "Main" { return 1; }
        if symbol.name.to_lowercase().contains("controller") { return 2; }
        if symbol.name == "App" { return 3; }
        if symbol.name.to_lowercase().contains("handler") { return 4; }
        5 // Default
    }

    /// Classify the type of entry point
    fn classify_entry_point(&self, symbol: &Symbol) -> String {
        if symbol.name == "main" || symbol.name == "Main" {
            "Application Entry Point".to_string()
        } else if symbol.name.to_lowercase().contains("controller") {
            "HTTP Controller".to_string()
        } else if symbol.name == "App" && (symbol.language == "typescript" || symbol.language == "javascript") {
            "React/JS App Component".to_string()
        } else if symbol.name.to_lowercase().contains("handler") {
            "Event Handler".to_string()
        } else {
            "Entry Point".to_string()
        }
    }
}

impl TraceExecutionTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("🔍 Tracing execution flow from: {}", self.start_point);

        // Check if workspace is indexed
        let is_indexed = *handler.is_indexed.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        if !is_indexed {
            let message = "❌ Workspace not indexed yet!\n💡 Run index_workspace first to enable execution tracing.";
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        let message = format!(
            "🔍 **Cross-Language Execution Tracing**\n\
            ========================================\n\n\
            🎯 Tracing from: {}\n\
            📊 Max depth: {}\n\
            🧠 Semantic connections: {}\n\
            ⚡ Min confidence: {:.1}\n\n\
            🚧 Revolutionary polyglot tracing coming soon!\n\
            🌊 Will trace data flow across:\n\
            • React Components → TypeScript Services\n\
            • API Controllers → C# Business Logic\n\
            • Database Calls → SQL Procedures\n\
            • Cross-language dependency chains\n\n\
            💡 This will be the first code intelligence platform capable of\n\
            complete polyglot stack understanding!",
            self.start_point,
            self.max_depth,
            self.include_semantic,
            self.min_confidence
        );

        Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
    }
}

impl GetMinimalContextTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("🎯 Getting minimal context for: {}", self.target);

        // Check if workspace is indexed
        let is_indexed = *handler.is_indexed.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        if !is_indexed {
            let message = "❌ Workspace not indexed yet!\n💡 Run index_workspace first to enable context optimization.";
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        let message = format!(
            "🎯 **AI Context Optimization**\n\
            ===============================\n\n\
            🎯 Target: {}\n\
            📊 Max tokens: {}\n\
            🔗 Include dependencies: {}\n\
            📚 Include examples: {}\n\n\
            🚧 Smart context optimization coming soon!\n\
            🧠 Will provide exactly the right context for AI:\n\
            • Intelligent dependency ranking\n\
            • Smart code chunking (preserve meaning)\n\
            • Token-aware context fitting\n\
            • Remove framework noise, keep business logic\n\
            • Usage examples when helpful\n\n\
            💡 This will maximize AI understanding within token limits!",
            self.target,
            self.max_tokens,
            self.include_dependencies,
            self.include_examples
        );

        Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
    }
}

impl FindLogicTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("🏢 Finding business logic for domain: {}", self.domain);

        // Check if workspace is indexed
        let is_indexed = *handler.is_indexed.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        if !is_indexed {
            let message = "❌ Workspace not indexed yet!\n💡 Run index_workspace first to enable business logic detection.";
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        let message = format!(
            "🏢 **Business Logic Detection**\n\
            ==============================\n\n\
            🎯 Domain: {}\n\
            📊 Max results: {}\n\
            🏛️ Group by layer: {}\n\
            ⚡ Min business score: {:.1}\n\n\
            🚧 Intelligent business logic detection coming soon!\n\
            🎯 Will filter framework noise and focus on:\n\
            • Core domain logic (high business value)\n\
            • Service layer business rules\n\
            • Domain entities and aggregates\n\
            • Business process workflows\n\
            • Validation and business constraints\n\n\
            💡 Perfect for understanding what the code actually does!",
            self.domain,
            self.max_results,
            self.group_by_layer,
            self.min_business_score
        );

        Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
    }
}

impl ScoreCriticalityTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("📊 Scoring criticality for: {}", self.target);

        // Check if workspace is indexed
        let is_indexed = *handler.is_indexed.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        if !is_indexed {
            let message = "❌ Workspace not indexed yet!\n💡 Run index_workspace first to enable criticality scoring.";
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        let message = format!(
            "📊 **Criticality Scoring Engine**\n\
            ==================================\n\n\
            🎯 Target: {}\n\
            📈 Score type: {}\n\
            📋 Include breakdown: {}\n\n\
            🚧 Advanced criticality scoring coming soon!\n\
            📊 Will calculate 0-100 criticality scores based on:\n\
            • Usage frequency (how often referenced)\n\
            • Cross-language dependencies\n\
            • Business logic importance\n\
            • Entry point proximity\n\
            • Architectural significance\n\n\
            💡 Perfect for AI agents to focus on what matters most!",
            self.target,
            self.score_type,
            self.include_breakdown
        );

        Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
    }
}

//******************//
//   Fast Edit      //
//******************//
/// Surgical code editing with automatic rollback and validation
#[mcp_tool(
    name = "fast_edit",
    description = "EDIT WITH CONFIDENCE - Surgical code changes that preserve structure with automatic rollback",
    title = "Fast Surgical Code Editor",
    idempotent_hint = false,
    destructive_hint = true,
    open_world_hint = false,
    read_only_hint = false,
    meta = r#"{"category": "editing", "safety": "auto_rollback"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FastEditTool {
    /// Path to the file to edit
    pub file_path: String,
    /// The exact text to find and replace
    pub find_text: String,
    /// The replacement text
    pub replace_text: String,
    /// Validate changes before applying (default: true)
    #[serde(default = "default_true")]
    pub validate: bool,
    /// Create backup before editing (default: true)
    #[serde(default = "default_true")]
    pub backup: bool,
    /// Dry run mode - show what would be changed without applying (default: false)
    #[serde(default)]
    pub dry_run: bool,
}

impl FastEditTool {
    pub async fn call_tool(&self, _handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("⚡ Fast edit: {} -> replace '{}' with '{}'",
               self.file_path, self.find_text, self.replace_text);

        // Validate inputs
        if self.find_text.is_empty() {
            let message = "❌ find_text cannot be empty\n💡 Specify the exact text to find and replace";
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        if self.find_text == self.replace_text {
            let message = "❌ find_text and replace_text are identical\n💡 No changes needed";
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // Check if file exists
        if !std::path::Path::new(&self.file_path).exists() {
            let message = format!("❌ File not found: {}\n💡 Check the file path", self.file_path);
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // Read current file content
        let original_content = match fs::read_to_string(&self.file_path) {
            Ok(content) => content,
            Err(e) => {
                let message = format!("❌ Failed to read file: {}\n💡 Check file permissions", e);
                return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
            }
        };

        // Check if find_text exists in the file
        if !original_content.contains(&self.find_text) {
            let message = format!(
                "❌ Text not found in file: '{}'\n\
                💡 Check the exact text to find (case sensitive)",
                self.find_text
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // Perform the replacement
        let modified_content = original_content.replace(&self.find_text, &self.replace_text);

        // Calculate diff using diffy
        let patch = diffy::create_patch(&original_content, &modified_content);

        if self.dry_run {
            let message = format!(
                "🔍 Dry run mode - showing changes to: {}\n\
                📊 Changes preview:\n\n{}\n\n\
                💡 Set dry_run=false to apply changes",
                self.file_path, patch
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // Create backup if requested
        let backup_path = if self.backup {
            let backup_path = format!("{}.backup", self.file_path);
            match fs::write(&backup_path, &original_content) {
                Ok(_) => Some(backup_path),
                Err(e) => {
                    warn!("Failed to create backup: {}", e);
                    None
                }
            }
        } else {
            None
        };

        // Basic validation (syntax check would go here)
        if self.validate {
            let validation_result = self.validate_changes(&modified_content);
            if let Err(validation_error) = validation_result {
                let message = format!(
                    "❌ Validation failed: {}\n\
                    💡 Changes would break the code structure",
                    validation_error
                );
                return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
            }
        }

        // Apply changes
        match fs::write(&self.file_path, &modified_content) {
            Ok(_) => {
                let changes_count = self.find_text.lines().count().max(self.replace_text.lines().count());
                let backup_info = if let Some(backup) = backup_path {
                    format!("\n💾 Backup created: {}", backup)
                } else {
                    String::new()
                };

                let message = format!(
                    "✅ Fast edit successful!\n\
                    📁 File: {}\n\
                    📊 Changed {} line(s)\n\
                    🔍 Diff:\n{}{}\n\n\
                    🎯 Next actions:\n\
                    • Run tests to verify changes\n\
                    • Use fast_refs to check impact\n\
                    • Use fast_search to find related code",
                    self.file_path, changes_count, patch, backup_info
                );
                Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
            },
            Err(e) => {
                let message = format!("❌ Failed to write file: {}\n💡 Check file permissions", e);
                Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
            }
        }
    }

    /// Basic validation to prevent obviously broken code
    fn validate_changes(&self, content: &str) -> Result<()> {
        // Basic brace/bracket matching
        let mut braces = 0i32;
        let mut brackets = 0i32;
        let mut parens = 0i32;

        for ch in content.chars() {
            match ch {
                '{' => braces += 1,
                '}' => braces -= 1,
                '[' => brackets += 1,
                ']' => brackets -= 1,
                '(' => parens += 1,
                ')' => parens -= 1,
                _ => {}
            }
        }

        if braces != 0 {
            return Err(anyhow::anyhow!("Unmatched braces {} ({})", "{}", braces));
        }
        if brackets != 0 {
            return Err(anyhow::anyhow!("Unmatched brackets [] ({})", brackets));
        }
        if parens != 0 {
            return Err(anyhow::anyhow!("Unmatched parentheses () ({})", parens));
        }

        Ok(())
    }
}

//******************//
//   JulieTools     //
//******************//
// Generates the JulieTools enum with all tool variants
tool_box!(JulieTools, [
    // Core tools - optimized for speed and adoption
    IndexWorkspaceTool,
    FastSearchTool,     // Merged: SearchCodeTool + SemanticSearchTool
    FastGotoTool,       // Renamed: GotoDefinitionTool
    FastRefsTool,       // Renamed: FindReferencesTool
    FastExploreTool,    // Renamed: ExploreTool (absorbs overview/trace/context)
    FindLogicTool,      // Renamed: FindBusinessLogicTool
    FastEditTool,       // NEW: Surgical editing with diffy + validation
    // TODO: BatchOpsTool - workspace-wide operations
    // Removed: NavigateTool (redundant with FastGotoTool)
    // Removed: ExploreOverviewTool (merged into FastExploreTool)
    // Removed: TraceExecutionTool (merged into FastExploreTool)
    // Removed: GetMinimalContextTool (merged into FastSearchTool)
    // Removed: ScoreCriticalityTool (merged into FastExploreTool)
]);