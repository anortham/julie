use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use rust_mcp_sdk::macros::mcp_tool;
use rust_mcp_sdk::macros::JsonSchema;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use tracing::{info, debug, warn, error};
use std::path::{Path, PathBuf};
use std::fs;
use std::collections::HashSet;

use crate::handler::JulieServerHandler;
use crate::extractors::Symbol;
use crate::workspace::JulieWorkspace;
use super::shared::{BLACKLISTED_EXTENSIONS, BLACKLISTED_DIRECTORIES};

//******************//
// Index Workspace  //
//******************//
#[mcp_tool(
    name = "index_workspace",
    description = "🚀 UNLOCK JULIE'S POWER - Index workspace to enable lightning-fast search and navigation (ESSENTIAL FIRST STEP)",
    title = "Index Workspace for Code Intelligence",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true,
    meta = r#"{"priority": "high", "category": "initialization"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct IndexWorkspaceTool {
    /// Path to workspace root directory (defaults to current directory).
    /// Examples: ".", "/Users/me/project", "~/Source/myapp", "../other-project"
    /// Julie auto-detects workspace markers (.git, Cargo.toml, package.json, pyproject.toml)
    #[serde(default)]
    pub workspace_path: Option<String>,
    /// Force complete re-indexing even if cache exists (default: false).
    /// Use when: files changed outside Julie, git branch switched, or index seems stale
    /// Warning: Full re-index may take several minutes for large codebases
    #[serde(default)]
    pub force_reindex: Option<bool>,
}

impl IndexWorkspaceTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        info!("📚 Starting workspace indexing...");

        let workspace_path = self.resolve_workspace_path()?;
        let force_reindex = self.force_reindex.unwrap_or(false);

        info!("🎯 Resolved workspace path: {}", workspace_path.display());

        // Initialize or load workspace in handler
        handler.initialize_workspace(Some(workspace_path.to_string_lossy().to_string())).await?;

        // Check if already indexed and not forcing reindex
        if !force_reindex {
            let is_indexed = *handler.is_indexed.read().await;
            if is_indexed {
                let symbol_count = handler.symbols.read().await.len();
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
                *handler.is_indexed.write().await = true;

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
            handler.symbols.write().await.clear();
            handler.relationships.write().await.clear();
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
        let total_symbols = handler.symbols.read().await.len();
        let total_relationships = handler.relationships.read().await.len();

        // CRITICAL FIX: Feed symbols to SearchEngine for fast indexed search
        if total_symbols > 0 {
            info!("⚡ Populating SearchEngine with {} symbols...", total_symbols);
            let symbols = handler.symbols.read().await;
            let symbol_vec: Vec<Symbol> = symbols.clone();
            drop(symbols); // Release the read lock

            let mut search_engine = handler.search_engine.write().await;

            // Index all symbols in SearchEngine
            search_engine.index_symbols(symbol_vec).await.map_err(|e| {
                error!("Failed to populate SearchEngine: {}", e);
                anyhow::anyhow!("SearchEngine indexing failed: {}", e)
            })?;

            // Commit to make symbols searchable
            search_engine.commit().await.map_err(|e| {
                error!("Failed to commit SearchEngine: {}", e);
                anyhow::anyhow!("SearchEngine commit failed: {}", e)
            })?;

            info!("🚀 SearchEngine populated and committed - searches will now be fast!");
        }

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

        // Store in persistent database and search index if workspace is available
        if let Some(workspace) = handler.get_workspace().await? {
            if let Some(db) = &workspace.db {
                let db_lock = db.lock().await;

                // For now, all indexing through the primary index_workspace tool uses 'primary' workspace_id
                // TODO: Phase 4 - support indexing reference workspaces with their own workspace_id
                let workspace_id = "primary";

                // Calculate and store file hash for change detection
                let _file_hash = crate::database::calculate_file_hash(file_path)?;
                let file_info = crate::database::create_file_info(file_path, language)?;
                db_lock.store_file_info(&file_info, workspace_id)?;

                // Store symbols in database
                if let Err(e) = db_lock.store_symbols(&symbols, workspace_id) {
                    warn!("Failed to store symbols in database: {}", e);
                }

                // Store relationships in database
                if let Err(e) = db_lock.store_relationships(&relationships, workspace_id) {
                    warn!("Failed to store relationships in database: {}", e);
                }

                debug!("✅ Stored {} symbols and {} relationships in database",
                       symbols.len(), relationships.len());
            }

            // Also add symbols to search index for fast retrieval
            if let Some(search_index) = &workspace.search {
                let mut search_lock = search_index.write().await;
                if let Err(e) = search_lock.index_symbols(symbols.clone()).await {
                    warn!("Failed to index symbols in search engine: {}", e);
                } else {
                    debug!("✅ Indexed {} symbols in Tantivy search", symbols.len());
                }
            }
        }

        // Store results in handler (compatibility)
        {
            let mut symbol_storage = handler.symbols.write().await;
            symbol_storage.extend(symbols);
        }

        {
            let mut relationship_storage = handler.relationships.write().await;
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