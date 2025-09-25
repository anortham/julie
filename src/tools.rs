use rust_mcp_sdk::schema::{schema_utils::CallToolError, CallToolResult, TextContent};
use rust_mcp_sdk::{macros::mcp_tool, tool_box};
use rust_mcp_sdk::macros::JsonSchema;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use tracing::{info, debug, warn, error};
use std::path::{Path, PathBuf};
use std::fs;
use std::collections::HashSet;
use sha2::{Sha256, Digest};

use crate::handler::JulieServerHandler;
use crate::extractors::{Symbol, SymbolKind, Relationship, BaseExtractor};
use crate::workspace::JulieWorkspace;

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
        let workspace = self.initialize_workspace(&workspace_path, force_reindex)?;

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
    name = "search_code",
    description = "Search for code symbols, functions, classes across all supported languages with fuzzy matching.",
    title = "Code Search with Fuzzy Matching",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true,
    meta = r#"{"category": "search", "performance": "sub_10ms"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SearchCodeTool {
    /// Search query (symbol name, function name, etc.)
    pub query: String,
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

impl SearchCodeTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("🔍 Searching for: {}", self.query);

        // Check if workspace is indexed
        let is_indexed = *handler.is_indexed.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        if !is_indexed {
            let message = "❌ Workspace not indexed yet!\n💡 Run index_workspace first to enable search.";
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // Perform search
        let results = self.search_symbols(handler)?;

        if results.is_empty() {
            let message = format!(
                "🔍 No results found for: '{}'\n\
                💡 Try a broader search term or check the spelling",
                self.query
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // Format results
        let mut message = format!(
            "🔍 Found {} results for: '{}'\n\n",
            results.len().min(self.limit as usize),
            self.query
        );

        for (i, symbol) in results.iter().take(self.limit as usize).enumerate() {
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

        if results.len() > self.limit as usize {
            message.push_str(&format!("\n... and {} more results\n", results.len() - self.limit as usize));
        }

        Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
    }

    fn search_symbols(&self, handler: &JulieServerHandler) -> Result<Vec<Symbol>> {
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
}

//******************//
// Goto Definition  //
//******************//
#[mcp_tool(
    name = "goto_definition",
    description = "Navigate to the definition of a symbol with precise location information.",
    title = "Go to Definition",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true,
    meta = r#"{"category": "navigation", "precision": "line_level"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GotoDefinitionTool {
    /// Symbol name to find definition for
    pub symbol: String,
    /// Optional context file path for better resolution
    #[serde(default)]
    pub context_file: Option<String>,
    /// Optional line number for context
    #[serde(default)]
    pub line_number: Option<u32>,
}

impl GotoDefinitionTool {
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

        // Find exact name matches first, then partial matches
        let mut exact_matches: Vec<Symbol> = symbols.iter()
            .filter(|symbol| symbol.name == self.symbol)
            .cloned()
            .collect();

        // Sort exact matches by priority (prefer classes, functions over variables)
        exact_matches.sort_by_key(|s| self.definition_priority(&s.kind));

        // If we have context file, prioritize symbols from that file or nearby
        if let Some(context_file) = &self.context_file {
            exact_matches.sort_by(|a, b| {
                let a_in_context = a.file_path.contains(context_file);
                let b_in_context = b.file_path.contains(context_file);
                match (a_in_context, b_in_context) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => std::cmp::Ordering::Equal,
                }
            });
        }

        Ok(exact_matches)
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
    name = "find_references",
    description = "Find all references to a symbol across the codebase.",
    title = "Find All References",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true,
    meta = r#"{"category": "navigation", "scope": "workspace"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FindReferencesTool {
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

impl FindReferencesTool {
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

        // Find symbol definitions
        let definitions: Vec<Symbol> = symbols.iter()
            .filter(|symbol| symbol.name == self.symbol)
            .cloned()
            .collect();

        // Get symbol IDs for reference search
        let symbol_ids: Vec<String> = definitions.iter().map(|s| s.id.clone()).collect();

        // Find relationships where this symbol is referenced
        let references: Vec<Relationship> = relationships.iter()
            .filter(|rel| {
                symbol_ids.iter().any(|id| rel.to_symbol_id == *id || rel.from_symbol_id == *id)
            })
            .cloned()
            .collect();

        Ok((definitions, references))
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
    pub async fn call_tool(&self, _handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("🧠 Semantic search for: {}", self.query);

        // TODO: Implement semantic search with ONNX embeddings
        let message = format!(
            "🧠 Semantic Search for: '{}'\n\
            🔄 Mode: {}\n\
            📊 Limit: {}\n\n\
            🚧 Semantic search not yet implemented\n\
            🎯 Will use ONNX embeddings for meaning-based code search\n\
            💡 Use search_code for now for basic text-based search",
            self.query,
            self.mode,
            self.limit
        );

        Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
    }
}

//******************//
//     Explore      //
//******************//
#[mcp_tool(
    name = "explore",
    description = "Explore codebase architecture, dependencies, and relationships.",
    title = "Explore Codebase Architecture",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true,
    meta = r#"{"category": "analysis", "scope": "architectural"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ExploreTool {
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

impl ExploreTool {
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
//   JulieTools     //
//******************//
// Generates the JulieTools enum with all tool variants
tool_box!(JulieTools, [
    IndexWorkspaceTool,
    SearchCodeTool,
    GotoDefinitionTool,
    FindReferencesTool,
    SemanticSearchTool,
    ExploreTool,
    NavigateTool
]);