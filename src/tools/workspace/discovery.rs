use crate::tools::shared::{BLACKLISTED_DIRECTORIES, BLACKLISTED_EXTENSIONS};
use crate::tools::workspace::commands::ManageWorkspaceTool;
use crate::utils::ignore::is_ignored_by_pattern;
use anyhow::Result;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

impl ManageWorkspaceTool {
    pub(crate) fn discover_indexable_files(&self, workspace_path: &Path) -> Result<Vec<PathBuf>> {
        let blacklisted_dirs: HashSet<&str> = BLACKLISTED_DIRECTORIES.iter().copied().collect();
        let blacklisted_exts: HashSet<&str> = BLACKLISTED_EXTENSIONS.iter().copied().collect();
        let max_file_size = 1024 * 1024; // 1MB limit for files

        let julieignore_path = workspace_path.join(".julieignore");

        // Check if .julieignore exists, auto-generate if not
        let custom_ignores = if julieignore_path.exists() {
            self.load_julieignore(workspace_path)?
        } else {
            info!("🤖 No .julieignore found - scanning for vendor patterns...");

            // Step 1: Collect ALL files first (including minified for vendor pattern detection)
            // 🔧 BUG FIX: Use empty dir blacklist so we can detect vendor patterns in
            // directories like target/, vendor/, node_modules/ that are normally blacklisted
            let mut all_files = Vec::new();
            self.walk_directory_recursive(
                workspace_path,
                &HashSet::new(), // Empty blacklist - scan everything for vendor detection
                &blacklisted_exts,
                max_file_size,
                &[], // No custom ignores yet
                &mut all_files,
                true, // Skip minified check - need all files for vendor detection
            )?;

            info!(
                "📊 Discovered {} files total after hardcoded filters",
                all_files.len()
            );

            // Step 2: Analyze for vendor patterns
            let detected_patterns = self.analyze_vendor_patterns(&all_files, workspace_path)?;

            // Step 3: Generate .julieignore file if patterns detected
            if !detected_patterns.is_empty() {
                self.generate_julieignore_file(workspace_path, &detected_patterns)?;
                info!(
                    "✅ Generated .julieignore with {} patterns",
                    detected_patterns.len()
                );
                detected_patterns
            } else {
                info!("✨ No vendor patterns detected - project looks clean!");
                Vec::new()
            }
        };

        if !custom_ignores.is_empty() {
            info!(
                "🔍 Loaded {} custom ignore patterns for file discovery",
                custom_ignores.len()
            );
        }

        debug!(
            "🔍 Starting recursive file discovery from: {}",
            workspace_path.display()
        );

        // Now do final discovery with custom ignore patterns applied
        let mut indexable_files = Vec::new();
        self.walk_directory_recursive(
            workspace_path,
            &blacklisted_dirs,
            &blacklisted_exts,
            max_file_size,
            &custom_ignores,
            &mut indexable_files,
            false, // Filter minified files for actual indexing
        )?;

        debug!("📊 File discovery summary:");
        debug!("  - Total indexable files: {}", indexable_files.len());

        Ok(indexable_files)
    }

    /// Recursively walk directory tree, excluding blacklisted paths
    pub(crate) fn walk_directory_recursive(
        &self,
        dir_path: &Path,
        blacklisted_dirs: &HashSet<&str>,
        blacklisted_exts: &HashSet<&str>,
        max_file_size: u64,
        custom_ignores: &[String],
        indexable_files: &mut Vec<PathBuf>,
        skip_minified_check: bool,
    ) -> Result<()> {
        let entries = fs::read_dir(dir_path)
            .map_err(|e| anyhow::anyhow!("Failed to read directory {:?}: {}", dir_path, e))?;

        for entry in entries {
            let entry =
                entry.map_err(|e| anyhow::anyhow!("Failed to read directory entry: {}", e))?;
            let path = entry.path();
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            // Skip hidden files/directories that start with . (except known code files)
            if file_name.starts_with('.') && !self.is_known_dotfile(&path) {
                continue;
            }

            // Check against custom .julieignore patterns
            if is_ignored_by_pattern(&path, custom_ignores) {
                info!("⏭️  Skipping custom-ignored path: {}", path.display());
                continue;
            }

            if path.is_dir() {
                // Check if directory should be blacklisted
                if blacklisted_dirs.contains(file_name) {
                    debug!("⏭️  Skipping blacklisted directory: {}", path.display());
                    continue;
                }

                // Recursively process subdirectory
                self.walk_directory_recursive(
                    &path,
                    blacklisted_dirs,
                    blacklisted_exts,
                    max_file_size,
                    custom_ignores,
                    indexable_files,
                    skip_minified_check,
                )?;
            } else if path.is_file() {
                // Check file extension and size
                if self.should_index_file(
                    &path,
                    blacklisted_exts,
                    max_file_size,
                    skip_minified_check,
                )? {
                    // 🔥 CRITICAL: Canonicalize paths to resolve symlinks (macOS /var -> /private/var)
                    // This ensures file reads work correctly downstream
                    let canonical_path = path.canonicalize().unwrap_or(path);
                    indexable_files.push(canonical_path);
                }
            }
        }

        Ok(())
    }

    /// Check if a file should be indexed based on blacklist and size limits
    pub(crate) fn should_index_file(
        &self,
        file_path: &Path,
        blacklisted_exts: &HashSet<&str>,
        max_file_size: u64,
        skip_minified_check: bool,
    ) -> Result<bool> {
        // Skip minified files (they're generated, not source code)
        // BUT: don't skip when collecting files for vendor pattern analysis
        if !skip_minified_check && self.is_minified_file(file_path) {
            debug!("⏭️  Skipping minified file: {}", file_path.display());
            return Ok(false);
        }

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
            debug!(
                "⏭️  Skipping large file ({} bytes): {}",
                metadata.len(),
                file_path.display()
            );
            return Ok(false);
        }

        // If no extension, check if it's likely a text file by reading first few bytes
        if extension.is_empty() {
            return self.is_likely_text_file(file_path);
        }

        // Index any non-blacklisted file
        Ok(true)
    }

    /// Check if a dotfile is a known configuration file that should be indexed
    pub(crate) fn is_known_dotfile(&self, path: &Path) -> bool {
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        matches!(
            file_name,
            ".gitignore"
                | ".gitattributes"
                | ".editorconfig"
                | ".eslintrc"
                | ".prettierrc"
                | ".babelrc"
                | ".tsconfig"
                | ".jsconfig"
                | ".cargo"
                | ".env"
                | ".npmrc"
                | ".memories" // Development memory checkpoints
        )
    }

    /// Heuristic to determine if a file without extension is likely a text file
    pub(crate) fn is_likely_text_file(&self, file_path: &Path) -> Result<bool> {
        // Read first 512 bytes to check for binary content
        let mut file = fs::File::open(file_path)
            .map_err(|e| anyhow::anyhow!("Failed to open file {:?}: {}", file_path, e))?;

        let mut buffer = [0; 512];
        let bytes_read = file
            .read(&mut buffer)
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
            .filter(|&&b| (32..=126).contains(&b) || b == 9 || b == 10 || b == 13 || b >= 128)
            .count();

        let text_ratio = printable_count as f64 / bytes_read as f64;
        Ok(text_ratio > 0.8) // At least 80% printable characters
    }

    /// Check if a file is minified (generated code we should skip)
    pub(crate) fn is_minified_file(&self, file_path: &Path) -> bool {
        let file_name = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        file_name.contains(".min.")
            || file_name.ends_with(".min.js")
            || file_name.ends_with(".min.css")
            || file_name.ends_with(".bundle.js") // Bundle files are generated/minified
            || file_name.ends_with(".bundle.css")
    }

    /// Load custom ignore patterns from .julieignore file in workspace root
    pub(crate) fn load_julieignore(&self, workspace_path: &Path) -> Result<Vec<String>> {
        let ignore_file = workspace_path.join(".julieignore");

        if !ignore_file.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&ignore_file)
            .map_err(|e| anyhow::anyhow!("Failed to read .julieignore: {}", e))?;

        let patterns: Vec<String> = content
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(|line| line.to_string())
            .collect();

        if !patterns.is_empty() {
            info!(
                "📋 Loaded {} custom ignore patterns from .julieignore",
                patterns.len()
            );
            info!("   Patterns: {:?}", patterns);
        } else {
            info!("⚠️  No patterns found in .julieignore (file exists but all lines filtered out)");
        }

        Ok(patterns)
    }

    /// Analyze files for vendor patterns and return directory paths to exclude
    pub(crate) fn analyze_vendor_patterns(
        &self,
        files: &[PathBuf],
        workspace_root: &Path,
    ) -> Result<Vec<String>> {
        let mut patterns = Vec::new();
        let mut dir_stats: HashMap<PathBuf, DirectoryStats> = HashMap::new();

        // Collect statistics for each directory
        for file in files {
            if let Some(parent) = file.parent() {
                let stats = dir_stats.entry(parent.to_path_buf()).or_default();
                stats.file_count += 1;

                // Check for vendor indicators
                if let Some(name) = file.file_name().and_then(|n| n.to_str()) {
                    if name.contains(".min.") {
                        stats.minified_count += 1;
                    }
                    if name.starts_with("jquery") {
                        stats.jquery_count += 1;
                    }
                    if name.starts_with("bootstrap") {
                        stats.bootstrap_count += 1;
                    }
                }
            }
        }

        // Build a set of all ancestor directories to check (including those with no direct files)
        let mut vendor_candidates: std::collections::HashSet<PathBuf> =
            std::collections::HashSet::new();

        for (dir, _) in &dir_stats {
            // First check the directory itself
            if let Some(dir_name) = dir.file_name().and_then(|n| n.to_str()) {
                // 🔧 BUG FIX: Include all common build output and vendor directories
                // that are normally blacklisted (target, node_modules, vendor, etc.)
                if matches!(
                    dir_name,
                    "libs"
                        | "lib"
                        | "plugin"
                        | "plugins"
                        | "vendor"
                        | "third-party"
                        | "target"
                        | "node_modules"
                        | "build"
                        | "dist"
                        | "out"
                        | "bin"
                        | "obj"
                        | "Debug"
                        | "Release"
                        | "packages"
                        | "bower_components"
                ) {
                    vendor_candidates.insert(dir.to_path_buf());
                }
            }

            // Then check all ancestors of this directory
            let mut current = dir.as_path();
            while let Some(parent) = current.parent() {
                if parent == workspace_root {
                    break;
                }

                if let Some(dir_name) = parent.file_name().and_then(|n| n.to_str()) {
                    if matches!(
                        dir_name,
                        "libs"
                            | "lib"
                            | "plugin"
                            | "plugins"
                            | "vendor"
                            | "third-party"
                            | "target"
                            | "node_modules"
                            | "build"
                            | "dist"
                            | "out"
                            | "bin"
                            | "obj"
                            | "Debug"
                            | "Release"
                            | "packages"
                            | "bower_components"
                    ) {
                        vendor_candidates.insert(parent.to_path_buf());
                    }
                }
                current = parent;
            }
        }

        // For each vendor candidate directory, count files recursively
        for vendor_dir in vendor_candidates {
            let recursive_count: usize = dir_stats
                .iter()
                .filter(|(subdir, _)| subdir.starts_with(&vendor_dir))
                .map(|(_, s)| s.file_count)
                .sum();

            let pattern = self.dir_to_pattern(&vendor_dir, workspace_root);
            info!(
                "🔍 Checking vendor candidate: {} (recursive_count: {})",
                pattern, recursive_count
            );

            if recursive_count > 5 {
                info!(
                    "📦 Detected vendor directory: {} ({} files recursively)",
                    pattern, recursive_count
                );
                patterns.push(pattern);
            }
        }

        // Now check directories in dir_stats for medium-confidence patterns
        for (dir, stats) in &dir_stats {
            // Medium confidence: Lots of vendor-named files
            if stats.jquery_count > 3 || stats.bootstrap_count > 2 {
                let pattern = self.dir_to_pattern(&dir, workspace_root);

                // Skip if already covered by a parent pattern
                if !patterns.iter().any(|p| pattern.starts_with(p)) {
                    info!(
                        "📦 Detected library directory: {} (jquery/bootstrap files)",
                        pattern
                    );
                    patterns.push(pattern);
                }
            }
            // Medium confidence: High concentration of minified files
            else if stats.minified_count > 10 && stats.minified_count > stats.file_count / 2 {
                let pattern = self.dir_to_pattern(&dir, workspace_root);

                // Skip if already covered by a parent pattern
                if !patterns.iter().any(|p| pattern.starts_with(p)) {
                    info!(
                        "📦 Detected minified code directory: {} ({} minified files)",
                        pattern, stats.minified_count
                    );
                    patterns.push(pattern);
                }
            }
        }

        Ok(patterns)
    }

    /// Convert directory path to relative pattern for .julieignore
    pub(crate) fn dir_to_pattern(&self, dir: &Path, workspace_root: &Path) -> String {
        dir.strip_prefix(workspace_root)
            .unwrap_or(dir)
            .to_string_lossy()
            .replace('\\', "/")
    }

    /// Generate .julieignore file with detected patterns and comprehensive documentation
    pub(crate) fn generate_julieignore_file(
        &self,
        workspace_path: &Path,
        patterns: &[String],
    ) -> Result<()> {
        let content = format!(
            r#"# .julieignore - Julie Code Intelligence Exclusion Patterns
# Auto-generated by Julie on {}
#
# ═══════════════════════════════════════════════════════════════
# What Julie Did Automatically
# ═══════════════════════════════════════════════════════════════
# Julie analyzed your project and detected vendor/third-party code patterns.
# These patterns exclude files from:
# • Symbol extraction (function/class definitions)
# • Search indexes (Tantivy full-text search)
#
# Files are still searchable as TEXT using fast_search(mode="text"),
# but won't clutter symbol navigation or semantic search results.
#
# ═══════════════════════════════════════════════════════════════
# Why Exclude Vendor Code?
# ═══════════════════════════════════════════════════════════════
# 1. Search Quality: Prevents vendor code from polluting search results
# 2. Performance: Skips symbol extraction for thousands of vendor functions
# 3. Relevance: Semantic search focuses on YOUR code, not libraries
#
# ═══════════════════════════════════════════════════════════════
# How to Modify This File
# ═══════════════════════════════════════════════════════════════
# • Add patterns: Just add new lines with glob patterns (gitignore syntax)
# • Remove patterns: Delete lines or comment out with #
# • Check impact: Use manage_workspace(operation="health")
#
# FALSE POSITIVE? If Julie excluded something important:
# 1. Delete or comment out the pattern below
# 2. Julie will automatically reindex on next file change
#
# DISABLE AUTO-GENERATION: Create this file manually before first run
#
# ═══════════════════════════════════════════════════════════════
# Auto-Detected Vendor Directories
# ═══════════════════════════════════════════════════════════════
{}

# ═══════════════════════════════════════════════════════════════
# Common Patterns (Uncomment if needed in your project)
# ═══════════════════════════════════════════════════════════════
# *.min.js
# *.min.css
# jquery*.js
# bootstrap*.js
# angular*.js

# ═══════════════════════════════════════════════════════════════
# Debugging: If Search Isn't Finding Files
# ═══════════════════════════════════════════════════════════════
# Use manage_workspace(operation="health") to see:
# • How many files are excluded by each pattern
# • Whether patterns are too broad
#
# If a pattern excludes files it shouldn't, comment it out or make
# it more specific (e.g., "**/vendor/lib/**" vs "**/lib/**")
"#,
            chrono::Local::now().format("%Y-%m-%d"),
            patterns
                .iter()
                .map(|p| format!("{}/", p))
                .collect::<Vec<_>>()
                .join("\n")
        );

        std::fs::write(workspace_path.join(".julieignore"), content)?;
        info!("📝 Created .julieignore - review and commit to version control");

        Ok(())
    }
}

/// Statistics for analyzing vendor code patterns in a directory
#[derive(Default)]
struct DirectoryStats {
    file_count: usize,
    minified_count: usize,
    jquery_count: usize,
    bootstrap_count: usize,
}
