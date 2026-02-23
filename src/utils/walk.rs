use crate::tools::shared::BLACKLISTED_DIRECTORIES;
use ignore::WalkBuilder;
use std::collections::HashSet;
use std::path::Path;

/// Configuration for how the walker filters entries.
pub struct WalkConfig {
    pub use_julieignore: bool,
    pub use_blacklisted_dirs: bool,
}

impl WalkConfig {
    /// Vendor detection: gitignore ON, blacklisted dirs OFF, no julieignore.
    /// Used for Phase 1 of discovery — scanning for vendor patterns before
    /// .julieignore exists.
    pub fn vendor_scan() -> Self {
        Self {
            use_julieignore: false,
            use_blacklisted_dirs: false,
        }
    }

    /// Full indexing: gitignore ON, julieignore ON, blacklisted dirs ON.
    /// Used for Phase 2 of discovery — the final file list for indexing.
    pub fn full_index() -> Self {
        Self {
            use_julieignore: true,
            use_blacklisted_dirs: true,
        }
    }

    /// Stale file scanning: same config as full_index.
    /// Used by startup.rs to detect changed files.
    pub fn stale_scan() -> Self {
        Self::full_index()
    }
}

/// Build an `ignore`-crate Walk iterator for the given workspace and config.
///
/// Configures:
/// - `hidden(false)` — include dotfiles; let .gitignore + blacklist handle exclusion
/// - `git_ignore(true)` — respect .gitignore (including nested, global, .git/info/exclude)
/// - `.julieignore` — if `config.use_julieignore`, added as custom ignore filename
/// - `filter_entry` — always excludes `.git`; optionally excludes BLACKLISTED_DIRECTORIES
pub fn build_walker(workspace_path: &Path, config: &WalkConfig) -> ignore::Walk {
    let mut builder = WalkBuilder::new(workspace_path);

    builder
        .hidden(false) // Include dotfiles — we filter .git explicitly
        .git_ignore(true) // Respect .gitignore (nested, global, .git/info/exclude)
        .git_global(true)
        .git_exclude(true)
        .follow_links(false)
        .ignore(false); // Don't read .ignore files — only .gitignore + .julieignore

    if config.use_julieignore {
        builder.add_custom_ignore_filename(".julieignore");
    }

    let blacklisted_dirs: HashSet<&'static str> = if config.use_blacklisted_dirs {
        BLACKLISTED_DIRECTORIES.iter().copied().collect()
    } else {
        HashSet::new()
    };

    builder.filter_entry(move |entry| {
        let file_name = entry.file_name().to_str().unwrap_or("");

        // Always exclude .git — hidden(false) would otherwise include it.
        // See: https://github.com/BurntSushi/ripgrep/issues/3099
        if file_name == ".git" {
            return false;
        }

        // Filter blacklisted directories when enabled
        if !blacklisted_dirs.is_empty()
            && entry.file_type().map_or(false, |ft| ft.is_dir())
            && blacklisted_dirs.contains(file_name)
        {
            return false;
        }

        true
    });

    builder.build()
}
